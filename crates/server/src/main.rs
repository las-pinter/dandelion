//! Entry point of the `dandelion` binary.

use std::{net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use dandelion_core::BuildInfo;
use dandelion_server::{
    app,
    audit::AuditLog,
    config::{self, BindAddr},
    openapi::ApiDoc,
    shutdown,
    state::AppState,
    storage,
};
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
    /// Print the `OpenAPI` document as JSON.
    Openapi,
    /// Verify that the embedded web application is present.
    Assets,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing();

    let (bind, data_dir) = match cli.command {
        None => (BindAddr::DEFAULT, None),
        Some(Command::Serve { bind, data_dir }) => (bind, data_dir),
        Some(Command::Openapi) => {
            print_openapi()?;
            return Ok(());
        }
        Some(Command::Assets) => {
            check_assets()?;
            return Ok(());
        }
    };

    let data_dir = config::data_dir(data_dir)?;
    let database = config::database_path(&data_dir);
    let pool = storage::open(&database)
        .await
        .with_context(|| format!("open database {}", database.display()))?;
    tracing::info!(path = %database.display(), "database ready");
    let state = AppState::new(AuditLog::new(pool));

    serve(bind, state).await
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

    use super::{BindAddr, Cli, Command};

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
}
