use octo_engine::auth::AuthConfigYaml;
use octo_engine::providers::ProviderChainConfig;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Main configuration for octo-server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Server configuration
    pub server: ServerConfig,
    /// LLM provider configuration
    pub provider: ProviderConfig,
    /// Database configuration
    pub database: DatabaseConfig,
    /// Logging configuration
    pub logging: LoggingConfig,
    /// MCP configuration
    pub mcp: McpConfig,
    /// Skills configuration
    pub skills: SkillsConfig,
    /// Auth configuration (optional)
    #[serde(default)]
    pub auth: AuthConfigYaml,
    /// Scheduler configuration
    #[serde(default)]
    pub scheduler: SchedulerConfig,
    /// Provider Chain configuration (optional)
    #[serde(default)]
    pub provider_chain: Option<ProviderChainConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Server host (default: 127.0.0.1)
    pub host: String,
    /// Server port (default: 3001)
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 3001,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Provider name: "anthropic" or "openai"
    pub name: String,
    /// API key (required)
    pub api_key: String,
    /// Base URL for API (optional, for proxies)
    pub base_url: Option<String>,
    /// Model name (optional, provider default if not set)
    pub model: Option<String>,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            name: "anthropic".to_string(),
            api_key: "".to_string(),
            base_url: None,
            model: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// SQLite database path
    pub path: String,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: "./data/octo.db".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// RUST_LOG filter string
    pub level: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "octo_server=debug,octo_engine=debug,tower_http=debug".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    /// MCP servers directory (optional)
    pub servers_dir: Option<String>,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            servers_dir: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsConfig {
    /// Skills directories to load from
    pub dirs: Vec<String>,
}

impl Default for SkillsConfig {
    fn default() -> Self {
        Self {
            dirs: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default = "default_check_interval")]
    pub check_interval_secs: u64,

    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: usize,
}

fn default_check_interval() -> u64 { 60 }
fn default_max_concurrent() -> usize { 5 }

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            check_interval_secs: 60,
            max_concurrent: 5,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            provider: ProviderConfig::default(),
            database: DatabaseConfig::default(),
            logging: LoggingConfig::default(),
            mcp: McpConfig::default(),
            skills: SkillsConfig::default(),
            auth: AuthConfigYaml::default(),
            scheduler: SchedulerConfig::default(),
            provider_chain: None,
        }
    }
}

impl Config {
    /// Load configuration with priority: config.yaml < CLI args < .env
    ///
    /// Priority (lowest to highest):
    /// 1. config.yaml - base configuration file
    /// 2. CLI arguments - e.g., --port 4000
    /// 3. Environment variables (.env) - highest priority for overrides
    pub fn load(config_path: Option<&PathBuf>, cli_port: Option<u16>, cli_host: Option<&str>) -> Self {
        // Step 1: Load from config.yaml (lowest priority)
        let mut config = if let Some(path) = config_path {
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(path) {
                    if let Ok(cfg) = serde_yaml::from_str::<Config>(&content) {
                        Some(cfg)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }.unwrap_or_default();

        // Step 2: CLI arguments override config.yaml
        if let Some(port) = cli_port {
            config.server.port = port;
        }
        if let Some(host) = cli_host {
            config.server.host = host.to_string();
        }

        // Step 3: Environment variables have highest priority (override everything)
        // Server
        if let Ok(host) = std::env::var("OCTO_HOST") {
            config.server.host = host;
        }
        if let Ok(port) = std::env::var("OCTO_PORT") {
            if let Ok(p) = port.parse() {
                config.server.port = p;
            }
        }

        // Provider - read first to determine other fields
        if let Ok(provider_name) = std::env::var("LLM_PROVIDER") {
            config.provider.name = provider_name;
        }

        // Read api_key, base_url, model based on provider
        match config.provider.name.as_str() {
            "openai" => {
                if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
                    if !api_key.is_empty() {
                        config.provider.api_key = api_key;
                    }
                }
                if let Ok(url) = std::env::var("OPENAI_BASE_URL") {
                    config.provider.base_url = Some(url);
                }
                if let Ok(model) = std::env::var("OPENAI_MODEL_NAME") {
                    config.provider.model = Some(model);
                }
            }
            _ => {
                // Default to anthropic
                if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
                    if !api_key.is_empty() {
                        config.provider.api_key = api_key;
                    }
                }
                if let Ok(url) = std::env::var("ANTHROPIC_BASE_URL") {
                    config.provider.base_url = Some(url);
                }
                if let Ok(model) = std::env::var("ANTHROPIC_MODEL_NAME") {
                    config.provider.model = Some(model);
                }
            }
        }

        // Database
        if let Ok(path) = std::env::var("OCTO_DB_PATH") {
            config.database.path = path;
        }

        // Logging
        if let Ok(level) = std::env::var("RUST_LOG") {
            config.logging.level = level;
        }

        config
    }

    /// Generate a default config.yaml with all parameters commented
    /// Programmatically generated from actual code defaults
    pub fn generate_default_yaml() -> String {
        let defaults = Self::default();
        let mut output = String::new();

        output.push_str("# =============================================================================\n");
        output.push_str("# Octo Server Configuration\n");
        output.push_str("# =============================================================================\n");
        output.push_str("# Copy this file to config.yaml and uncomment/modify parameters as needed.\n");
        output.push_str("# Priority: config.yaml < CLI args < .env\n");
        output.push_str("# =============================================================================\n\n");

        // Server
        output.push_str("# Server configuration\n");
        output.push_str("# server:\n");
        output.push_str(&format!("#   host: {}    # Server bind address\n", defaults.server.host));
        output.push_str(&format!("#   port: {}          # Server port\n\n", defaults.server.port));

        // Provider
        output.push_str("# LLM Provider configuration\n");
        output.push_str("# provider:\n");
        output.push_str(&format!("#   name: {}     # Provider: anthropic or openai\n", defaults.provider.name));
        output.push_str("#   api_key: \"\"         # API key (required)\n");
        output.push_str(&format!("#   base_url: {:?}      # Optional proxy URL\n", defaults.provider.base_url));
        output.push_str(&format!("#   model: {:?}         # Optional model override\n\n", defaults.provider.model));

        // Database
        output.push_str("# Database configuration\n");
        output.push_str("# database:\n");
        output.push_str(&format!("#   path: {}  # SQLite database path\n\n", defaults.database.path));

        // Logging
        output.push_str("# Logging configuration\n");
        output.push_str("# logging:\n");
        output.push_str(&format!("#   level: {}\n\n", defaults.logging.level));

        // MCP
        output.push_str("# MCP configuration\n");
        output.push_str("# mcp:\n");
        output.push_str(&format!("#   servers_dir: {:?}   # Optional MCP servers directory\n\n", defaults.mcp.servers_dir));

        // Skills
        output.push_str("# Skills configuration\n");
        output.push_str("# skills:\n");
        output.push_str(&format!("#   dirs: {:?}            # List of skills directories\n", defaults.skills.dirs));

        output
    }
}
