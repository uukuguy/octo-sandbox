use std::sync::Arc;

use axum::{routing::get, Router};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::api;
use crate::state::AppState;
use crate::ws::ws_handler;

async fn health() -> &'static str {
    "ok"
}

pub fn build_router(state: Arc<AppState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/api/health", get(health))
        .route("/ws", get(ws_handler))
        .nest("/api", api::routes())
        .with_state(state)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}
