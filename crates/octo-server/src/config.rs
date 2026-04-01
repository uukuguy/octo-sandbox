use octo_engine::auth::AuthConfigYaml;
use octo_engine::providers::{ProviderChainConfig, ProviderConfig, SmartRoutingConfig};
use octo_engine::scheduler::SchedulerConfig;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Sync configuration (offline-first sync with HLC timestamps)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Enable sync (default: false)
    #[serde(default)]
    pub enabled: bool,
    /// Node identifier (auto-generated UUID if not set)
    #[serde(default)]
    pub node_id: Option<String>,
}

/// TLS configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Enable TLS (default: false)
    #[serde(default)]
    pub enabled: bool,
    /// Path to PEM certificate file
    #[serde(default)]
    pub cert_path: Option<PathBuf>,
    /// Path to PEM private key file
    #[serde(default)]
    pub key_path: Option<PathBuf>,
    /// Auto-generate self-signed certificate (default: false)
    #[serde(default)]
    pub self_signed: bool,
    /// Directory for self-signed cert output (default: ./data/tls)
    #[serde(default)]
    pub self_signed_dir: Option<PathBuf>,
}

/// Main configuration for octo-server
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// Server configuration
    #[serde(default)]
    pub server: ServerConfig,
    /// LLM provider configuration
    #[serde(default)]
    pub provider: ProviderConfig,
    /// Database configuration
    #[serde(default)]
    pub database: DatabaseConfig,
    /// Logging configuration
    #[serde(default)]
    pub logging: LoggingConfig,
    /// MCP configuration
    #[serde(default)]
    pub mcp: McpConfig,
    /// Skills configuration
    #[serde(default)]
    pub skills: SkillsConfig,
    /// Tools configuration
    #[serde(default)]
    pub tools: ToolsConfig,
    /// Auth configuration (optional)
    #[serde(default)]
    pub auth: AuthConfigYaml,
    /// Scheduler configuration
    #[serde(default)]
    pub scheduler: SchedulerConfig,
    /// Provider Chain configuration (optional)
    #[serde(default)]
    pub provider_chain: Option<ProviderChainConfig>,
    /// Working directory for sandbox (optional)
    #[serde(default)]
    pub working_dir: Option<String>,
    /// Enable event bus for observability (default: false)
    #[serde(default)]
    pub enable_event_bus: bool,
    /// Smart routing configuration (optional)
    #[serde(default)]
    pub smart_routing: Option<SmartRoutingConfig>,
    /// TLS configuration
    #[serde(default)]
    pub tls: TlsConfig,
    /// Sync configuration (offline-first sync)
    #[serde(default)]
    pub sync: SyncConfig,
    /// Multi-session configuration
    #[serde(default)]
    pub sessions: SessionsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Server host (default: 127.0.0.1)
    pub host: String,
    /// Server port (default: 3001)
    pub port: u16,
    /// Allowed CORS origins (empty = allow all)
    #[serde(default)]
    pub cors_origins: Vec<String>,
    /// Enable strict CORS mode: rejects wildcard when true (default: false)
    #[serde(default)]
    pub cors_strict: bool,
    /// Max request body size in bytes (default: 10MB = 10485760)
    #[serde(default)]
    pub max_body_size: Option<usize>,
    /// Request timeout in seconds (default: 30)
    #[serde(default)]
    pub request_timeout_secs: Option<u64>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 3001,
            cors_origins: vec![],
            cors_strict: false,
            max_body_size: None,
            request_timeout_secs: None,
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
            level: "octo_server=info,octo_engine=info,tower_http=info".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpConfig {
    /// MCP servers directory (optional)
    pub servers_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillsConfig {
    /// Skills directories to load from
    pub dirs: Vec<String>,
}

/// Tools configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolsConfig {
    /// Web search engine priority order (e.g., ["jina", "tavily", "ddg"])
    #[serde(default)]
    pub web_search_priority: Vec<String>,
}

/// Multi-session configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionsConfig {
    /// Maximum concurrent sessions (default: 64)
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: usize,
    /// Idle timeout in seconds (0 = no timeout, default: 3600)
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_secs: u64,
    /// Memory isolation mode: "strict" (per-session) or "relaxed" (per-user)
    #[serde(default = "default_memory_isolation")]
    pub memory_isolation: String,
}

fn default_max_concurrent() -> usize {
    64
}
fn default_idle_timeout() -> u64 {
    3600
}
fn default_memory_isolation() -> String {
    "strict".to_string()
}

impl Default for SessionsConfig {
    fn default() -> Self {
        Self {
            max_concurrent: default_max_concurrent(),
            idle_timeout_secs: default_idle_timeout(),
            memory_isolation: default_memory_isolation(),
        }
    }
}

/// Credentials file structure (`~/.octo/credentials.yaml`)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CredentialsFile {
    #[serde(default)]
    pub providers: std::collections::HashMap<String, ProviderCredential>,
}

/// Per-provider credential entry
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderCredential {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

/// Load a YAML config file, returning None if missing or invalid.
fn load_yaml_config(path: &Path) -> Option<Config> {
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(path).ok()?;
    match serde_yaml::from_str::<Config>(&content) {
        Ok(cfg) => Some(cfg),
        Err(e) => {
            tracing::warn!("Failed to parse {}: {}", path.display(), e);
            None
        }
    }
}

/// Recursively merge two YAML Values. overlay wins for scalars/sequences.
fn merge_yaml_values(base: serde_yaml::Value, overlay: serde_yaml::Value) -> serde_yaml::Value {
    use serde_yaml::Value;
    match (base, overlay) {
        (Value::Mapping(mut base_map), Value::Mapping(overlay_map)) => {
            for (key, overlay_v) in overlay_map {
                let merged = if let Some(base_v) = base_map.remove(&key) {
                    merge_yaml_values(base_v, overlay_v)
                } else {
                    overlay_v
                };
                base_map.insert(key, merged);
            }
            Value::Mapping(base_map)
        }
        (_base, overlay) => overlay,
    }
}

/// Merge two configs: `overlay` fields override `base` fields.
fn merge_configs(base: Config, overlay: Config) -> Config {
    let base_val = serde_yaml::to_value(&base).unwrap_or(serde_yaml::Value::Null);
    let overlay_val = serde_yaml::to_value(&overlay).unwrap_or(serde_yaml::Value::Null);
    let merged = merge_yaml_values(base_val, overlay_val);
    serde_yaml::from_value(merged).unwrap_or(base)
}

impl Config {
    /// Load configuration with layered priority.
    ///
    /// Priority (lowest to highest):
    /// 1. Code defaults (impl Default)
    /// 2. Global config: `~/.octo/config.yaml`
    /// 3. Project config: `$PWD/.octo/config.yaml`
    /// 4. Project local config: `$PWD/.octo/config.local.yaml`
    /// 5. Legacy fallback: `$PWD/config.yaml` (if no .octo configs found)
    /// 6. CLI arguments: --port, --host
    /// 7. Environment variables: OCTO_*, ANTHROPIC_*, OPENAI_*
    ///
    /// When `explicit_config` is provided (--config flag), it replaces
    /// steps 2-5 entirely (only that file + CLI + env apply).
    pub fn load(
        explicit_config: Option<&PathBuf>,
        cli_port: Option<u16>,
        cli_host: Option<&str>,
        octo_root: &octo_engine::OctoRoot,
    ) -> Self {
        let mut config = if let Some(path) = explicit_config {
            // Explicit --config: use only this file, skip auto-discovery
            load_yaml_config(path).unwrap_or_default()
        } else {
            // Auto-discovery: merge global → project → local
            let mut cfg = Config::default();

            // Layer 1: Global config
            if let Some(global) = load_yaml_config(&octo_root.global_config()) {
                tracing::debug!("Loaded global config: {}", octo_root.global_config().display());
                cfg = merge_configs(cfg, global);
            }

            // Layer 2: Project config
            if let Some(project) = load_yaml_config(&octo_root.project_config()) {
                tracing::debug!("Loaded project config: {}", octo_root.project_config().display());
                cfg = merge_configs(cfg, project);
            }

            // Layer 3: Project local config
            if let Some(local) = load_yaml_config(&octo_root.project_local_config()) {
                tracing::debug!("Loaded local config: {}", octo_root.project_local_config().display());
                cfg = merge_configs(cfg, local);
            }

            // Legacy fallback: $PWD/config.yaml (if no project config was found)
            let legacy_path = octo_root.working_dir().join("config.yaml");
            if !octo_root.project_config().exists() && legacy_path.exists() {
                tracing::warn!(
                    "Found config.yaml at project root (legacy location). \
                     Please move it to .octo/config.yaml: \
                     mv config.yaml .octo/config.yaml"
                );
                if let Some(legacy) = load_yaml_config(&legacy_path) {
                    cfg = merge_configs(cfg, legacy);
                }
            }

            cfg
        };

        // CLI arguments override
        if let Some(port) = cli_port {
            config.server.port = port;
        }
        if let Some(host) = cli_host {
            config.server.host = host.to_string();
        }

        // Credentials file: between config merge and env overrides
        // Priority: env vars > credentials.yaml > config.yaml
        let cred_path = octo_root.credentials_path();
        if cred_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&cred_path) {
                match serde_yaml::from_str::<CredentialsFile>(&content) {
                    Ok(creds) => {
                        let provider_name = config.provider.name.clone();
                        if let Some(cred) = creds.providers.get(&provider_name) {
                            if config.provider.api_key.is_none() {
                                config.provider.api_key = cred.api_key.clone();
                            }
                            if config.provider.base_url.is_none() {
                                config.provider.base_url = cred.base_url.clone();
                            }
                        }
                        tracing::debug!("Loaded credentials from {}", cred_path.display());
                    }
                    Err(e) => tracing::warn!("Failed to parse credentials: {}", e),
                }
            }
        }

        // Environment variables have highest priority (override everything)
        // Server
        if let Ok(host) = std::env::var("OCTO_HOST") {
            config.server.host = host;
        }
        if let Ok(port) = std::env::var("OCTO_PORT") {
            if let Ok(p) = port.parse() {
                config.server.port = p;
            }
        }

        // CORS strict mode
        if let Ok(strict) = std::env::var("OCTO_CORS_STRICT") {
            config.server.cors_strict = strict.parse().unwrap_or(false);
        }
        // Request body size limit
        if let Ok(size) = std::env::var("OCTO_MAX_BODY_SIZE") {
            if let Ok(n) = size.parse() {
                config.server.max_body_size = Some(n);
            }
        }
        // Request timeout
        if let Ok(timeout) = std::env::var("OCTO_REQUEST_TIMEOUT_SECS") {
            if let Ok(n) = timeout.parse() {
                config.server.request_timeout_secs = Some(n);
            }
        }

        // CORS origins
        if let Ok(origins) = std::env::var("OCTO_CORS_ORIGINS") {
            config.server.cors_origins = origins
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
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
                        config.provider.api_key = Some(api_key);
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
                        config.provider.api_key = Some(api_key);
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

        // Working directory
        if let Ok(dir) = std::env::var("OCTO_WORKING_DIR") {
            config.working_dir = Some(dir);
        }

        // Event bus
        if let Ok(enabled) = std::env::var("OCTO_ENABLE_EVENT_BUS") {
            config.enable_event_bus = enabled.parse().unwrap_or(false);
        }

        // TLS
        if let Ok(enabled) = std::env::var("OCTO_TLS_ENABLED") {
            config.tls.enabled = enabled.parse().unwrap_or(false);
        }
        if let Ok(path) = std::env::var("OCTO_TLS_CERT_PATH") {
            config.tls.cert_path = Some(PathBuf::from(path));
        }
        if let Ok(path) = std::env::var("OCTO_TLS_KEY_PATH") {
            config.tls.key_path = Some(PathBuf::from(path));
        }
        if let Ok(v) = std::env::var("OCTO_TLS_SELF_SIGNED") {
            config.tls.self_signed = v.parse().unwrap_or(false);
        }

        // Auth: OCTO_AUTH_MODE and OCTO_API_KEY override config.yaml
        if let Ok(mode) = std::env::var("OCTO_AUTH_MODE") {
            let m = match mode.to_lowercase().as_str() {
                "none" => Some(octo_engine::auth::AuthMode::None),
                "api_key" | "apikey" => Some(octo_engine::auth::AuthMode::ApiKey),
                "full" => Some(octo_engine::auth::AuthMode::Full),
                _ => None,
            };
            if let Some(m) = m {
                config.auth.mode = Some(m);
            }
        }
        if let Ok(key) = std::env::var("OCTO_API_KEY") {
            if !key.is_empty() {
                use octo_engine::auth::ApiKeyConfig;
                let keys = config.auth.api_keys.get_or_insert_with(Vec::new);
                keys.push(ApiKeyConfig {
                    key,
                    user_id: Some(
                        std::env::var("OCTO_API_KEY_USER").unwrap_or_else(|_| "default".into()),
                    ),
                    permissions: vec!["read".into(), "write".into(), "admin".into()],
                    role: None,
                    expires_at: None,
                });
            }
        }

        // Sessions config
        if let Ok(max) = std::env::var("OCTO_SESSIONS_MAX_CONCURRENT") {
            if let Ok(n) = max.parse() {
                config.sessions.max_concurrent = n;
            }
        }
        if let Ok(timeout) = std::env::var("OCTO_SESSIONS_IDLE_TIMEOUT") {
            if let Ok(n) = timeout.parse() {
                config.sessions.idle_timeout_secs = n;
            }
        }
        if let Ok(mode) = std::env::var("OCTO_SESSIONS_MEMORY_ISOLATION") {
            config.sessions.memory_isolation = mode;
        }

        config
    }

    /// Generate a default config.yaml with all parameters commented
    /// Programmatically generated from actual code defaults
    pub fn generate_default_yaml() -> String {
        let defaults = Self::default();
        let mut output = String::new();

        output.push_str(
            "# =============================================================================\n",
        );
        output.push_str("# Octo Server Configuration\n");
        output.push_str(
            "# =============================================================================\n",
        );
        output.push_str(
            "# Copy this file to config.yaml and uncomment/modify parameters as needed.\n",
        );
        output.push_str("# Priority: config.yaml < .env < CLI args < shell env vars\n");
        output.push_str(
            "# =============================================================================\n\n",
        );

        // Server
        output.push_str("# Server configuration\n");
        output.push_str("# server:\n");
        output.push_str(&format!(
            "#   host: {}    # Server bind address\n",
            defaults.server.host
        ));
        output.push_str(&format!(
            "#   port: {}          # Server port\n",
            defaults.server.port
        ));
        output.push_str("#   cors_strict: false    # Reject wildcard CORS in production\n");
        output.push_str("#   max_body_size: 10485760  # Max request body (bytes, default 10MB)\n");
        output.push_str("#   request_timeout_secs: 30 # Request timeout (seconds)\n\n");

        // Provider
        output.push_str("# LLM Provider configuration\n");
        output.push_str("# provider:\n");
        output.push_str(&format!(
            "#   name: {}     # Provider: anthropic or openai\n",
            defaults.provider.name
        ));
        output.push_str("#   api_key: \"\"         # API key (required)\n");
        output.push_str(&format!(
            "#   base_url: {:?}      # Optional proxy URL\n",
            defaults.provider.base_url
        ));
        output.push_str(&format!(
            "#   model: {:?}         # Optional model override\n\n",
            defaults.provider.model
        ));

        // Database
        output.push_str("# Database configuration\n");
        output.push_str("# database:\n");
        output.push_str(&format!(
            "#   path: {}  # SQLite database path\n\n",
            defaults.database.path
        ));

        // Logging
        output.push_str("# Logging configuration\n");
        output.push_str("# logging:\n");
        output.push_str(&format!("#   level: {}\n\n", defaults.logging.level));

        // MCP
        output.push_str("# MCP configuration\n");
        output.push_str("# mcp:\n");
        output.push_str(&format!(
            "#   servers_dir: {:?}   # Optional MCP servers directory\n\n",
            defaults.mcp.servers_dir
        ));

        // Skills
        output.push_str("# Skills configuration\n");
        output.push_str("# skills:\n");
        output.push_str(&format!(
            "#   dirs: {:?}            # List of skills directories\n\n",
            defaults.skills.dirs
        ));

        // Working directory
        output.push_str("# Working directory for sandbox (optional)\n");
        output.push_str("# working_dir: \"./data/sandbox\"   # Optional working directory\n\n");

        // Event bus
        output.push_str("# Enable event bus for observability\n");
        output.push_str(&format!(
            "# enable_event_bus: {}    # Enable event bus (default: false)\n",
            defaults.enable_event_bus
        ));

        // TLS
        output.push_str("\n# TLS configuration\n");
        output.push_str("# tls:\n");
        output.push_str("#   enabled: false        # Enable HTTPS\n");
        output.push_str("#   cert_path: null       # Path to PEM certificate\n");
        output.push_str("#   key_path: null        # Path to PEM private key\n");
        output.push_str("#   self_signed: false    # Auto-generate self-signed cert\n");
        output.push_str("#   self_signed_dir: null # Output dir for self-signed certs\n");

        // Auth
        output.push_str("\n# Auth configuration\n");
        output.push_str("# Configure via environment variables (recommended) or inline below.\n");
        output.push_str("#\n");
        output.push_str("# Option 1: Disable auth (local dev only)\n");
        output.push_str("#   OCTO_AUTH_MODE=none\n");
        output.push_str("#\n");
        output.push_str("# Option 2: API key auth\n");
        output.push_str("#   OCTO_AUTH_MODE=api_key\n");
        output.push_str("#   OCTO_API_KEY=your-secret-key     # key clients use in Authorization: Bearer header\n");
        output.push_str(
            "#   OCTO_API_KEY_USER=dev            # optional user id (default: \"default\")\n",
        );
        output.push_str("#\n");
        output.push_str("# auth:\n");
        output.push_str("#   mode: api_key   # none | api_key\n");
        output.push_str("#   api_keys:\n");
        output.push_str("#     - key: \"your-secret-key\"\n");
        output.push_str("#       user_id: \"dev\"\n");
        output.push_str("#       permissions: [\"read\", \"write\", \"admin\"]\n");

        // Sessions
        output.push_str("\n# Multi-session configuration\n");
        output.push_str("# sessions:\n");
        output.push_str(&format!(
            "#   max_concurrent: {}    # Maximum concurrent sessions\n",
            defaults.sessions.max_concurrent
        ));
        output.push_str(&format!(
            "#   idle_timeout_secs: {} # Idle timeout (0 = no timeout)\n",
            defaults.sessions.idle_timeout_secs
        ));
        output.push_str(&format!(
            "#   memory_isolation: {}  # strict (per-session) or relaxed (per-user)\n",
            defaults.sessions.memory_isolation
        ));

        // Scheduler
        output.push_str("\n# Scheduler configuration\n");
        output.push_str("# scheduler:\n");
        output.push_str(&format!(
            "#   enabled: {}          # Enable cron scheduler\n",
            defaults.scheduler.enabled
        ));
        output.push_str(&format!(
            "#   check_interval_secs: {}  # Interval between task checks (seconds)\n",
            defaults.scheduler.check_interval_secs
        ));
        output.push_str(&format!(
            "#   max_concurrent: {}       # Max concurrent scheduled tasks\n",
            defaults.scheduler.max_concurrent
        ));

        // Provider Chain
        output.push_str("\n# Provider chain configuration (optional, multi-provider failover)\n");
        output.push_str("# provider_chain:\n");
        output.push_str("#   failover_policy: automatic  # automatic | manual\n");
        output.push_str("#   health_check_interval_sec: 30\n");
        output.push_str("#   instances:\n");
        output.push_str("#     - name: primary\n");
        output.push_str("#       provider: anthropic\n");
        output.push_str("#       model: claude-sonnet-4-20250514\n");
        output.push_str("#       priority: 1\n");
        output.push_str("#     - name: fallback\n");
        output.push_str("#       provider: openai\n");
        output.push_str("#       model: gpt-4o\n");
        output.push_str("#       priority: 2\n");

        // Smart Routing
        output.push_str("\n# Smart routing configuration (optional, complexity-based model routing)\n");
        output.push_str("# smart_routing:\n");
        output.push_str("#   enabled: false         # Enable smart routing\n");
        output.push_str("#   default_tier: medium    # Default tier: low | medium | high\n");
        output.push_str("#   tiers: {}               # Custom tier configurations\n");
        output.push_str("#   thresholds: null        # Custom analyzer thresholds\n");

        // Sync
        output.push_str("\n# Sync configuration (offline-first sync with HLC timestamps)\n");
        output.push_str("# sync:\n");
        output.push_str(&format!(
            "#   enabled: {}          # Enable offline sync\n",
            defaults.sync.enabled
        ));
        output.push_str("#   node_id: null          # Node identifier (auto-generated UUID if not set)\n");

        output
    }
}
