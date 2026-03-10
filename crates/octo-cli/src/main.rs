//! Octo CLI - Local CLI for interacting with Octo agents
//!
//! This CLI provides commands for managing agents, sessions, memories, and tools.

use anyhow::Result;
use clap::Parser;
use tracing::info;
use tracing_subscriber::{fmt, EnvFilter};

mod commands;
mod output;
mod repl;
mod tui;
mod ui;

use commands::{
    execute_ask, execute_run, generate_completions, handle_agent, handle_config, handle_mcp,
    handle_memory, handle_session, handle_tools, run_doctor, AgentCommands, AppState,
    CompletionsCommands, ConfigCommands, McpCommands, MemoryCommands, SessionCommands,
    ToolsCommands,
};

#[derive(Parser)]
#[command(name = "octo")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Octo — AI Agent Workbench CLI", long_about = None)]
pub(crate) struct Cli {
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

    /// Output format (text, json, table)
    #[arg(long, global = true, default_value = "text")]
    output: String,

    /// Disable color output
    #[arg(long, global = true)]
    no_color: bool,

    /// Suppress non-essential output
    #[arg(short, long, global = true)]
    quiet: bool,
}

#[derive(Parser)]
enum Commands {
    /// Start interactive REPL session
    Run {
        /// Resume last session
        #[arg(short = 'c', long = "continue")]
        resume: bool,
        /// Resume specific session
        #[arg(short, long)]
        session: Option<String>,
        /// Use specific agent
        #[arg(short, long)]
        agent: Option<String>,
        /// Color theme
        #[arg(long, default_value = "cyan")]
        theme: String,
    },

    /// Send a single query (headless mode)
    Ask {
        /// The message to send
        #[arg(value_name = "MESSAGE")]
        message: String,
        /// Use specific session
        #[arg(short, long)]
        session: Option<String>,
        /// Use specific agent
        #[arg(short, long)]
        agent: Option<String>,
    },

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
    #[command(name = "tool")]
    Tool {
        #[command(subcommand)]
        action: ToolsCommands,
    },

    /// Manage MCP servers
    Mcp {
        #[command(subcommand)]
        action: McpCommands,
    },

    /// Configuration management
    Config {
        #[command(subcommand)]
        action: ConfigCommands,
    },

    /// Run health diagnostics
    Doctor {
        /// Attempt to fix issues automatically
        #[arg(long)]
        repair: bool,
    },

    /// Start full-screen TUI mode
    Tui {
        /// Color theme
        #[arg(long, default_value = "cyan")]
        theme: String,
    },

    /// Generate shell completions
    Completions {
        #[command(subcommand)]
        action: CompletionsCommands,
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

    // Build output config from CLI flags
    let output_config = output::OutputConfig {
        format: match cli.output.as_str() {
            "json" => output::OutputFormat::Json,
            "stream-json" => output::OutputFormat::StreamJson,
            _ => output::OutputFormat::Text,
        },
        color: !cli.no_color && std::io::IsTerminal::is_terminal(&std::io::stdout()),
        quiet: cli.quiet,
    };

    // Initialize app state
    let state = AppState::new(db_path.into(), output_config).await?;

    match cli.command {
        Commands::Run {
            resume,
            session,
            agent,
            theme,
        } => {
            execute_run(
                commands::run::RunOptions {
                    resume,
                    session_id: session,
                    agent_id: agent,
                    theme,
                },
                &state,
            )
            .await?;
        }
        Commands::Ask {
            message,
            session,
            agent,
        } => {
            execute_ask(
                commands::ask::AskOptions {
                    message,
                    session_id: session,
                    agent_id: agent,
                },
                &state,
            )
            .await?;
        }
        Commands::Agent { action } => handle_agent(action, &state).await?,
        Commands::Session { action } => handle_session(action, &state).await?,
        Commands::Memory { action } => handle_memory(action, &state).await?,
        Commands::Tool { action } => handle_tools(action, &state).await?,
        Commands::Mcp { action } => handle_mcp(action, &state).await?,
        Commands::Config { action } => handle_config(action, &state).await?,
        Commands::Tui { theme } => {
            let theme_name = theme.parse().unwrap_or_default();
            tui::run_tui(state, theme_name).await?;
        }
        Commands::Doctor { repair } => run_doctor(repair, &state).await?,
        Commands::Completions { action } => match action {
            CompletionsCommands::Generate { shell } => generate_completions(shell)?,
        },
    }

    Ok(())
}
