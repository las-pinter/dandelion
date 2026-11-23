//! Entry point of the `dandelion` binary.

use std::{net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use dandelion_core::BuildInfo;
use dandelion_server::{
    app,
    audit::{AuditEntry, AuditLog},
    auth::{
        Auth, SETUP_TOKEN_TTL_MS,
        origin::PublicOrigin,
        session::now_millis,
        store::AuthStore,
        token::{hash_token, random_token},
    },
    config::{self, BindAddr},
    openapi::ApiDoc,
    shutdown,
    state::AppState,
    storage,
};
use sqlx::SqlitePool;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;
use utoipa::OpenApi;

const BUILD: BuildInfo = BuildInfo::new("dandelion", env!("CARGO_PKG_VERSION"));

/// Monitor and control this development machine from the browser.
#[derive(Debug, Parser)]
#[command(name = "dandelion", version = BUILD.version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Serve the API and the embedded web application.
    Serve {
        /// Loopback address to bind. Non-loopback addresses are refused.
        #[arg(long, default_value_t = BindAddr::DEFAULT, value_parser = clap::value_parser!(BindAddr))]
        bind: BindAddr,
        /// Data directory for the SQLite database.
        #[arg(long)]
        data_dir: Option<PathBuf>,
    },
    /// Manage authentication from this machine.
    Auth {
        #[command(subcommand)]
        command: AuthCommand,
    },
    /// Print the `OpenAPI` document as JSON.
    Openapi,
    /// Verify that the embedded web application is present.
    Assets,
}

#[derive(Debug, Subcommand)]
enum AuthCommand {
    /// Wipe every credential and session, then print a fresh setup link.
    Reset {
        /// Data directory for the SQLite database.
        #[arg(long)]
        data_dir: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing();

    match cli.command {
        None => run_server(BindAddr::DEFAULT, None).await,
        Some(Command::Serve { bind, data_dir }) => run_server(bind, data_dir).await,
        Some(Command::Auth { command }) => run_auth(command).await,
        Some(Command::Openapi) => print_openapi(),
        Some(Command::Assets) => check_assets(),
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

fn print_openapi() -> Result<()> {
    let document = ApiDoc::openapi();
    println!("{}", serde_json::to_string_pretty(&document)?);
    Ok(())
}

fn check_assets() -> Result<()> {
    if dandelion_server::static_files::bundle_is_embedded() {
        println!("embedded web UI present");
        return Ok(());
    }

    anyhow::bail!("embedded web UI is empty; build web/dist before the Rust binary")
}

async fn run_server(bind: BindAddr, data_dir: Option<PathBuf>) -> Result<()> {
    let (_data_dir, pool) = open_database(data_dir).await?;
    let origin = PublicOrigin::resolve().await.context(
        "resolve the public origin; set DANDELION_PUBLIC_URL when Tailscale is unavailable",
    )?;
    tracing::info!(origin = %origin.as_str(), "public origin");

    let auth = Auth::new(pool.clone(), origin).context("configure authentication")?;
    let audit = AuditLog::new(pool);

    if !auth.setup_complete().await.context("read setup state")? {
        let token = auth
            .issue_setup_token(now_millis())
            .await
            .context("issue a setup token")?;
        audit
            .record(AuditEntry {
                actor: "system",
                action: "auth.setup.token",
                target: "owner",
                detail: None,
            })
            .await
            .context("audit the setup token")?;
        println!(
            "Dandelion setup is required. Open: {}/setup?token={token}",
            auth.origin().as_str()
        );
    }

    serve(bind, AppState::new(audit, auth)).await
}

async fn run_auth(command: AuthCommand) -> Result<()> {
    match command {
        AuthCommand::Reset { data_dir } => reset_credentials(data_dir).await,
    }
}

async fn reset_credentials(data_dir: Option<PathBuf>) -> Result<()> {
    // Resolve the origin before destroying anything: a failed reset must leave
    // the existing credentials in place, and the operator always gets a full
    // setup URL.
    let origin = PublicOrigin::resolve().await.context(
        "resolve the public origin; set DANDELION_PUBLIC_URL when Tailscale is unavailable",
    )?;
    let (_data_dir, pool) = open_database(data_dir).await?;
    let store = AuthStore::new(pool.clone());
    store.wipe_auth().await.context("wipe credentials")?;

    let token = random_token().context("generate a setup token")?;
    let now = now_millis();
    store
        .insert_setup_token(&hash_token(&token), now, now + SETUP_TOKEN_TTL_MS)
        .await
        .context("store the setup token")?;
    AuditLog::new(pool)
        .record(AuditEntry {
            actor: "cli",
            action: "auth.reset",
            target: "owner",
            detail: None,
        })
        .await
        .context("audit the reset")?;

    println!(
        "Dandelion credentials reset. Open: {}/setup?token={token}",
        origin.as_str()
    );
    println!("The setup link is single-use and expires in 15 minutes.");

    Ok(())
}

async fn open_database(data_dir: Option<PathBuf>) -> Result<(PathBuf, SqlitePool)> {
    let data_dir = config::data_dir(data_dir)?;
    let database = config::database_path(&data_dir);
    let pool = storage::open(&database)
        .await
        .with_context(|| format!("open database {}", database.display()))?;
    tracing::info!(path = %database.display(), "database ready");
    Ok((data_dir, pool))
}

async fn serve(bind: BindAddr, state: AppState) -> Result<()> {
    let listener = TcpListener::bind(bind.socket_addr())
        .await
        .with_context(|| format!("bind {bind}"))?;
    let address: SocketAddr = listener.local_addr().context("read the bound address")?;
    tracing::info!(%address, "dandelion listening");

    axum::serve(listener, app::build_router(state))
        .with_graceful_shutdown(shutdown::shutdown_signal())
        .await
        .context("serve HTTP")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use clap::{CommandFactory, Parser};

    use super::{AuthCommand, BindAddr, Cli, Command};

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn default_command_is_serve() {
        let cli = Cli::try_parse_from(["dandelion"]).expect("cli parses");

        assert!(cli.command.is_none());
    }

    #[test]
    fn bind_flag_accepts_loopback() {
        let cli = Cli::try_parse_from(["dandelion", "serve", "--bind", "127.0.0.1:7443"])
            .expect("cli parses");

        assert!(matches!(cli.command, Some(Command::Serve { .. })));
    }

    #[test]
    fn bind_flag_rejects_non_loopback() {
        let result = Cli::try_parse_from(["dandelion", "serve", "--bind", "0.0.0.0:7443"]);

        assert!(result.is_err());
    }

    #[test]
    fn serve_defaults_to_loopback() {
        let cli = Cli::try_parse_from(["dandelion", "serve"]).expect("cli parses");
        let Some(Command::Serve { bind, .. }) = cli.command else {
            panic!("serve subcommand expected");
        };

        assert_eq!(bind, BindAddr::DEFAULT);
    }

    #[test]
    fn data_dir_flag_is_accepted() {
        let cli = Cli::try_parse_from(["dandelion", "serve", "--data-dir", "/tmp/dandelion"])
            .expect("cli parses");
        let Some(Command::Serve { data_dir, .. }) = cli.command else {
            panic!("serve subcommand expected");
        };

        assert_eq!(data_dir, Some(PathBuf::from("/tmp/dandelion")));
        assert_eq!(data_dir.as_deref(), Some(Path::new("/tmp/dandelion")));
    }

    #[test]
    fn auth_reset_is_a_subcommand() {
        let cli =
            Cli::try_parse_from(["dandelion", "auth", "reset", "--data-dir", "/tmp/dandelion"])
                .expect("cli parses");
        let Some(Command::Auth {
            command: AuthCommand::Reset { data_dir },
        }) = cli.command
        else {
            panic!("auth reset subcommand expected");
        };

        assert_eq!(data_dir, Some(PathBuf::from("/tmp/dandelion")));
    }

    #[test]
    fn auth_requires_a_subcommand() {
        assert!(Cli::try_parse_from(["dandelion", "auth"]).is_err());
    }
}
