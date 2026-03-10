//! Command type definitions for Octo CLI

use clap::Subcommand;

/// Agent subcommands
#[derive(Subcommand)]
pub enum AgentCommands {
    /// List all available agents
    List,
    /// Show agent details
    Info {
        /// Agent ID
        #[arg(value_name = "AGENT_ID")]
        agent_id: String,
    },
    /// Create a new agent
    Create {
        /// Agent name
        #[arg(value_name = "NAME")]
        name: String,
        /// Agent role
        #[arg(short, long)]
        role: Option<String>,
        /// Agent goal
        #[arg(short, long)]
        goal: Option<String>,
    },
    /// Start an agent
    Start {
        /// Agent ID
        #[arg(value_name = "AGENT_ID")]
        agent_id: String,
    },
    /// Pause an agent
    Pause {
        /// Agent ID
        #[arg(value_name = "AGENT_ID")]
        agent_id: String,
    },
    /// Stop an agent
    Stop {
        /// Agent ID
        #[arg(value_name = "AGENT_ID")]
        agent_id: String,
    },
    /// Delete an agent
    Delete {
        /// Agent ID
        #[arg(value_name = "AGENT_ID")]
        agent_id: String,
    },
}

/// Session subcommands
#[derive(Subcommand)]
pub enum SessionCommands {
    /// List all sessions
    List {
        /// Maximum results
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },
    /// Create a new session
    Create {
        /// Session name (optional)
        #[arg(short, long)]
        name: Option<String>,
    },
    /// Show session details
    Show {
        /// Session ID
        #[arg(value_name = "SESSION_ID")]
        session_id: String,
    },
    /// Delete a session
    Delete {
        /// Session ID
        #[arg(value_name = "SESSION_ID")]
        session_id: String,
    },
    /// Export a session
    Export {
        /// Session ID
        #[arg(value_name = "SESSION_ID")]
        session_id: String,
        /// Export format
        #[arg(short, long, default_value = "json")]
        format: String,
        /// Output file path
        #[arg(short, long)]
        output: Option<String>,
    },
}

/// Memory subcommands
#[derive(Subcommand)]
pub enum MemoryCommands {
    /// Search memory
    Search {
        /// Search query
        #[arg(value_name = "QUERY")]
        query: String,
        /// Maximum results
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },
    /// List recent memories
    List {
        /// Maximum results
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },
    /// Add a memory entry
    Add {
        /// Memory content
        #[arg(value_name = "CONTENT")]
        content: String,
        /// Memory tags (comma-separated)
        #[arg(short, long)]
        tags: Option<String>,
    },
    /// Show knowledge graph entities
    Graph {
        /// Entity name filter
        #[arg(value_name = "QUERY")]
        query: Option<String>,
        /// Maximum results
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },
}

/// Tools subcommands
#[derive(Subcommand)]
pub enum ToolsCommands {
    /// List all available tools
    List,
    /// Invoke a tool
    Invoke {
        /// Tool name
        #[arg(value_name = "TOOL_NAME")]
        tool_name: String,
        /// Tool arguments as JSON
        #[arg(value_name = "ARGS")]
        args: Option<String>,
    },
    /// Show tool details
    Info {
        /// Tool name
        #[arg(value_name = "TOOL_NAME")]
        tool_name: String,
    },
}

/// MCP server management subcommands
#[derive(Subcommand)]
pub enum McpCommands {
    /// List configured MCP servers
    List,
    /// Add a new MCP server
    Add {
        /// Server name
        #[arg(value_name = "NAME")]
        name: String,
        /// Server command
        #[arg(value_name = "COMMAND")]
        command: String,
        /// Command arguments
        #[arg(value_name = "ARGS")]
        args: Vec<String>,
    },
    /// Remove an MCP server
    Remove {
        /// Server name
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// Show MCP server status
    Status {
        /// Server name (optional, show all if omitted)
        #[arg(value_name = "NAME")]
        name: Option<String>,
    },
    /// Show MCP server logs
    Logs {
        /// Server name
        #[arg(value_name = "NAME")]
        name: String,
        /// Number of log lines
        #[arg(short, long, default_value = "50")]
        lines: usize,
    },
}

/// Config subcommands
#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Show current configuration
    Show,
    /// Validate configuration
    Validate,
    /// Initialize configuration (interactive)
    Init,
    /// Get a config value
    Get {
        /// Config key
        #[arg(value_name = "KEY")]
        key: String,
    },
    /// Set a config value
    Set {
        /// Config key
        #[arg(value_name = "KEY")]
        key: String,
        /// Config value
        #[arg(value_name = "VALUE")]
        value: String,
    },
    /// Show configuration file paths
    Paths,
}

/// Completions subcommands
#[derive(Subcommand)]
pub enum CompletionsCommands {
    /// Generate shell completions
    Generate {
        /// Shell type
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}
