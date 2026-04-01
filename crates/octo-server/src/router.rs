use std::sync::Arc;
use std::time::Duration;

use crate::middleware::auth_middleware_with_role;
use axum::{body::Body, extract::Request, extract::State, routing::get, Json, Router};
use octo_engine::auth::AuthConfig;
use serde::Serialize;
use tower_http::cors::{Any, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;
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

/// Simple liveness probe — returns 200 OK if the process is alive
async fn liveness() -> &'static str {
    "ok"
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

/// Security headers middleware — adds standard protective headers to all responses
async fn security_headers_middleware(
    state: axum::extract::State<Arc<AppState>>,
    req: Request<Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let mut response = next.run(req).await;
    let headers = response.headers_mut();

    headers.insert(
        axum::http::header::X_CONTENT_TYPE_OPTIONS,
        "nosniff".parse().unwrap(),
    );
    headers.insert(
        axum::http::header::X_FRAME_OPTIONS,
        "DENY".parse().unwrap(),
    );
    headers.insert(
        axum::http::header::REFERRER_POLICY,
        "strict-origin-when-cross-origin".parse().unwrap(),
    );

    // HSTS only when TLS is enabled
    if state.config.tls.enabled {
        headers.insert(
            axum::http::header::STRICT_TRANSPORT_SECURITY,
            "max-age=31536000; includeSubDomains".parse().unwrap(),
        );
    }

    response
}

pub fn build_router(state: Arc<AppState>) -> Router {
    // CORS: strict mode rejects wildcard when cors_strict=true
    let cors = if state.config.server.cors_strict {
        if state.config.server.cors_origins.is_empty() {
            tracing::warn!(
                "CORS strict mode enabled but no origins configured. \
                 Set server.cors_origins or disable cors_strict."
            );
            // In strict mode with no origins, deny all cross-origin requests
            CorsLayer::new()
                .allow_methods(Any)
                .allow_headers(Any)
        } else {
            let origins: Vec<axum::http::HeaderValue> = state
                .config
                .server
                .cors_origins
                .iter()
                .filter_map(|o| o.parse().ok())
                .collect();
            CorsLayer::new()
                .allow_origin(origins)
                .allow_methods(Any)
                .allow_headers(Any)
        }
    } else if state.config.server.cors_origins.is_empty() {
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any)
    } else {
        let origins: Vec<axum::http::HeaderValue> = state
            .config
            .server
            .cors_origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect();
        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods(Any)
            .allow_headers(Any)
    };

    // Rate limiter: 100 requests per minute per IP
    let rate_limiter = RateLimiter::new(100, 60);

    // Audit middleware state - uses the same database path as AppState
    let audit_state = AuditMiddlewareState::new(state.db_path.clone());

    // Clone state for auth middleware (we need to pass a separate Arc<AppState> for the middleware)
    let auth_state = state.clone();

    // Request body size limit (configurable, default 10MB)
    let max_body_size = state.config.server.max_body_size.unwrap_or(10 * 1024 * 1024);
    // Request timeout (configurable, default 30s)
    let request_timeout = Duration::from_secs(state.config.server.request_timeout_secs.unwrap_or(30));

    #[allow(deprecated)] // TimeoutLayer::new deprecated but with_status_code not in 0.6
    let timeout_layer = TimeoutLayer::new(request_timeout);
    let security_state = state.clone();

    // Build router with middleware layers
    Router::new()
        // Health checks are open (no auth required)
        .route("/api/health", get(health))
        .route("/api/health/live", get(liveness))
        // WebSocket endpoint (auth middleware applied)
        .route("/ws", get(ws_handler))
        // API routes - all protected by auth middleware
        // Auth middleware injects UserContext into request extensions
        .nest("/api/v1", api::routes())
        .with_state(state)
        .with_state(rate_limiter.clone())
        .with_state(audit_state.clone())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        // Request body size limit
        .layer(RequestBodyLimitLayer::new(max_body_size))
        // Request timeout
        .layer(timeout_layer)
        // Security headers middleware (runs on every response)
        .layer(axum::middleware::from_fn_with_state(
            security_state,
            security_headers_middleware,
        ))
        // Middleware layers use LIFO ordering: last added = first to run.
        // Desired execution order: rate_limit → auth → audit
        // So we add them in reverse: audit first, rate_limit last.
        //
        // Audit middleware - logs all requests (runs AFTER auth, so UserContext is available)
        .layer(axum::middleware::from_fn_with_state(
            audit_state,
            audit_middleware,
        ))
        // Auth middleware - validates API keys and injects UserContext
        .layer(axum::middleware::from_fn_with_state(
            auth_state,
            auth_middleware_wrapper,
        ))
        // Rate limiting middleware (runs FIRST - before auth and audit)
        .layer(axum::middleware::from_fn_with_state(
            rate_limiter,
            rate_limit_middleware,
        ))
}
