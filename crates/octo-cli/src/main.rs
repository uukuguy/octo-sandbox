//! Octo CLI - Local CLI for interacting with Octo agents
//!
//! This CLI provides commands for managing agents, sessions, memories, and tools.

use anyhow::Result;
use clap::Parser;
use tracing::info;
use tracing_subscriber::{fmt, EnvFilter};

mod commands;

use commands::{
    handle_agent, handle_config, handle_memory, handle_session, handle_tools, AgentCommands,
    AppState, ConfigCommands, MemoryCommands, SessionCommands, ToolsCommands,
};

#[derive(Parser)]
#[command(name = "octo")]
#[command(version = "0.1.0")]
#[command(about = "Octo CLI - Local CLI for interacting with Octo agents", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Path to configuration file
    #[arg(short, long, global = true, default_value = "config.yaml")]
    config: String,

    /// Database path (overrides config)
    #[arg(short, long, global = true)]
    db: Option<String>,
}

#[derive(Parser)]
enum Commands {
    /// Manage agents
    Agent {
        #[command(subcommand)]
        action: AgentCommands,
    },

    /// Manage sessions
    Session {
        #[command(subcommand)]
        action: SessionCommands,
    },

    /// Manage memory
    Memory {
        #[command(subcommand)]
        action: MemoryCommands,
    },

    /// Manage tools
    Tools {
        #[command(subcommand)]
        action: ToolsCommands,
    },

    /// Configuration management
    Config {
        #[command(subcommand)]
        action: ConfigCommands,
    },
}

fn init_logging(verbose: bool) {
    let filter = if verbose {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"))
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
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
    // Load .env file if present
    dotenvy::dotenv().ok();

    let cli = Cli::parse();
    init_logging(cli.verbose);

    info!("Starting Octo CLI");

    // Determine database path
    let db_path = cli.db.unwrap_or_else(|| {
        // Try to get from config or use default
        std::env::var("OCTO_DB_PATH").unwrap_or_else(|_| "octo.db".to_string())
    });

    // Initialize app state
    let state = AppState::new(db_path.into()).await?;

    match cli.command {
        Commands::Agent { action } => handle_agent(action, &state).await?,
        Commands::Session { action } => handle_session(action, &state).await?,
        Commands::Memory { action } => handle_memory(action, &state).await?,
        Commands::Tools { action } => handle_tools(action, &state).await?,
        Commands::Config { action } => handle_config(action, &state).await?,
    }

    Ok(())
}
