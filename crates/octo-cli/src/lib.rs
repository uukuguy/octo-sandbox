//! Octo CLI library — exposes dashboard and other modules for reuse.
//!
//! The primary consumer is `octo-desktop`, which needs `commands::dashboard::build_router`
//! to embed the dashboard HTTP server inside the Tauri app.

use clap::Parser;

use commands::{
    AgentCommands, CompletionsCommands, ConfigCommands, EvalCommands, McpCommands, MemoryCommands,
    SessionCommands, SkillCommands, ToolsCommands,
};

pub mod commands;
pub mod dashboard;
pub mod output;
pub mod repl;
pub mod tui;
pub(crate) mod ui;

// ── CLI Argument Definitions ────────────────────────────────────────

#[derive(Parser)]
#[command(name = "octo")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Octo — AI Agent Workbench CLI", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Path to configuration file
    #[arg(short, long, global = true, default_value = "config.yaml")]
    pub config: String,

    /// Database path (overrides config)
    #[arg(short, long, global = true)]
    pub db: Option<String>,

    /// Output format (text, json, table)
    #[arg(long, global = true, default_value = "text")]
    pub output: String,

    /// Disable color output
    #[arg(long, global = true)]
    pub no_color: bool,

    /// Suppress non-essential output
    #[arg(short, long, global = true)]
    pub quiet: bool,
}

#[derive(Parser)]
pub enum Commands {
    /// Start interactive REPL session
    Run {
        /// Resume last session
        #[arg(short = 'C', long = "continue")]
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
        /// Additional directories to include as context
        #[arg(long = "add-dir")]
        add_dirs: Vec<String>,
        /// Enable dual agent mode (Plan + Build agents)
        #[arg(long)]
        dual: bool,
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

    /// Manage skills
    Skill {
        #[command(subcommand)]
        action: SkillCommands,
    },

    /// Evaluation management
    Eval {
        #[command(subcommand)]
        action: EvalCommands,
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

    /// Launch embedded web dashboard
    Dashboard {
        /// Port to listen on
        #[arg(short, long, default_value = "8080")]
        port: u16,
        /// Host to bind to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Open browser on start
        #[arg(long)]
        open: bool,
        /// Enable TLS/HTTPS
        #[arg(long)]
        enable_tls: bool,
        /// Path to TLS certificate (PEM format)
        #[arg(long)]
        cert_path: Option<String>,
        /// Path to TLS private key (PEM format)
        #[arg(long)]
        key_path: Option<String>,
        /// Require API key authentication for protected endpoints
        #[arg(long)]
        require_auth: bool,
        /// Allowed CORS origins (comma-separated)
        #[arg(long, value_delimiter = ',')]
        allowed_origins: Vec<String>,
        /// Generate self-signed TLS certificate for development
        #[arg(long)]
        generate_cert: bool,
    },
}
