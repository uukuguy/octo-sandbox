# P1-3: PlatformState + Per-User AgentRuntime Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 实现 PlatformState + 每用户独立 AgentRuntime，支持多用户登录和会话管理

**Architecture:** 三层架构 - AppState (Platform) → UserRuntime (per-user) → Session/AgentRuntime (per-session)，使用 DashMap 实现懒加载和并发控制

**Tech Stack:** Rust, Axum, DashMap, tokio, octo-engine (AgentRuntime)

---

## Task 1: Define UserRuntimeConfig and Extend PlatformConfig

**Files:**
- Modify: `crates/octo-platform-server/src/main.rs:25-41`

**Step 1: Add UserRuntimeConfig struct**

Add after PlatformConfig definition:

```rust
/// User runtime configuration
#[derive(Debug, Clone)]
pub struct UserRuntimeConfig {
    pub max_concurrent_agents: u32,     // 默认 3
    pub session_timeout_minutes: u32,    // 默认 30
    pub db_path_template: String,        // "data-platform/users/{user_id}"
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
```

**Step 2: Extend PlatformConfig with user_runtime**

Modify PlatformConfig:

```rust
#[derive(Debug, Clone)]
pub struct PlatformConfig {
    pub host: String,
    pub port: u16,
    pub data_dir: PathBuf,
    pub user_runtime: UserRuntimeConfig,  // 新增
}
```

Modify Default impl:

```rust
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
```

**Step 3: Run cargo check**

Run: `cd crates/octo-platform-server && cargo check`
Expected: SUCCESS

**Step 4: Commit**

```bash
git add crates/octo-platform-server/src/main.rs
git commit -m "feat: add UserRuntimeConfig to PlatformConfig"
```

---

## Task 2: Create UserRuntime Module

**Files:**
- Create: `crates/octo-platform-server/src/user_runtime.rs`
- Modify: `crates/octo-platform-server/src/main.rs`

**Step 1: Create user_runtime.rs**

```rust
//! User runtime management - per-user AgentRuntime lifecycle

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// User runtime - one per user, manages sessions
pub struct UserRuntime {
    pub user_id: String,
    pub config: Arc<UserRuntimeConfig>,
    pub sessions: DashMap<String, Session>,
    pub db_path: PathBuf,
}

/// Session - one per conversation
#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub user_id: String,
    pub name: Option<String>,
    pub status: SessionStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Active,
    Paused,
    Completed,
}

impl Default for SessionStatus {
    fn default() -> Self {
        SessionStatus::Active
    }
}

impl Session {
    pub fn new(user_id: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            user_id,
            name: None,
            status: SessionStatus::Active,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }
}

impl UserRuntime {
    pub fn new(user_id: String, config: Arc<UserRuntimeConfig>) -> Result<Self> {
        let db_path = PathBuf::from(
            config.db_path_template.replace("{user_id}", &user_id)
        );

        // Ensure directory exists
        std::fs::create_dir_all(&db_path)
            .context("create user data directory")?;

        tracing::info!("UserRuntime created for user: {} at {:?}", user_id, db_path);

        Ok(Self {
            user_id,
            config,
            sessions: DashMap::new(),
            db_path,
        })
    }

    pub fn create_session(&self, name: Option<String>) -> Result<Session> {
        // Check concurrent limit
        let current = self.sessions.len() as u32;
        if current >= self.config.max_concurrent_agents {
            anyhow::bail!(
                "Concurrent session limit exceeded: max {}, current {}",
                self.config.max_concurrent_agents,
                current
            );
        }

        let session = match name {
            Some(n) => Session::new(self.user_id.clone()).with_name(n),
            None => Session::new(self.user_id.clone()),
        };

        self.sessions.insert(session.id.clone(), session.clone());
        Ok(session)
    }

    pub fn get_session(&self, session_id: &str) -> Option<Session> {
        self.sessions.get(session_id).map(|s| s.clone())
    }

    pub fn list_sessions(&self) -> Vec<Session> {
        self.sessions.iter().map(|s| s.clone()).collect()
    }

    pub fn delete_session(&self, session_id: &str) -> bool {
        self.sessions.remove(session_id).is_some()
    }
}
```

**Step 2: Export module in main.rs**

Add at top of main.rs:

```rust
pub mod user_runtime;
```

Add after AppState definition:

```rust
use dashmap::DashMap;
```

**Step 3: Run cargo check**

Run: `cd crates/octo-platform-server && cargo check`
Expected: SUCCESS

**Step 4: Commit**

```bash
git add crates/octo-platform-server/src/user_runtime.rs crates/octo-platform-server/src/main.rs
git commit -m "feat: create UserRuntime module with session management"
```

---

## Task 3: Integrate UserRuntime into AppState

**Files:**
- Modify: `crates/octo-platform-server/src/main.rs:43-65`

**Step 1: Add users DashMap to AppState**

Modify AppState:

```rust
#[derive(Debug, Clone)]
pub struct AppState {
    pub config: PlatformConfig,
    pub db: Arc<db::UserDatabase>,
    pub jwt: Arc<auth::JwtManager>,
    pub users: DashMap<String, Arc<UserRuntime>>,  // 新增
}
```

**Step 2: Initialize users DashMap in AppState::new**

Modify AppState::new:

```rust
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
}
```

**Step 3: Add helper method to get/create UserRuntime**

Add after AppState::new:

```rust
impl AppState {
    pub fn get_or_create_user_runtime(&self, user_id: &str) -> Arc<UserRuntime> {
        self.users
            .entry(user_id.to_string())
            .or_insert_with(|| {
                Arc::new(
                    UserRuntime::new(
                        user_id.to_string(),
                        Arc::new(self.config.user_runtime.clone()),
                    ).expect("create user runtime")
                )
            })
            .clone()
    }
}
```

**Step 4: Run cargo check**

Run: `cd crates/octo-platform-server && cargo check`
Expected: SUCCESS

**Step 5: Commit**

```bash
git add crates/octo-platform-server/src/main.rs
git commit -m "feat: integrate UserRuntime DashMap into AppState"
```

---

## Task 4: Implement Session CRUD API

**Files:**
- Create: `crates/octo-platform-server/src/api/sessions.rs`
- Modify: `crates/octo-platform-server/src/main.rs`

**Step 1: Create sessions.rs**

```rust
//! Session API handlers

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::user_runtime::{Session, SessionStatus};

/// Request to create a session
#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub name: Option<String>,
}

/// Response for a session
#[derive(Debug, Serialize)]
pub struct SessionResponse {
    pub id: String,
    pub user_id: String,
    pub name: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<Session> for SessionResponse {
    fn from(s: Session) -> Self {
        Self {
            id: s.id,
            user_id: s.user_id,
            name: s.name,
            status: match s.status {
                SessionStatus::Active => "active".to_string(),
                SessionStatus::Paused => "paused".to_string(),
                SessionStatus::Completed => "completed".to_string(),
            },
            created_at: s.created_at.to_rfc3339(),
            updated_at: s.updated_at.to_rfc3339(),
        }
    }
}

/// List all sessions for current user
pub async fn list_sessions(
    State(state): State<super::ArcAppState>,
    request: super::AuthExtractor,
) -> Result<Json<Vec<SessionResponse>>, super::ErrorResponse> {
    let user_runtime = state.get_or_create_user_runtime(&request.user_id);
    let sessions = user_runtime.list_sessions();

    Ok(Json(sessions.into_iter().map(|s| s.into()).collect()))
}

/// Create a new session
pub async fn create_session(
    State(state): State<super::ArcAppState>,
    request: super::AuthExtractor,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<SessionResponse>, super::ErrorResponse> {
    let user_runtime = state.get_or_create_user_runtime(&request.user_id);

    let session = user_runtime
        .create_session(req.name)
        .map_err(|e| super::ErrorResponse { error: e.to_string() })?;

    Ok(Json(session.into()))
}

/// Get a specific session
pub async fn get_session(
    State(state): State<super::ArcAppState>,
    request: super::AuthExtractor,
    Path(session_id): Path<String>,
) -> Result<Json<SessionResponse>, super::ErrorResponse> {
    let user_runtime = state.get_or_create_user_runtime(&request.user_id);

    let session = user_runtime
        .get_session(&session_id)
        .ok_or_else(|| super::ErrorResponse {
            error: "Session not found".to_string(),
        })?;

    Ok(Json(session.into()))
}

/// Delete a session
pub async fn delete_session(
    State(state): State<super::ArcAppState>,
    request: super::AuthExtractor,
    Path(session_id): Path<String>,
) -> Result<StatusCode, super::ErrorResponse> {
    let user_runtime = state.get_or_create_user_runtime(&request.user_id);

    let deleted = user_runtime.delete_session(&session_id);
    if !deleted {
        return Err(super::ErrorResponse {
            error: "Session not found".to_string(),
        });
    }

    Ok(StatusCode::NO_CONTENT)
}
```

**Step 2: Add AuthExtractor helper in main.rs**

Add after extract_bearer_token function:

```rust
/// Authenticated request extractor
#[derive(Debug)]
pub struct AuthExtractor {
    pub user_id: String,
    pub email: String,
    pub role: String,
}

impl AuthExtractor {
    fn from_headers(headers: &axum::http::HeaderMap) -> Result<Self, ErrorResponse> {
        let token = extract_bearer_token(headers)?;
        let claims = state
            .jwt
            .verify_token(&token)
            .map_err(|_| ErrorResponse {
                error: "Invalid token".to_string(),
            })?;

        Ok(Self {
            user_id: claims.claims.sub,
            email: claims.claims.email,
            role: claims.claims.role,
        })
    }
}
```

Wait - this won't work because we need state. Let me revise:

```rust
/// Authenticated request extractor - extract user info from JWT
pub async fn require_auth(
    State(state): State<Arc<AppState>>,
    request: axum::http::Request<axum::body::Body>,
) -> Result<(Arc<AppState>, AuthExtractor), (StatusCode, Json<ErrorResponse>)> {
    let token = extract_bearer_token(request.headers())
        .map_err(|e| (StatusCode::UNAUTHORIZED, Json(e)))?;

    let claims = state
        .jwt
        .verify_token(&token)
        .map_err(|_| (StatusCode::UNAUTHORIZED, Json(ErrorResponse {
            error: "Invalid token".to_string(),
        })))?;

    let auth = AuthExtractor {
        user_id: claims.claims.sub,
        email: claims.claims.email,
        role: claims.claims.role,
    };

    Ok((state, auth))
}
```

**Step 3: Simplify - use middleware pattern instead**

Actually, let's keep it simpler. Modify the handlers directly to extract user_id from JWT.

Update sessions.rs to import AppState:

```rust
use crate::AppState;
```

Update handler signatures - we need a different approach. Let's create an extractor:

```rust
/// Extract user_id from JWT in Authorization header
pub async fn extract_user_id(
    State(state): State<Arc<AppState>>,
    headers: &axum::http::HeaderMap,
) -> Result<String, ErrorResponse> {
    let token = extract_bearer_token(headers)?;
    let claims = state
        .jwt
        .verify_token(&token)
        .map_err(|_| ErrorResponse {
            error: "Invalid token".to_string(),
        })?;
    Ok(claims.claims.sub)
}
```

Then use it in handlers:

```rust
pub async fn list_sessions(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<SessionResponse>>, ErrorResponse> {
    let user_id = extract_user_id(State(state), headers).await?;
    let user_runtime = state.get_or_create_user_runtime(&user_id);
    let sessions = user_runtime.list_sessions();
    Ok(Json(sessions.into_iter().map(|s| s.into()).collect()))
}
```

Actually this is getting complex. Let me create a simpler extractor pattern:

**Step 2 (Revised): Add typed extractor**

Add to main.rs:

```rust
use axum::{
    extract::{FromRef, FromRequestParts},
    http::{request::Parts, StatusCode},
};

/// User ID extracted from JWT
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub user_id: String,
    pub email: String,
    pub role: String,
}

impl<S> FromRequestParts<S> for CurrentUser
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, Json<ErrorResponse>);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let state = ArcAppState::from_request_parts(parts, state)
            .await
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse {
                error: "State not found".to_string(),
            })))?;

        let token = extract_bearer_token(parts.headers())
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
```

Add type alias:

```rust
type ArcAppState = Arc<AppState>;
```

**Step 3: Update sessions.rs handlers to use CurrentUser**

```rust
use crate::{CurrentUser, ErrorResponse};

pub async fn list_sessions(
    State(state): State<ArcAppState>,
    current_user: CurrentUser,
) -> Result<Json<Vec<SessionResponse>>, ErrorResponse> {
    let user_runtime = state.get_or_create_user_runtime(&current_user.user_id);
    let sessions = user_runtime.list_sessions();
    Ok(Json(sessions.into_iter().map(|s| s.into()).collect()))
}

pub async fn create_session(
    State(state): State<ArcAppState>,
    current_user: CurrentUser,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<SessionResponse>, ErrorResponse> {
    let user_runtime = state.get_or_create_user_runtime(&current_user.user_id);
    let session = user_runtime
        .create_session(req.name)
        .map_err(|e| ErrorResponse { error: e.to_string() })?;
    Ok(Json(session.into()))
}

pub async fn get_session(
    State(state): State<ArcAppState>,
    current_user: CurrentUser,
    Path(session_id): Path<String>,
) -> Result<Json<SessionResponse>, ErrorResponse> {
    let user_runtime = state.get_or_create_user_runtime(&current_user.user_id);
    let session = user_runtime
        .get_session(&session_id)
        .ok_or_else(|| ErrorResponse { error: "Session not found".to_string() })?;
    Ok(Json(session.into()))
}

pub async fn delete_session(
    State(state): State<ArcAppState>,
    current_user: CurrentUser,
    Path(session_id): Path<String>,
) -> Result<StatusCode, ErrorResponse> {
    let user_runtime = state.get_or_create_user_runtime(&current_user.user_id);
    let deleted = user_runtime.delete_session(&session_id);
    if !deleted {
        return Err(ErrorResponse { error: "Session not found".to_string() });
    }
    Ok(StatusCode::NO_CONTENT)
}
```

**Step 4: Register routes in main.rs**

Add module:

```rust
pub mod api;
```

Update router:

```rust
let app = Router::new()
    .route("/health", get(health))
    .route("/api/auth/register", post(register))
    .route("/api/auth/login", post(login))
    .route("/api/auth/refresh", post(refresh))
    .route("/api/auth/me", get(me))
    // 新增
    .route("/api/sessions", get(api::sessions::list_sessions))
    .route("/api/sessions", post(api::sessions::create_session))
    .route("/api/sessions/{session_id}", get(api::sessions::get_session))
    .route("/api/sessions/{session_id}", delete(api::sessions::delete_session))
    .layer(TraceLayer::new_for_http())
    .with_state(state);
```

Wait - we need to organize the module first. Let me create the api directory structure.

Actually, simpler: just put sessions in main.rs for now or create a simple api/mod.rs.

Let's create api/mod.rs:

```rust
pub mod sessions;
```

**Step 5: Run cargo check**

Run: `cd crates/octo-platform-server && cargo check`
Expected: SUCCESS

**Step 6: Commit**

```bash
git add crates/octo-platform-server/src/api/
git commit -m "feat: implement Session CRUD API"
```

---

## Task 5: Implement WebSocket Handler

**Files:**
- Create: `crates/octo-platform-server/src/ws.rs`
- Modify: `crates/octo-platform-server/src/main.rs`

**Step 1: Create ws.rs**

```rust
//! WebSocket handler for real-time agent communication

use std::{sync::Arc, time::Duration};

use anyhow::Result;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    response::IntoResponse,
    Extractor,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::{AppState, CurrentUser, ErrorResponse};

/// WebSocket message from client
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    #[serde(rename = "chat")]
    Chat { content: String },
    #[serde(rename = "ping")]
    Ping,
}

/// WebSocket message to client
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    #[serde(rename = "response")]
    Response { content: String, done: bool },
    #[serde(rename = "error")]
    Error { message: String },
    #[serde(rename = "pong")]
    Pong,
}

/// WebSocket handler
pub async fn ws_handler(
    State(state): State<Arc<AppState>>,
    current_user: CurrentUser,
    Path(session_id): Path<String>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(state, current_user.user_id, session_id, socket))
}

async fn handle_socket(state: Arc<AppState>, user_id: String, session_id: String, socket: WebSocket) {
    let (mut sender, mut receiver) = socket.split();

    // Create a channel for sending messages back to client
    let (tx, mut rx) = mpsc::channel::<ServerMessage>(100);

    // Spawn task to forward messages to WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let text = serde_json::to_string(&msg).unwrap_or_default();
            if sender.send(Message::Text(text)).await.is_err() {
                break;
            }
        }
    });

    // Handle incoming messages
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) {
                    match client_msg {
                        ClientMessage::Chat { content } => {
                            // TODO: Integrate with AgentRuntime
                            let response = ServerMessage::Response {
                                content: format!("Echo: {}", content),
                                done: true,
                            };
                            let _ = tx.send(response).await;
                        }
                        ClientMessage::Ping => {
                            let _ = tx.send(ServerMessage::Pong).await;
                        }
                    }
                }
            }
            Ok(Message::Close(_)) => break,
            Err(_) => break,
            _ => {}
        }
    }

    send_task.abort();
    tracing::info!("WebSocket closed for session: {}", session_id);
}
```

**Step 2: Add ws module and routes in main.rs**

Add:

```rust
pub mod ws;
```

Update router (need tokio-tungstenite for WS support):

```rust
use crate::ws::ws_handler;

// In router:
.route("/ws/{session_id}", get(ws_handler))
```

**Step 3: Add dependencies**

Check Cargo.toml and add:

```toml
futures-util = "0.3"
tokio-tungstenite = "0.21"
```

**Step 4: Run cargo check**

Run: `cd crates/octo-platform-server && cargo check`
Expected: SUCCESS

**Step 5: Commit**

```bash
git add crates/octo-platform-server/src/ws.rs crates/octo-platform-server/src/main.rs crates/octo-platform-server/Cargo.toml
git commit -m "feat: implement WebSocket handler for real-time communication"
```

---

## Task 6: Integrate AgentRuntime (Stub for Now)

**Files:**
- Modify: `crates/octo-platform-server/src/ws.rs`

**Step 1: Add AgentRuntime stub in ws.rs**

For now, just echo back messages. Full AgentRuntime integration is P1-4.

Add comment:

```rust
// TODO: Integrate with AgentRuntime (P1-4)
// For now, just echo the message back
let response = ServerMessage::Response {
    content: format!("[Stub] Received: {}", content),
    done: true,
};
```

**Step 2: Commit**

```bash
git commit -m "chore: add AgentRuntime integration placeholder"
```

---

## Task 7: Write Unit Tests

**Files:**
- Create: `crates/octo-platform-server/tests/test_user_runtime.rs`

**Step 1: Create test file**

```rust
//! Tests for UserRuntime

use octo_platform_server::user_runtime::{Session, SessionStatus, UserRuntime, UserRuntimeConfig};
use std::sync::Arc;

fn create_test_config() -> UserRuntimeConfig {
    UserRuntimeConfig {
        max_concurrent_agents: 3,
        session_timeout_minutes: 30,
        db_path_template: "data-platform/test/users/{user_id}".to_string(),
    }
}

#[test]
fn test_user_runtime_creation() {
    let runtime = UserRuntime::new("test-user-1".to_string(), Arc::new(create_test_config()));
    assert!(runtime.is_ok());
    let runtime = runtime.unwrap();
    assert_eq!(runtime.user_id, "test-user-1");
    assert!(runtime.sessions.is_empty());
}

#[test]
fn test_create_session() {
    let runtime = UserRuntime::new("test-user-1".to_string(), Arc::new(create_test_config())).unwrap();

    let session = runtime.create_session(None);
    assert!(session.is_ok());
    let session = session.unwrap();
    assert_eq!(session.user_id, "test-user-1");
    assert_eq!(session.status, SessionStatus::Active);
}

#[test]
fn test_create_named_session() {
    let runtime = UserRuntime::new("test-user-1".to_string(), Arc::new(create_test_config())).unwrap();

    let session = runtime.create_session(Some("My Session".to_string()));
    assert!(session.is_ok());
    let session = session.unwrap();
    assert_eq!(session.name, Some("My Session".to_string()));
}

#[test]
fn test_concurrent_session_limit() {
    let runtime = UserRuntime::new("test-user-1".to_string(), Arc::new(create_test_config())).unwrap();

    // Create 3 sessions (at limit)
    for _ in 0..3 {
        let result = runtime.create_session(None);
        assert!(result.is_ok());
    }

    // Try to create 4th - should fail
    let result = runtime.create_session(None);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Concurrent session limit"));
}

#[test]
fn test_get_session() {
    let runtime = UserRuntime::new("test-user-1".to_string(), Arc::new(create_test_config())).unwrap();

    let session = runtime.create_session(None).unwrap();
    let retrieved = runtime.get_session(&session.id);

    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().id, session.id);
}

#[test]
fn test_delete_session() {
    let runtime = UserRuntime::new("test-user-1".to_string(), Arc::new(create_test_config())).unwrap();

    let session = runtime.create_session(None).unwrap();
    assert_eq!(runtime.sessions.len(), 1);

    let deleted = runtime.delete_session(&session.id);
    assert!(deleted);
    assert_eq!(runtime.sessions.len(), 0);
}

#[test]
fn test_list_sessions() {
    let runtime = UserRuntime::new("test-user-1".to_string(), Arc::new(create_test_config())).unwrap();

    runtime.create_session(None).unwrap();
    runtime.create_session(Some("Session 2".to_string())).unwrap();

    let sessions = runtime.list_sessions();
    assert_eq!(sessions.len(), 2);
}
```

**Step 2: Run tests**

Run: `cd crates/octo-platform-server && cargo test`
Expected: All tests pass

**Step 3: Commit**

```bash
git add crates/octo-platform-server/tests/
git commit -m "test: add UserRuntime unit tests"
```

---

## Summary

| Task | Description | Status |
|------|-------------|--------|
| 1 | UserRuntimeConfig + PlatformConfig | ⏳ |
| 2 | Create user_runtime.rs module | ⏳ |
| 3 | Integrate UserRuntime into AppState | ⏳ |
| 4 | Session CRUD API | ⏳ |
| 5 | WebSocket handler | ⏳ |
| 6 | AgentRuntime stub | ⏳ |
| 7 | Unit tests | ⏳ |

---

## Plan complete and saved to `docs/plans/2026-03-04-p1-3-platform-state-implementation.md`.

**Two execution options:**

1. **Subagent-Driven (this session)** - I dispatch fresh subagent per task, review between tasks, fast iteration

2. **Parallel Session (separate)** - Open new session with executing-plans, batch execution with checkpoints

**Which approach?**
