//! Security Policy and AI Defence API (AO-T4 + AO-T5)
//!
//! T4: Security policy inspection, tracker status, command risk assessment.
//! T5: AI defence scan (injection + PII), PII redaction, defence status.

use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

// ── T4 Response / Request types ─────────────────────────────────────────────

#[derive(Serialize)]
pub struct PolicyResponse {
    pub autonomy_level: String,
    pub workspace_only: bool,
    pub allowed_commands: Vec<String>,
    pub forbidden_paths: Vec<String>,
    pub max_actions_per_hour: u32,
    pub require_approval_for_medium_risk: bool,
    pub block_high_risk_commands: bool,
}

#[derive(Deserialize)]
pub struct PolicyUpdateRequest {
    pub autonomy_level: Option<String>,
    pub require_approval_for_medium_risk: Option<bool>,
    pub block_high_risk_commands: Option<bool>,
}

#[derive(Serialize)]
pub struct PolicyUpdateResponse {
    pub updated_fields: Vec<String>,
}

#[derive(Serialize)]
pub struct TrackerResponse {
    pub actions_in_window: usize,
    pub window_secs: u64,
}

#[derive(Deserialize)]
pub struct CheckCommandRequest {
    pub command: String,
}

#[derive(Serialize)]
pub struct CheckCommandResponse {
    pub command: String,
    pub risk_level: String,
    pub requires_approval: bool,
    pub allowed: bool,
    pub error: Option<String>,
}

// ── T5 Response / Request types ─────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ScanRequest {
    pub text: String,
}

#[derive(Serialize)]
pub struct ScanResponse {
    pub has_injection: bool,
    pub has_pii: bool,
    pub safe: bool,
}

#[derive(Deserialize)]
pub struct RedactRequest {
    pub text: String,
}

#[derive(Serialize)]
pub struct RedactResponse {
    pub redacted: String,
}

#[derive(Serialize)]
pub struct DefenceStatusResponse {
    pub injection_enabled: bool,
    pub pii_enabled: bool,
    pub output_validation_enabled: bool,
}

// ── Router ──────────────────────────────────────────────────────────────────

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/security/policy", get(get_policy).put(update_policy))
        .route("/security/tracker", get(get_tracker))
        .route("/security/check-command", post(check_command))
        .route("/security/scan", post(scan))
        .route("/security/pii/redact", post(redact_pii))
        .route("/security/defence/status", get(defence_status))
}

// ── T4 Handlers ─────────────────────────────────────────────────────────────

async fn get_policy(State(state): State<Arc<AppState>>) -> Json<PolicyResponse> {
    let policy = state.agent_supervisor.security_policy();
    let overrides = state.runtime_overrides.read().await;

    let autonomy_level = overrides
        .autonomy_level
        .clone()
        .unwrap_or_else(|| format!("{:?}", policy.autonomy));
    let require_approval = overrides
        .require_approval_for_medium_risk
        .unwrap_or(policy.require_approval_for_medium_risk);
    let block_high_risk = overrides
        .block_high_risk_commands
        .unwrap_or(policy.block_high_risk_commands);

    Json(PolicyResponse {
        autonomy_level,
        workspace_only: policy.workspace_only,
        allowed_commands: policy.allowed_commands.clone(),
        forbidden_paths: policy.forbidden_paths.clone(),
        max_actions_per_hour: policy.max_actions_per_hour,
        require_approval_for_medium_risk: require_approval,
        block_high_risk_commands: block_high_risk,
    })
}

async fn update_policy(
    State(state): State<Arc<AppState>>,
    Json(body): Json<PolicyUpdateRequest>,
) -> Result<Json<PolicyUpdateResponse>, (StatusCode, Json<serde_json::Value>)> {
    // Validate autonomy_level if provided
    if let Some(ref level) = body.autonomy_level {
        match level.to_lowercase().as_str() {
            "readonly" | "supervised" | "full" => {}
            _ => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "invalid_value",
                        "message": "autonomy_level must be 'readonly', 'supervised', or 'full'",
                    })),
                ));
            }
        }
    }

    let mut overrides = state.runtime_overrides.write().await;
    let mut updated = Vec::new();

    if let Some(ref level) = body.autonomy_level {
        if overrides.autonomy_level.as_deref() != Some(level) {
            overrides.autonomy_level = Some(level.clone());
            updated.push("autonomy_level".to_string());
        }
    }

    if let Some(val) = body.require_approval_for_medium_risk {
        overrides.require_approval_for_medium_risk = Some(val);
        updated.push("require_approval_for_medium_risk".to_string());
    }

    if let Some(val) = body.block_high_risk_commands {
        overrides.block_high_risk_commands = Some(val);
        updated.push("block_high_risk_commands".to_string());
    }

    Ok(Json(PolicyUpdateResponse {
        updated_fields: updated,
    }))
}

async fn get_tracker(State(state): State<Arc<AppState>>) -> Json<TrackerResponse> {
    let policy = state.agent_supervisor.security_policy();
    Json(TrackerResponse {
        actions_in_window: policy.tracker.count(),
        window_secs: 3600,
    })
}

async fn check_command(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CheckCommandRequest>,
) -> Json<CheckCommandResponse> {
    let policy = state.agent_supervisor.security_policy();
    let risk_level = policy.assess_command_risk(&body.command);
    let requires_approval = policy.requires_approval(&body.command);
    let check_result = policy.check_command(&body.command);

    Json(CheckCommandResponse {
        command: body.command,
        risk_level: format!("{:?}", risk_level),
        requires_approval,
        allowed: check_result.is_ok(),
        error: check_result.err(),
    })
}

// ── T5 Handlers ─────────────────────────────────────────────────────────────

async fn scan(Json(body): Json<ScanRequest>) -> Json<ScanResponse> {
    let defence = octo_engine::security::AiDefence::new();
    let has_injection = defence.has_injection(&body.text);
    let has_pii = defence.has_pii(&body.text);
    Json(ScanResponse {
        has_injection,
        has_pii,
        safe: !has_injection && !has_pii,
    })
}

async fn redact_pii(Json(body): Json<RedactRequest>) -> Json<RedactResponse> {
    let defence = octo_engine::security::AiDefence::new();
    Json(RedactResponse {
        redacted: defence.redact_pii(&body.text),
    })
}

async fn defence_status() -> Json<DefenceStatusResponse> {
    let defence = octo_engine::security::AiDefence::new();
    Json(DefenceStatusResponse {
        injection_enabled: defence.injection_enabled(),
        pii_enabled: defence.pii_enabled(),
        output_validation_enabled: defence.output_validation_enabled(),
    })
}
