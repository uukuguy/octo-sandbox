//! octo-platform-server
//!
//! Multi-tenant multi-agent platform API server.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::{
    body::Body,
    extract::{FromRequestParts, State},
    http::{header::AUTHORIZATION, request::Parts, StatusCode, Request},
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post},
    Router,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub mod auth;
pub mod db;
pub mod user_runtime;
pub mod api;
pub use user_runtime::UserRuntime;

/// Platform configuration
#[derive(Debug, Clone)]
pub struct PlatformConfig {
    pub host: String,
    pub port: u16,
    pub data_dir: PathBuf,
    pub user_runtime: UserRuntimeConfig,
}

impl Default for PlatformConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 3002,
            data_dir: PathBuf::from("./data-platform"),
            user_runtime: UserRuntimeConfig::default(),
        }
    }
}

/// User runtime configuration
#[derive(Debug, Clone)]
pub struct UserRuntimeConfig {
    pub max_concurrent_agents: u32,
    pub session_timeout_minutes: u32,
    pub db_path_template: String,
}

impl Default for UserRuntimeConfig {
    fn default() -> Self {
        Self {
            max_concurrent_agents: 3,
            session_timeout_minutes: 30,
            db_path_template: "data-platform/users/{user_id}".to_string(),
        }
    }
}

/// Application state
#[derive(Debug, Clone)]
pub struct AppState {
    pub config: PlatformConfig,
    pub db: Arc<db::UserDatabase>,
    pub jwt: Arc<auth::JwtManager>,
    pub users: DashMap<String, Arc<UserRuntime>>,
}

impl AppState {
    pub fn new(config: PlatformConfig) -> Result<Self> {
        let db = Arc::new(
            db::UserDatabase::open(&config.data_dir)
                .context("initialize user database")?,
        );

        let jwt_config = auth::JwtConfig::from_env()
            .context("JWT configuration from environment")?;
        let jwt = Arc::new(auth::JwtManager::new(jwt_config));

        Ok(Self {
            config,
            db,
            jwt,
            users: DashMap::new(),
        })
    }

    pub fn get_or_create_user_runtime(&self, user_id: &str) -> Result<Arc<UserRuntime>, anyhow::Error> {
        // Try to get existing user runtime first (read-only, fast path)
        if let Some(existing) = self.users.get(user_id) {
            return Ok(existing.clone());
        }

        // Create new user runtime with proper error handling
        let user_runtime = Arc::new(
            UserRuntime::new(
                user_id.to_string(),
                Arc::new(self.config.user_runtime.clone()),
            ).context("create user runtime")?
        );

        // Try to insert - another thread might have created it first
        let entry = self.users.entry(user_id.to_string());
        match entry {
            dashmap::Entry::Occupied(existing) => {
                // Another thread beat us to it, return their runtime
                Ok(Arc::clone(existing.get()))
            }
            dashmap::Entry::Vacant(vacant) => {
                // We won the race, insert our runtime
                Ok(Arc::clone(&vacant.insert(user_runtime)))
            }
        }
    }
}

// ============ Auth API Types ============

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub user: db::UserResponse,
}

#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub user: db::UserResponse,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

impl IntoResponse for ErrorResponse {
    fn into_response(self) -> Response {
        (StatusCode::BAD_REQUEST, Json(self)).into_response()
    }
}

fn extract_bearer_token(headers: &axum::http::HeaderMap) -> Result<String, ErrorResponse> {
    headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
        .ok_or_else(|| ErrorResponse {
            error: "Missing or invalid Authorization header".to_string(),
        })
}

/// Type alias for Arc<AppState>
pub type ArcAppState = Arc<AppState>;

/// User ID extracted from JWT
#[derive(Debug, Clone)]
pub struct AuthExtractor {
    pub user_id: String,
    pub email: String,
    pub role: String,
}

impl<S> FromRequestParts<S> for AuthExtractor
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, Json<ErrorResponse>);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Get state from extensions instead - this is how Axum State extraction works
        let state = parts
            .extensions
            .get::<ArcAppState>()
            .cloned()
            .ok_or_else(|| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse {
                error: "State not found".to_string(),
            })))?;

        let token = extract_bearer_token(&parts.headers)
            .map_err(|e| (StatusCode::UNAUTHORIZED, Json(e)))?;

        let claims = state
            .jwt
            .verify_token(&token)
            .map_err(|_| (StatusCode::UNAUTHORIZED, Json(ErrorResponse {
                error: "Invalid token".to_string(),
            })))?;

        Ok(Self {
            user_id: claims.claims.sub,
            email: claims.claims.email,
            role: claims.claims.role,
        })
    }
}

// ============ Auth Handlers ============

async fn register(
    State(state): State<Arc<AppState>>,
    Json(req): Json<db::RegisterRequest>,
) -> Result<Json<RegisterResponse>, ErrorResponse> {
    let user = state
        .db
        .register(&req)
        .map_err(|_| ErrorResponse { error: "Failed to register user".to_string() })?;

    Ok(Json(RegisterResponse { user }))
}

async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<db::LoginRequest>,
) -> Result<Json<LoginResponse>, ErrorResponse> {
    let user = state
        .db
        .authenticate(&req)
        .map_err(|_| ErrorResponse { error: "Invalid credentials".to_string() })?;

    let access_token = state
        .jwt
        .generate_access_token(&user.id, &user.email, &user.role.to_string())
        .map_err(|_| ErrorResponse {
            error: "Failed to generate access token".to_string(),
        })?;

    let refresh_token = state
        .jwt
        .generate_refresh_token(&user.id, &user.email, &user.role.to_string())
        .map_err(|_| ErrorResponse {
            error: "Failed to generate refresh token".to_string(),
        })?;

    Ok(Json(LoginResponse {
        access_token,
        refresh_token,
        user,
    }))
}

#[derive(Debug, Deserialize)]
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
        .get_user(&claims.sub)
        .map_err(|_| ErrorResponse { error: "Failed to get user".to_string() })?
        .ok_or_else(|| ErrorResponse {
            error: "User not found".to_string(),
        })?;

    let access_token = state
        .jwt
        .generate_access_token(&user.id, &user.email, &user.role.to_string())
        .map_err(|_| ErrorResponse {
            error: "Failed to generate access token".to_string(),
        })?;

    let refresh_token = state
        .jwt
        .generate_refresh_token(&user.id, &user.email, &user.role.to_string())
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
    let token = extract_bearer_token(request.headers())?;

    let claims = state
        .jwt
        .verify_token(&token)
        .map_err(|_| ErrorResponse {
            error: "Invalid token".to_string(),
        })?;

    let user = state
        .db
        .get_user(&claims.claims.sub)
        .map_err(|_| ErrorResponse { error: "Failed to get user".to_string() })?
        .ok_or_else(|| ErrorResponse {
            error: "User not found".to_string(),
        })?;

    Ok(Json(user))
}

// ============ Main ============

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
        .route("/api/sessions", get(api::sessions::list_sessions))
        .route("/api/sessions", post(api::sessions::create_session))
        .route("/api/sessions/{session_id}", get(api::sessions::get_session))
        .route("/api/sessions/{session_id}", delete(api::sessions::delete_session))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = format!("{}:{}", config.host, config.port);
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("Listening on {}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}
