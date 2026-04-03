//! Grid Studio - Full-screen TUI + Web Dashboard for Grid agents
//!
//! This is the studio binary (`grid-studio`), providing:
//! - Full-screen TUI mode (default)
//! - Embedded web dashboard (--web)
//!
//! Requires the "studio" feature to be enabled.

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::{fmt, EnvFilter};

use grid_cli::commands::{self, AppState};
use grid_cli::output;
use grid_cli::tui;
use grid_cli::StudioCommands;

#[derive(Parser)]
#[command(name = "grid-studio")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Grid Studio — Autonomous AI Agent Workbench", long_about = None)]
struct StudioCli {
    #[command(subcommand)]
    command: Option<StudioCommands>,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Database path (overrides config)
    #[arg(short, long, global = true)]
    db: Option<String>,

    /// Target project directory
    #[arg(short = 'P', long, global = true)]
    project: Option<String>,

    /// Color theme
    #[arg(long, default_value = "indigo")]
    theme: String,
}

fn init_logging_tui(verbose: bool) {
    let filter = if verbose {
        EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("grid_cli=info,grid_engine=debug"))
    } else {
        EnvFilter::new("grid_cli=warn,grid_engine=warn")
    };

    // TUI mode: redirect tracing to log file to avoid corrupting ratatui screen
    let log_dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("grid");
    let _ = std::fs::create_dir_all(&log_dir);
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("tui.log"))
        .expect("Failed to open TUI log file");
    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(true)
        .with_line_number(true)
        .with_ansi(false)
        .with_writer(std::sync::Mutex::new(log_file))
        .init();
}

fn init_logging_dashboard(verbose: bool) {
    let filter = if verbose {
        EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("grid_cli=info,grid_engine=debug"))
    } else {
        EnvFilter::new("grid_cli=warn,grid_engine=warn")
    };

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(true)
        .with_line_number(true)
        .init();
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let cli = StudioCli::parse();
    let is_dashboard = matches!(cli.command, Some(StudioCommands::Dashboard { .. }));

    if is_dashboard {
        init_logging_dashboard(cli.verbose);
    } else {
        init_logging_tui(cli.verbose);
    }

    let grid_root = if let Some(ref project_path) = cli.project {
        grid_engine::GridRoot::with_project_dir(project_path)?
    } else {
        grid_engine::GridRoot::discover().unwrap_or_else(|e| {
            tracing::warn!("Failed to discover GridRoot: {}, using defaults", e);
            let wd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            grid_engine::GridRoot::with_working_dir(&wd).expect("GridRoot fallback failed")
        })
    };

    if let Err(e) = grid_root.ensure_dirs() {
        tracing::warn!("Failed to ensure GridRoot directories: {}", e);
    }

    let db_path = cli.db.unwrap_or_else(|| {
        grid_root.resolve_db_path().to_string_lossy().to_string()
    });

    let output_config = output::OutputConfig {
        format: output::OutputFormat::Text,
        color: true,
        quiet: false,
    };

    let state = AppState::with_grid_root(db_path.into(), output_config, grid_root).await?;

    match cli.command {
        Some(StudioCommands::Dashboard {
            port, host, open, enable_tls, cert_path, key_path,
            require_auth, allowed_origins, generate_cert,
        }) => {
            let (cert_path, key_path, tls_enabled) = if generate_cert {
                let cert_dir = state.grid_root.tls_dir();
                let (cp, kp) = commands::dashboard_cert::generate_self_signed_cert(&cert_dir)?;
                (Some(cp), Some(kp), true)
            } else {
                (cert_path, key_path, enable_tls)
            };

            let opts = commands::dashboard::DashboardOptions {
                port,
                host,
                open,
                tls_enabled,
                cert_path,
                key_path,
                require_auth,
                allowed_origins,
            };
            commands::dashboard::run_dashboard(&opts).await?;
        }
        Some(StudioCommands::Tui { .. }) | None => {
            // Default: launch TUI
            tui::run_tui_conversation(&state).await?;
        }
    }

    Ok(())
}
