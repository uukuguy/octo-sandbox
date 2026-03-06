use std::sync::Arc;

use axum::{body::Body, extract::Request, extract::State, routing::get, Json, Router};
use octo_engine::auth::{auth_middleware_with_role, AuthConfig};
use serde::Serialize;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::api;
use crate::middleware::{audit_middleware, AuditMiddlewareState, RateLimiter};
use crate::state::AppState;
use crate::ws::ws_handler;

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    uptime_secs: u64,
    provider: String,
    mcp_servers: Vec<McpServerStatus>,
    version: &'static str,
}

#[derive(Serialize)]
struct McpServerStatus {
    name: String,
    status: String,
}

async fn health(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    let uptime = state.start_time.elapsed().as_secs();

    // Get MCP server states
    let mcp_states = state.agent_supervisor.get_all_mcp_server_states().await;
    let mcp_servers: Vec<McpServerStatus> = mcp_states
        .into_iter()
        .map(|(name, state)| McpServerStatus {
            name,
            status: format!("{:?}", state),
        })
        .collect();

    Json(HealthResponse {
        status: "ok",
        uptime_secs: uptime,
        provider: state.config.provider.name.clone(),
        mcp_servers,
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// Rate limiting middleware
async fn rate_limit_middleware(
    rate_limiter: axum::extract::State<RateLimiter>,
    req: Request<Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    use tracing::debug;

    // Extract client IP: try ConnectInfo extension first, then X-Forwarded-For header
    let client_ip = req
        .extensions()
        .get::<axum::extract::connect_info::ConnectInfo<std::net::SocketAddr>>()
        .map(|connect_info| connect_info.0.ip().to_string())
        .or_else(|| {
            req.headers()
                .get("x-forwarded-for")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.split(',').next())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "127.0.0.1".to_string());

    debug!(client_ip = %client_ip, "Rate limit check");

    if !rate_limiter.check(&client_ip).await {
        debug!("Rate limit exceeded for {}", client_ip);
        return (
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", "60")],
            "Rate limit exceeded. Please try again later.",
        )
            .into_response();
    }

    next.run(req).await
}

/// Auth middleware wrapper that extracts AuthConfig from AppState
async fn auth_middleware_wrapper(
    state: axum::extract::State<Arc<AppState>>,
    req: Request<Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let config: &AuthConfig = &state.auth_config;
    match auth_middleware_with_role(req, next, config).await {
        Ok(response) => response,
        Err(status) => {
            tracing::debug!("Auth middleware rejected request: {}", status);
            axum::response::Response::builder()
                .status(status)
                .body(Body::empty())
                .unwrap_or_else(|_| axum::response::Response::new(Body::empty()))
        }
    }
}

pub fn build_router(state: Arc<AppState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Rate limiter: 100 requests per minute per IP
    let rate_limiter = RateLimiter::new(100, 60);

    // Audit middleware state - uses the same database path as AppState
    let audit_state = AuditMiddlewareState::new(state.db_path.clone());

    // Clone state for auth middleware (we need to pass a separate Arc<AppState> for the middleware)
    let auth_state = state.clone();

    // Build router with middleware layers
    Router::new()
        // Health check is open (no auth required)
        .route("/api/health", get(health))
        // WebSocket endpoint (auth middleware applied)
        .route("/ws", get(ws_handler))
        // API routes - all protected by auth middleware
        // Auth middleware injects UserContext into request extensions
        .nest("/api", api::routes())
        .with_state(state)
        .with_state(rate_limiter.clone())
        .with_state(audit_state.clone())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        // Rate limiting middleware
        .layer(axum::middleware::from_fn_with_state(
            rate_limiter,
            rate_limit_middleware,
        ))
        // Auth middleware - validates API keys and injects UserContext
        .layer(axum::middleware::from_fn_with_state(
            auth_state,
            auth_middleware_wrapper,
        ))
        // Audit middleware - logs all requests to audit log
        .layer(axum::middleware::from_fn_with_state(
            audit_state,
            audit_middleware,
        ))
}
