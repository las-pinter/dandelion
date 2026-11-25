//! Entry point of the `dandelion` binary.

use std::{
    net::SocketAddr,
    num::NonZeroUsize,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use dandelion_core::BuildInfo;
use dandelion_server::{
    agents::{self, Agents},
    app,
    audit::{AuditEntry, AuditLog},
    auth::{
        Auth, SETUP_TOKEN_TTL_MS,
        origin::PublicOrigin,
        session::now_millis,
        store::AuthStore,
        token::{hash_token, random_token},
    },
    backup::{self, DEFAULT_KEEP},
    claude::ClaudeMonitor,
    config::{self, BindAddr},
    containers::Containers,
    metrics::{self, Metrics},
    openapi::ApiDoc,
    services::{self, Services},
    shutdown,
    state::AppState,
    storage,
    ws::hub::Hub,
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
    /// Write the WebSocket TypeScript bindings.
    TsTypes {
        /// Directory for the generated `.ts` files.
        #[arg(default_value = "web/src/api/gen/ws")]
        out_dir: PathBuf,
    },
    /// Verify that the embedded web application is present.
    Assets,
    /// Print the sudoers drop-in generated from the allowlist file.
    Sudoers {
        /// Data directory that holds sudo-allowlist.toml.
        #[arg(long)]
        data_dir: Option<PathBuf>,
        /// Account name for the drop-in; defaults to `$USER`.
        #[arg(long)]
        user: Option<String>,
    },
    /// Copy the live database to a consistent backup.
    Backup {
        /// Data directory that holds the database.
        #[arg(long)]
        data_dir: Option<PathBuf>,
        /// Destination directory; defaults to `<data-dir>/backups`.
        #[arg(long)]
        out_dir: Option<PathBuf>,
        /// How many backups to keep, newest first.
        #[arg(long, default_value_t = DEFAULT_KEEP)]
        keep: NonZeroUsize,
    },
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
        Some(Command::TsTypes { out_dir }) => export_ts_types(&out_dir),
        Some(Command::Assets) => check_assets(),
        Some(Command::Sudoers { data_dir, user }) => print_sudoers(data_dir, user),
        Some(Command::Backup {
            data_dir,
            out_dir,
            keep,
        }) => run_backup(data_dir, out_dir, keep).await,
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

fn export_ts_types(out_dir: &Path) -> Result<()> {
    use dandelion_server::ws::protocol::{ClientMessage, ServerMessage};
    use ts_rs::TS;

    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("create the bindings directory {}", out_dir.display()))?;
    let config = ts_rs::Config::new().with_out_dir(out_dir);
    ClientMessage::export_all(&config).context("export ClientMessage")?;
    ServerMessage::export_all(&config).context("export ServerMessage")?;
    println!("websocket bindings written to {}", out_dir.display());
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
    let (data_dir, pool) = open_database(data_dir).await?;
    remove_retired_vapid_key(&data_dir);
    let origin = PublicOrigin::resolve().await.context(
        "resolve the public origin; set DANDELION_PUBLIC_URL when Tailscale is unavailable",
    )?;
    tracing::info!(origin = %origin.as_str(), "public origin");

    let auth = Auth::new(pool.clone(), origin).context("configure authentication")?;
    let audit = AuditLog::new(pool.clone());
    let metrics = Metrics::new(pool.clone());

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

    let allowlist_path = config::allowlist_path(&data_dir);
    let allowlist = services::load_allowlist(&allowlist_path)
        .with_context(|| format!("load the sudo allowlist {}", allowlist_path.display()))?;
    tracing::info!(
        path = %allowlist_path.display(),
        rules = allowlist.rules().len(),
        "sudo allowlist"
    );
    let services = Services::connect(allowlist).await;
    let containers = Containers::connect();
    let agents = Agents::connect().await;
    let claude = ClaudeMonitor::from_home();
    tracing::info!(enabled = claude.is_enabled(), "Claude Code session monitor");

    let state = AppState::new(
        audit,
        auth,
        Hub::default(),
        metrics,
        services,
        containers,
        agents,
        claude,
    );
    metrics::collector::spawn(state.clone());
    agents::spawn_publisher(state.clone());
    dandelion_server::claude::spawn_monitor(state.clone());
    serve(bind, state).await
}

fn print_sudoers(data_dir: Option<PathBuf>, user: Option<String>) -> Result<()> {
    let data_dir = config::data_dir(data_dir)?;
    let allowlist_path = config::allowlist_path(&data_dir);
    let allowlist = services::load_allowlist(&allowlist_path)
        .with_context(|| format!("load the sudo allowlist {}", allowlist_path.display()))?;
    if allowlist.is_empty() {
        anyhow::bail!(
            "the allowlist {} has no rules; add a [[rules]] entry first",
            allowlist_path.display()
        );
    }
    let user = user
        .or_else(|| std::env::var("USER").ok())
        .filter(|value| !value.is_empty())
        .context("pass --user or set USER")?;
    print!("{}", services::sudoers_drop_in(&allowlist, &user)?);
    Ok(())
}

async fn run_backup(
    data_dir: Option<PathBuf>,
    out_dir: Option<PathBuf>,
    keep: NonZeroUsize,
) -> Result<()> {
    let data_dir = config::data_dir(data_dir)?;
    let path = backup::backup_database(&data_dir, out_dir.as_deref(), keep)
        .await
        .context("back up the database")?;
    println!("backup written: {}", path.display());
    Ok(())
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

/// Deletes the VAPID key an install from before the push removal left behind.
///
/// Web push is gone (ADR-0005), so the key alone would grant nothing; it is
/// still private key material and should not sit in the data directory. Best
/// effort: a missing file is the normal case, and any other failure is logged
/// rather than blocking startup.
fn remove_retired_vapid_key(data_dir: &Path) {
    let key = data_dir.join("vapid.json");
    match std::fs::remove_file(&key) {
        Ok(()) => tracing::info!(path = %key.display(), "removed the retired web push key"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::warn!(path = %key.display(), %error, "could not remove the retired web push key");
        }
    }
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

    use super::{AuthCommand, BindAddr, Cli, Command, remove_retired_vapid_key};

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

    #[test]
    fn backup_defaults_to_seven_copies() {
        let cli = Cli::try_parse_from(["dandelion", "backup"]).expect("cli parses");
        let Some(Command::Backup { keep, out_dir, .. }) = cli.command else {
            panic!("backup subcommand expected");
        };

        assert_eq!(keep.get(), 7);
        assert!(out_dir.is_none());
    }

    #[test]
    fn backup_rejects_keeping_nothing() {
        assert!(Cli::try_parse_from(["dandelion", "backup", "--keep", "0"]).is_err());
    }

    #[test]
    fn remove_retired_vapid_key_deletes_the_key() {
        let dir = tempfile::tempdir().expect("temp dir");
        let key = dir.path().join("vapid.json");
        std::fs::write(&key, b"{\"private_key\":\"retired\"}").expect("write the key");

        remove_retired_vapid_key(dir.path());

        assert!(!key.exists());
    }

    #[test]
    fn remove_retired_vapid_key_ignores_a_missing_key() {
        let dir = tempfile::tempdir().expect("temp dir");
        let neighbour = dir.path().join("sudo-allowlist.toml");
        std::fs::write(&neighbour, b"[[rules]]\n").expect("write a neighbour file");

        remove_retired_vapid_key(dir.path());
        remove_retired_vapid_key(dir.path());

        assert!(neighbour.exists());
    }
}
