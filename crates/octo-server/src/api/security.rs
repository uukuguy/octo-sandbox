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
    #[allow(dead_code)]
    pub autonomy_level: Option<String>,
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
    Json(PolicyResponse {
        autonomy_level: format!("{:?}", policy.autonomy),
        workspace_only: policy.workspace_only,
        allowed_commands: policy.allowed_commands.clone(),
        forbidden_paths: policy.forbidden_paths.clone(),
        max_actions_per_hour: policy.max_actions_per_hour,
        require_approval_for_medium_risk: policy.require_approval_for_medium_risk,
        block_high_risk_commands: policy.block_high_risk_commands,
    })
}

async fn update_policy(
    Json(_body): Json<PolicyUpdateRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    // SecurityPolicy is behind Arc — direct mutation is not possible.
    // Runtime policy updates require server restart for P2 scope.
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({
            "error": "Runtime policy updates require server restart"
        })),
    )
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
