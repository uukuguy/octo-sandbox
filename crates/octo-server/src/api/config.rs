use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::state::AppState;

/// Frontend configuration - subset of server config needed by frontend
#[derive(Serialize)]
pub struct FrontendConfig {
    /// Server host
    pub host: String,
    /// Server port
    pub port: u16,
    /// Backend API base URL
    pub api_url: String,
    /// WebSocket URL for real-time communication
    pub ws_url: String,
    /// MCP servers directory (if configured)
    pub mcp_servers_dir: Option<String>,
    /// Provider name (e.g., "anthropic", "openai")
    pub provider: String,
    /// Model being used (if set)
    pub model: Option<String>,
}

/// Get frontend configuration (merges base config with runtime overrides)
pub async fn get_config(State(state): State<Arc<AppState>>) -> Json<FrontendConfig> {
    let host = state.config.server.host.clone();
    let port = state.config.server.port;

    let api_url = format!("http://{}:{}", host, port);
    let ws_url = format!("ws://{}:{}", host, port);

    let overrides = state.runtime_overrides.read().await;
    let provider = overrides
        .provider_name
        .clone()
        .unwrap_or_else(|| state.config.provider.name.clone());
    let model = overrides
        .provider_model
        .clone()
        .or_else(|| state.config.provider.model.clone());

    Json(FrontendConfig {
        host,
        port,
        api_url,
        ws_url,
        mcp_servers_dir: state.config.mcp.servers_dir.clone(),
        provider,
        model,
    })
}

// ── AO-T8: Runtime Config Update ─────────────────────────────────────

/// Request body for PUT /config — all fields optional (partial update).
#[derive(Debug, Deserialize)]
pub struct ConfigUpdateRequest {
    pub logging_format: Option<String>,
    pub cors_strict: Option<bool>,
    pub cors_origins: Option<Vec<String>>,
    pub provider_name: Option<String>,
    pub provider_model: Option<String>,
    pub autonomy_level: Option<String>,
    // Non-updatable fields — if present, reject with 400
    pub port: Option<serde_json::Value>,
    pub host: Option<serde_json::Value>,
    pub db_path: Option<serde_json::Value>,
    pub tls: Option<serde_json::Value>,
}

#[derive(Serialize)]
pub struct ConfigUpdateResponse {
    pub updated_fields: Vec<String>,
    pub restart_required: bool,
}

/// PUT /config — runtime configuration hot-update (AO-T8)
pub async fn update_config(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ConfigUpdateRequest>,
) -> Result<Json<ConfigUpdateResponse>, (StatusCode, Json<serde_json::Value>)> {
    // Reject non-updatable fields
    let mut rejected = Vec::new();
    if req.port.is_some() {
        rejected.push("port");
    }
    if req.host.is_some() {
        rejected.push("host");
    }
    if req.db_path.is_some() {
        rejected.push("db_path");
    }
    if req.tls.is_some() {
        rejected.push("tls");
    }
    if !rejected.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "non_updatable_fields",
                "message": format!("These fields require server restart: {}", rejected.join(", ")),
                "fields": rejected,
            })),
        ));
    }

    let mut overrides = state.runtime_overrides.write().await;
    let mut updated = Vec::new();

    if let Some(ref fmt) = req.logging_format {
        if matches!(fmt.as_str(), "pretty" | "json") {
            if overrides.logging_format.as_deref() != Some(fmt) {
                overrides.logging_format = Some(fmt.clone());
                updated.push("logging_format".to_string());
            }
        } else {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid_value",
                    "message": "logging_format must be 'pretty' or 'json'",
                })),
            ));
        }
    }

    if let Some(strict) = req.cors_strict {
        if overrides.cors_strict != Some(strict) {
            overrides.cors_strict = Some(strict);
            updated.push("cors_strict".to_string());
        }
    }

    if let Some(ref origins) = req.cors_origins {
        if overrides.cors_origins.as_ref() != Some(origins) {
            overrides.cors_origins = Some(origins.clone());
            updated.push("cors_origins".to_string());
        }
    }

    if let Some(ref name) = req.provider_name {
        if overrides.provider_name.as_deref() != Some(name) {
            overrides.provider_name = Some(name.clone());
            updated.push("provider_name".to_string());
        }
    }

    if let Some(ref model) = req.provider_model {
        if overrides.provider_model.as_deref() != Some(model) {
            overrides.provider_model = Some(model.clone());
            updated.push("provider_model".to_string());
        }
    }

    if let Some(ref level) = req.autonomy_level {
        if overrides.autonomy_level.as_deref() != Some(level) {
            overrides.autonomy_level = Some(level.clone());
            updated.push("autonomy_level".to_string());
        }
    }

    Ok(Json(ConfigUpdateResponse {
        updated_fields: updated,
        restart_required: false,
    }))
}
