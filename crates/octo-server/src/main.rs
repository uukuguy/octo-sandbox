mod router;
mod session;
mod state;
mod ws;

use std::sync::Arc;

use anyhow::Result;
use tracing_subscriber::{fmt, EnvFilter};

use octo_engine::{create_provider, default_tools, InMemoryWorkingMemory};

use session::InMemorySessionStore;
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
            let key = std::env::var("ANTHROPIC_API_KEY")
                .expect("ANTHROPIC_API_KEY must be set");
            let url = std::env::var("ANTHROPIC_BASE_URL").ok();
            (key, url)
        }
    };

    let host = std::env::var("OCTO_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port = std::env::var("OCTO_PORT").unwrap_or_else(|_| "3001".into());
    let addr = format!("{host}:{port}");

    tracing::info!("Using provider: {provider_name}");
    let model = std::env::var("OPENAI_MODEL_NAME").ok();

    let provider = Arc::from(create_provider(&provider_name, api_key, base_url));
    let tools = Arc::new(default_tools());
    let memory: Arc<dyn octo_engine::WorkingMemory> = Arc::new(InMemoryWorkingMemory::new());
    let sessions: Arc<dyn session::SessionStore> = Arc::new(InMemorySessionStore::new());

    let state = Arc::new(AppState::new(provider, tools, memory, sessions, model));

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
