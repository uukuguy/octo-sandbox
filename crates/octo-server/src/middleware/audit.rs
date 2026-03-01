//! Audit middleware for Octo Server
//!
//! Logs HTTP requests to the audit log for security and compliance.

use std::sync::Arc;

use axum::{
    body::Body,
    extract::Request,
    middleware::Next,
    response::Response,
};
use octo_engine::audit::{AuditEvent, AuditStorage};
use octo_engine::auth::UserContext;
use std::path::PathBuf;
use tokio::sync::RwLock;

/// State for audit middleware - holds database path
#[derive(Clone)]
pub struct AuditMiddlewareState {
    /// Path to the SQLite database
    db_path: Arc<RwLock<PathBuf>>,
}

impl AuditMiddlewareState {
    /// Create new audit middleware state
    pub fn new(db_path: PathBuf) -> Self {
        Self {
            db_path: Arc::new(RwLock::new(db_path)),
        }
    }

    /// Get the current database path
    async fn get_db_path(&self) -> PathBuf {
        self.db_path.read().await.clone()
    }
}

/// Audit middleware - logs HTTP requests to audit log
///
/// Logs the following information:
/// - HTTP method and path
/// - Response status code
/// - Request duration
/// - User ID (if authenticated)
/// - Client IP address
pub async fn audit_middleware(
    state: axum::extract::State<AuditMiddlewareState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let start = std::time::Instant::now();

    // Extract user_id from extensions (set by auth middleware)
    let user_id = req
        .extensions()
        .get::<UserContext>()
        .and_then(|u| u.user_id.clone());

    // Extract client IP
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
        });

    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let query = req.uri().query().map(|s| s.to_string());

    // Execute the request
    let response = next.run(req).await;

    // Calculate duration
    let duration_ms = start.elapsed().as_millis() as u64;
    let status = response.status().as_u16();

    // Log asynchronously (don't block the response)
    let db_path = state.get_db_path().await;
    let log_user_id = user_id;
    let log_client_ip = client_ip;
    let log_method = method;
    let log_path = path;
    let log_query = query;
    let log_status = status;
    let log_duration = duration_ms;

    tokio::spawn(async move {
        let storage = match AuditStorage::new(&db_path) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Failed to create audit storage: {}", e);
                return;
            }
        };

        let event = AuditEvent {
            event_type: "http_request".to_string(),
            user_id: log_user_id,
            session_id: None,
            resource_id: None,
            action: format!("{} {}", log_method, log_path),
            result: if log_status >= 400 { "failure" } else { "success" }.to_string(),
            metadata: Some(serde_json::json!({
                "method": log_method,
                "path": log_path,
                "query": log_query,
                "status": log_status,
                "duration_ms": log_duration,
            })),
            ip_address: log_client_ip,
        };

        if let Err(e) = storage.log(event) {
            tracing::error!("Failed to log audit event: {}", e);
        }
    });

    response
}
