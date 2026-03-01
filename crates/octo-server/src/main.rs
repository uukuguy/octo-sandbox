mod api;
mod config;
mod middleware;
mod router;
mod session;
mod state;
mod ws;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use tracing_subscriber::{fmt, EnvFilter};

use octo_engine::{
    create_provider, default_tools, register_memory_tools, Database, mcp::McpManager,
    MemoryStore, SessionStore, SkillLoader, SkillRegistry, SkillTool,
    SqliteMemoryStore, SqliteSessionStore, SqliteWorkingMemory, ToolExecutionRecorder,
    WorkingMemory,
};
use state::AppState;

fn print_default_config() {
    println!("{}", config::Config::generate_default_yaml());
}

#[tokio::main]
async fn main() -> Result<()> {
    // Handle CLI arguments
    let args: Vec<String> = std::env::args().collect();
    let mut cli_port: Option<u16> = None;
    let mut cli_host: Option<&str> = None;
    let mut config_path: Option<PathBuf> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "config-gen" | "gen-config" => {
                print_default_config();
                return Ok(());
            }
            "--port" | "-p" => {
                if i + 1 < args.len() {
                    cli_port = args[i + 1].parse().ok();
                    i += 2;
                    continue;
                }
            }
            "--host" | "-h" => {
                if i + 1 < args.len() {
                    cli_host = Some(&args[i + 1]);
                    i += 2;
                    continue;
                }
            }
            "--config" | "-c" => {
                if i + 1 < args.len() {
                    config_path = Some(PathBuf::from(&args[i + 1]));
                    i += 2;
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }

    // Load .env FIRST (before config loading)
    dotenvy::dotenv_override().ok();

    // Load configuration: config.yaml < CLI args < .env
    let cfg = config::Config::load(config_path.as_ref(), cli_port, cli_host);

    // Apply logging config (clone to avoid moving)
    let log_filter = std::env::var("RUST_LOG").unwrap_or_else(|_| cfg.logging.level.clone());
    fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            EnvFilter::new(&log_filter)
        }))
        .init();

    let addr = format!("{}:{}", cfg.server.host, cfg.server.port);
    tracing::info!("Using provider: {}", cfg.provider.name);

    let provider_config = cfg.provider.clone();

    let api_key = if provider_config.api_key.is_empty() {
        // Fall back to env var if not in config
        match provider_config.name.as_str() {
            "openai" => std::env::var("OPENAI_API_KEY")
                .expect("OPENAI_API_KEY must be set in config.yaml or .env"),
            _ => std::env::var("ANTHROPIC_API_KEY")
                .expect("ANTHROPIC_API_KEY must be set in config.yaml or .env"),
        }
    } else {
        provider_config.api_key
    };

    let base_url = provider_config.base_url.clone().or_else(|| {
        match provider_config.name.as_str() {
            "openai" => std::env::var("OPENAI_BASE_URL").ok(),
            _ => std::env::var("ANTHROPIC_BASE_URL").ok(),
        }
    });

    // Read model based on provider - panic if not set
    let model = match provider_config.model.clone() {
        Some(m) => Some(m),
        None => {
            let env_var = match provider_config.name.as_str() {
                "openai" => "OPENAI_MODEL_NAME",
                _ => "ANTHROPIC_MODEL_NAME",
            };
            Some(std::env::var(env_var).expect(&format!(
                "{} must be set in .env when LLM_PROVIDER={}",
                env_var, provider_config.name
            )))
        }
    };

    let provider_name = provider_config.name.clone();

    // Database: SQLite with WAL mode (use config, with env override already applied)
    let db_path = cfg.database.path.clone();
    let db = Database::open(&db_path).await?;
    let conn = db.conn().clone();

    let provider: Arc<dyn octo_engine::Provider> =
        Arc::from(create_provider(&provider_name, api_key, base_url));

    // Working memory (Layer 0) -- SQLite-backed
    let memory: Arc<dyn WorkingMemory> =
        Arc::new(SqliteWorkingMemory::new(conn.clone()).await?);

    // Session store -- SQLite-backed with DashMap cache
    let sessions: Arc<dyn SessionStore> =
        Arc::new(SqliteSessionStore::new(conn.clone()).await?);

    // Persistent memory store (Layer 2) -- SQLite-backed
    let memory_store: Arc<dyn MemoryStore> = Arc::new(SqliteMemoryStore::new(conn.clone()));

    // MCP manager (for runtime server management)
    let mcp_manager = Arc::new(tokio::sync::Mutex::new(McpManager::new()));

    // Tool execution recorder (shares the same connection)
    let recorder = Arc::new(ToolExecutionRecorder::new(conn.clone()));

    // Skill system
    let home_dir = std::env::var("HOME")
        .map(PathBuf::from)
        .ok();
    let project_dir = std::env::current_dir().ok();
    let skill_loader = SkillLoader::new(
        project_dir.as_deref(),
        home_dir.as_deref(),
    );
    let skill_registry = Arc::new(SkillRegistry::new());
    if let Err(e) = skill_registry.load_from(&skill_loader) {
        tracing::warn!("Failed to load skills: {e}");
    }

    // Tool registry: built-in + memory tools + skill tools
    let mut tools = default_tools();
    register_memory_tools(&mut tools, memory_store.clone(), provider.clone());

    // Register invocable skills as tools
    for skill in skill_registry.invocable_skills() {
        tracing::info!("Registering skill tool: {}", skill.name);
        tools.register(SkillTool::new(skill));
    }
    let tools = Arc::new(tools);

    // Start skill hot-reload watcher
    let watch_loader = SkillLoader::new(
        project_dir.as_deref(),
        home_dir.as_deref(),
    );
    if let Err(e) = skill_registry.start_watching(watch_loader) {
        tracing::warn!("Failed to start skill watcher: {e}");
    }

    let state = Arc::new(AppState::new(
        provider,
        tools,
        memory,
        sessions,
        memory_store,
        std::path::PathBuf::from(&db_path),
        mcp_manager,
        model,
        Some(recorder),
        skill_registry,
        None, // scheduler - will be added in integration
        cfg.clone(),
    ));

    let app = router::build_router(state);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("octo-server listening on {addr}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C signal handler");
    tracing::info!("shutdown signal received");
}
