//! octo-platform-server
//!
//! Multi-tenant multi-agent platform API server.

use std::sync::Arc;

use anyhow::Result;
use axum::{
    body::Body,
    extract::State,
    http::Request,
    routing::{delete, get, patch, post, put},
    Json, Router,
};
use octo_platform_server::{api::admin, api::mcp, api::sessions, api::users, db, AppState, PlatformConfig};
use octo_platform_server::{ErrorResponse, LoginResponse, RegisterResponse};
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use octo_platform_server::ws::ws_handler;

// Auth handlers
async fn register(
    State(state): State<Arc<AppState>>,
    Json(req): Json<db::RegisterRequest>,
) -> Result<Json<RegisterResponse>, ErrorResponse> {
    let user = state.db.register(&req, None).map_err(|_| ErrorResponse {
        error: "Failed to register user".to_string(),
    })?;

    Ok(Json(RegisterResponse { user }))
}

async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<db::LoginRequest>,
) -> Result<Json<LoginResponse>, ErrorResponse> {
    let user = state.db.authenticate(&req).map_err(|_| ErrorResponse {
        error: "Invalid credentials".to_string(),
    })?;

    let access_token = state
        .jwt
        .generate_access_token(&user.id, &user.email, &user.role.to_string(), &user.tenant_id)
        .map_err(|_| ErrorResponse {
            error: "Failed to generate access token".to_string(),
        })?;

    let refresh_token = state
        .jwt
        .generate_refresh_token(&user.id, &user.email, &user.role.to_string(), &user.tenant_id)
        .map_err(|_| ErrorResponse {
            error: "Failed to generate refresh token".to_string(),
        })?;

    Ok(Json(LoginResponse {
        access_token,
        refresh_token,
        user,
    }))
}

#[derive(serde::Deserialize)]
struct RefreshRequest {
    refresh_token: String,
}

async fn refresh(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RefreshRequest>,
) -> Result<Json<LoginResponse>, ErrorResponse> {
    let token_data = state
        .jwt
        .verify_token(&req.refresh_token)
        .map_err(|_| ErrorResponse {
            error: "Invalid refresh token".to_string(),
        })?;

    let claims = token_data.claims;

    let user = state
        .db
        .get_user(&claims.tenant_id, &claims.sub)
        .map_err(|_| ErrorResponse {
            error: "Failed to get user".to_string(),
        })?
        .ok_or_else(|| ErrorResponse {
            error: "User not found".to_string(),
        })?;

    let access_token = state
        .jwt
        .generate_access_token(&user.id, &user.email, &user.role.to_string(), &user.tenant_id)
        .map_err(|_| ErrorResponse {
            error: "Failed to generate access token".to_string(),
        })?;

    let refresh_token = state
        .jwt
        .generate_refresh_token(&user.id, &user.email, &user.role.to_string(), &user.tenant_id)
        .map_err(|_| ErrorResponse {
            error: "Failed to generate refresh token".to_string(),
        })?;

    Ok(Json(LoginResponse {
        access_token,
        refresh_token,
        user,
    }))
}

async fn me(
    State(state): State<Arc<AppState>>,
    request: Request<Body>,
) -> Result<Json<db::UserResponse>, ErrorResponse> {
    use octo_platform_server::extract_bearer_token;

    let token = extract_bearer_token(request.headers())?;

    let claims = state.jwt.verify_token(&token).map_err(|_| ErrorResponse {
        error: "Invalid token".to_string(),
    })?;

    let user = state
        .db
        .get_user(&claims.claims.tenant_id, &claims.claims.sub)
        .map_err(|_| ErrorResponse {
            error: "Failed to get user".to_string(),
        })?
        .ok_or_else(|| ErrorResponse {
            error: "User not found".to_string(),
        })?;

    Ok(Json(user))
}

// Main

async fn health() -> &'static str {
    "ok"
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "octo_platform_server=debug,tower=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting octo-platform-server...");

    let config = PlatformConfig::default();
    let state = Arc::new(AppState::new(config.clone())?);

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/auth/register", post(register))
        .route("/api/auth/login", post(login))
        .route("/api/auth/refresh", post(refresh))
        .route("/api/auth/me", get(me))
        // Session routes
        .route("/api/sessions", get(sessions::list_sessions))
        .route("/api/sessions", post(sessions::create_session))
        .route("/api/sessions/{session_id}", get(sessions::get_session))
        .route(
            "/api/sessions/{session_id}",
            delete(sessions::delete_session),
        )
        // User routes (admin or self)
        .route("/api/users", get(users::list_users))
        .route("/api/users/{user_id}", get(users::get_user))
        .route("/api/users/{user_id}", put(users::update_user))
        .route("/api/users/{user_id}", delete(users::delete_user))
        .route(
            "/api/users/{user_id}/role",
            patch(users::update_user_role),
        )
        // MCP routes
        .route("/api/mcp", get(mcp::list_mcp).post(mcp::add_mcp))
        .route("/api/mcp/:id", get(mcp::get_mcp).delete(mcp::delete_mcp))
        // Admin routes
        .route("/api/admin/tenants", get(admin::tenants::list_tenants).post(admin::tenants::create_tenant))
        .route("/api/admin/tenants/:id", get(admin::tenants::get_tenant).patch(admin::tenants::update_tenant).delete(admin::tenants::delete_tenant))
        .route("/api/admin/tenants/:id/quotas", get(admin::tenants::get_quotas).patch(admin::tenants::update_quotas))
        // WebSocket
        .route("/ws/{session_id}", get(ws_handler))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = format!("{}:{}", config.host, config.port);
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("Listening on {}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}
