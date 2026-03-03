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
    create_provider, default_tools,
    mcp::McpManager,
    providers::ProviderChain,
    register_memory_tools,
    scheduler::{Scheduler, SqliteSchedulerStorage},
    AgentCatalog, AgentRunner, AgentStore, Database, MemoryStore, SessionStore, SkillLoader,
    SkillRegistry, SkillTool, SqliteMemoryStore, SqliteSessionStore, SqliteWorkingMemory,
    ToolExecutionRecorder, WorkingMemory,
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
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&log_filter)),
        )
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

    let base_url =
        provider_config
            .base_url
            .clone()
            .or_else(|| match provider_config.name.as_str() {
                "openai" => std::env::var("OPENAI_BASE_URL").ok(),
                _ => std::env::var("ANTHROPIC_BASE_URL").ok(),
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
        Arc::from(create_provider(&provider_name, api_key, base_url.clone()));

    // Initialize provider chain if configured
    let provider_chain = if let Some(ref pc_config) = cfg.provider_chain {
        let chain = Arc::new(ProviderChain::new(pc_config.failover_policy));
        let chain_clone = Arc::clone(&chain);

        // Add instances to the chain
        for instance_config in &pc_config.instances {
            // Resolve API key from env var if needed
            let api_key = if instance_config.api_key.starts_with("${")
                && instance_config.api_key.ends_with("}")
            {
                let env_var = &instance_config.api_key[2..instance_config.api_key.len() - 1];
                std::env::var(env_var).unwrap_or_else(|_| instance_config.api_key.clone())
            } else {
                instance_config.api_key.clone()
            };

            let instance = octo_engine::providers::LlmInstance {
                id: instance_config.id.clone(),
                provider: instance_config.provider.clone(),
                api_key,
                base_url: instance_config.base_url.clone(),
                model: instance_config.model.clone(),
                priority: instance_config.priority,
                max_rpm: instance_config.max_rpm,
                enabled: instance_config.enabled,
            };
            // Note: add_instance takes &self, so we need Arc<ProviderChain>
            chain_clone.add_instance(instance).await;
        }

        // Start health checker if configured
        chain
            .start_health_checker(octo_engine::providers::HealthCheckConfig {
                interval: std::time::Duration::from_secs(pc_config.health_check_interval_sec),
                timeout: std::time::Duration::from_secs(10),
            })
            .await;

        tracing::info!(
            "Provider chain initialized with {} instances",
            pc_config.instances.len()
        );
        Some(chain)
    } else {
        None
    };

    // Working memory (Layer 0) -- SQLite-backed
    let memory: Arc<dyn WorkingMemory> = Arc::new(SqliteWorkingMemory::new(conn.clone()).await?);

    // Session store -- SQLite-backed with DashMap cache
    let sessions: Arc<dyn SessionStore> = Arc::new(SqliteSessionStore::new(conn.clone()).await?);

    // Persistent memory store (Layer 2) -- SQLite-backed
    let memory_store: Arc<dyn MemoryStore> = Arc::new(SqliteMemoryStore::new(conn.clone()));

    // MCP manager (for runtime server management)
    let mcp_manager = Arc::new(tokio::sync::Mutex::new(McpManager::new()));

    // Tool execution recorder (shares the same connection)
    let recorder = Arc::new(ToolExecutionRecorder::new(conn.clone()));

    // Skill system
    let home_dir = std::env::var("HOME").map(PathBuf::from).ok();
    let project_dir = std::env::current_dir().ok();
    let skill_loader = SkillLoader::new(project_dir.as_deref(), home_dir.as_deref());
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
    let watch_loader = SkillLoader::new(project_dir.as_deref(), home_dir.as_deref());
    if let Err(e) = skill_registry.start_watching(watch_loader) {
        tracing::warn!("Failed to start skill watcher: {e}");
    }

    // Scheduler (with agent execution support)
    let scheduler = if cfg.scheduler.enabled {
        let storage = SqliteSchedulerStorage::new(conn.clone());
        let s = Scheduler::new(
            cfg.scheduler.clone(),
            Arc::new(storage),
            provider.clone(),
            tools.clone(),
            memory.clone(),
            sessions.clone(),
        );
        Some(Arc::new(s))
    } else {
        None
    };

    // Agent system: registry + runner
    // AgentStore uses a synchronous rusqlite connection (separate from the async tokio-rusqlite conn)
    let agent_conn = {
        use std::sync::Mutex;
        let raw = rusqlite::Connection::open(&db_path).expect("failed to open agent DB connection");
        Arc::new(Mutex::new(raw))
    };
    let agent_store = Arc::new(AgentStore::new(agent_conn).expect("failed to init AgentStore"));
    let agent_catalog = Arc::new(AgentCatalog::new().with_store(agent_store));
    let loaded = agent_catalog.load_from_store().unwrap_or(0);
    tracing::info!("Loaded {loaded} persisted agents");
    let default_model = model.clone().unwrap_or_else(|| "claude-opus-4-5".to_string());
    let agent_runner = Arc::new(AgentRunner::new(
        agent_catalog,
        provider.clone(),
        tools.clone(),
        memory.clone(),
        default_model,
    ).with_skill_registry(skill_registry.clone()));

    let state = Arc::new(AppState::new(
        provider_chain,
        tools,
        memory,
        sessions,
        memory_store,
        std::path::PathBuf::from(&db_path),
        mcp_manager,
        model,
        Some(recorder),
        skill_registry,
        scheduler,
        cfg.clone(),
        agent_runner,
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
