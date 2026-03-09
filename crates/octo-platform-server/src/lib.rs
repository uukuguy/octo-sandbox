//! octo-platform-server library
//!
//! Multi-tenant multi-agent platform API server library.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::{
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use dashmap::DashMap;
use serde::Serialize;

pub mod agent_pool;
pub mod api;
pub mod audit;
pub mod auth;
pub mod db;
pub mod middleware;
pub mod tenant;
pub mod user_runtime;
pub mod ws;

// Re-export agent_pool types
pub use agent_pool::{
    AgentInstance, AgentPool, InstanceId, InstanceState, IsolationStrategy, PoolConfig, PoolStats,
    Workspace,
};

// Re-export user_runtime types
pub use user_runtime::{Session, SessionStatus, UserRuntime};

// Re-export tenant types
pub use tenant::{ResourceQuota, Tenant, TenantManager, TenantPlan, TenantRuntime};

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

/// Application state
#[derive(Debug, Clone)]
pub struct AppState {
    pub config: PlatformConfig,
    pub db: Arc<db::UserDatabase>,
    pub jwt: Arc<auth::JwtManager>,
    pub users: DashMap<String, Arc<UserRuntime>>,
    pub agent_pool: Arc<AgentPool>,
    pub tenant_manager: Arc<TenantManager>,
}

impl AppState {
    pub fn new(config: PlatformConfig) -> Result<Self> {
        let db =
            Arc::new(db::UserDatabase::open(&config.data_dir).context("initialize user database")?);

        let jwt_config =
            auth::JwtConfig::from_env().context("JWT configuration from environment")?;
        let jwt = Arc::new(auth::JwtManager::new(jwt_config));

        let agent_pool = Arc::new(AgentPool::new());

        let tenant_manager = Arc::new(
            TenantManager::new(config.data_dir.clone()).context("initialize tenant manager")?,
        );

        Ok(Self {
            config,
            db,
            jwt,
            users: DashMap::new(),
            agent_pool,
            tenant_manager,
        })
    }

    pub fn get_or_create_user_runtime(
        &self,
        user_id: &str,
    ) -> Result<Arc<UserRuntime>, anyhow::Error> {
        // Try to get existing user runtime first (read-only, fast path)
        if let Some(existing) = self.users.get(user_id) {
            return Ok(existing.clone());
        }

        // Create new user runtime with proper error handling
        let user_runtime = Arc::new(
            UserRuntime::new(
                user_id.to_string(),
                Arc::new(self.config.user_runtime.clone()),
            )
            .context("create user runtime")?,
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

    /// Get the agent pool
    pub fn agent_pool(&self) -> &Arc<AgentPool> {
        &self.agent_pool
    }
}

/// Type alias for Arc<AppState>
pub type ArcAppState = Arc<AppState>;

/// User ID extracted from JWT
#[derive(Debug, Clone)]
pub struct AuthExtractor {
    pub user_id: String,
    pub email: String,
    pub role: String,
    pub tenant_id: String,
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
            .ok_or_else(|| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "State not found".to_string(),
                    }),
                )
            })?;

        let token = extract_bearer_token(&parts.headers)
            .map_err(|e| (StatusCode::UNAUTHORIZED, Json(e)))?;

        let claims = state.jwt.verify_token(&token).map_err(|_| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Invalid token".to_string(),
                }),
            )
        })?;

        Ok(Self {
            user_id: claims.claims.sub,
            email: claims.claims.email,
            role: claims.claims.role,
            tenant_id: claims.claims.tenant_id,
        })
    }
}

/// Error response type
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

impl IntoResponse for ErrorResponse {
    fn into_response(self) -> Response {
        (StatusCode::BAD_REQUEST, Json(self)).into_response()
    }
}

/// Extract bearer token from Authorization header
pub fn extract_bearer_token(headers: &axum::http::HeaderMap) -> Result<String, ErrorResponse> {
    headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
        .ok_or_else(|| ErrorResponse {
            error: "Missing or invalid Authorization header".to_string(),
        })
}

/// Login response type
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub user: db::UserResponse,
}

/// Register response type
#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub user: db::UserResponse,
}
