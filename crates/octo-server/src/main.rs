mod api;
mod router;
mod session;
mod state;
mod ws;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use tracing_subscriber::{fmt, EnvFilter};

use octo_engine::{
    create_provider, default_tools, register_memory_tools, Database, MemoryStore, SessionStore,
    SkillLoader, SkillRegistry, SkillTool, SqliteMemoryStore, SqliteSessionStore,
    SqliteWorkingMemory, ToolExecutionRecorder, WorkingMemory,
};
use state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv_override().ok();

    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("octo_server=info,tower_http=debug")),
        )
        .init();

    let provider_name = std::env::var("LLM_PROVIDER").unwrap_or_else(|_| "anthropic".into());

    let (api_key, base_url) = match provider_name.as_str() {
        "openai" => {
            let key = std::env::var("OPENAI_API_KEY")
                .expect("OPENAI_API_KEY must be set when PROVIDER=openai");
            let url = std::env::var("OPENAI_BASE_URL").ok();
            (key, url)
        }
        _ => {
            let key =
                std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY must be set");
            let url = std::env::var("ANTHROPIC_BASE_URL").ok();
            (key, url)
        }
    };

    let host = std::env::var("OCTO_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port = std::env::var("OCTO_PORT").unwrap_or_else(|_| "3001".into());
    let addr = format!("{host}:{port}");

    tracing::info!("Using provider: {provider_name}");
    let model = std::env::var("OPENAI_MODEL_NAME").ok();

    // Database: SQLite with WAL mode
    let db_path =
        std::env::var("OCTO_DB_PATH").unwrap_or_else(|_| "./data/octo.db".into());
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
        model,
        Some(recorder),
        skill_registry,
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
