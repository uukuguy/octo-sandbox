//! Metering API — token usage tracking and cost estimation (AO-T1)

use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct SnapshotResponse {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub requests: u64,
    pub errors: u64,
    pub duration_ms: u64,
    pub avg_tokens_per_request: f64,
    pub avg_duration_ms: f64,
}

#[derive(Debug, Serialize)]
pub struct ModelSummary {
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub requests: u64,
    pub errors: u64,
    pub duration_ms: u64,
    pub estimated_cost_usd: f64,
}

#[derive(Debug, Serialize)]
pub struct SummaryResponse {
    pub models: Vec<ModelSummary>,
    pub total_estimated_cost_usd: f64,
}

#[derive(Debug, Serialize)]
pub struct SessionSummary {
    pub session_id: String,
    pub models: Vec<ModelSummary>,
    pub total_estimated_cost_usd: f64,
}

#[derive(Debug, Deserialize)]
pub struct SessionQuery {
    pub session_id: Option<String>,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/metering/snapshot", get(snapshot))
        .route("/metering/summary", get(summary))
        .route("/metering/by-session", get(by_session))
        .route("/metering/reset", post(reset))
}

/// GET /api/v1/metering/snapshot — real-time token consumption
async fn snapshot(State(state): State<Arc<AppState>>) -> Json<SnapshotResponse> {
    let snap = state.agent_supervisor.metering();
    Json(SnapshotResponse {
        input_tokens: snap.input_tokens,
        output_tokens: snap.output_tokens,
        total_tokens: snap.total_tokens(),
        requests: snap.requests,
        errors: snap.errors,
        duration_ms: snap.duration_ms,
        avg_tokens_per_request: snap.avg_tokens_per_request(),
        avg_duration_ms: snap.avg_duration_ms(),
    })
}

/// GET /api/v1/metering/summary — accumulated usage by model with cost estimation
async fn summary(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SummaryResponse>, StatusCode> {
    let storage = state
        .metering_storage()
        .await
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let summaries = storage
        .summary_by_model()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let models: Vec<ModelSummary> = summaries
        .iter()
        .map(|s| {
            let cost =
                octo_engine::metering::storage::MeteringStorage::estimate_cost(s);
            ModelSummary {
                model: s.model.clone(),
                input_tokens: s.total_input_tokens,
                output_tokens: s.total_output_tokens,
                total_tokens: s.total_input_tokens + s.total_output_tokens,
                requests: s.total_requests,
                errors: s.total_errors,
                duration_ms: s.total_duration_ms,
                estimated_cost_usd: cost,
            }
        })
        .collect();

    let total_cost: f64 = models.iter().map(|m| m.estimated_cost_usd).sum();

    Ok(Json(SummaryResponse {
        models,
        total_estimated_cost_usd: total_cost,
    }))
}

/// GET /api/v1/metering/by-session — usage grouped by session
async fn by_session(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SessionQuery>,
) -> Result<Json<Vec<SessionSummary>>, StatusCode> {
    let storage = state
        .metering_storage()
        .await
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let (session_label, summaries) = if let Some(ref session_id) = query.session_id {
        let s = storage
            .summary_by_session(session_id.clone())
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        (session_id.clone(), s)
    } else {
        let s = storage
            .summary_by_model()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        ("all".to_string(), s)
    };

    let models: Vec<ModelSummary> = summaries
        .iter()
        .map(|s| {
            let cost =
                octo_engine::metering::storage::MeteringStorage::estimate_cost(s);
            ModelSummary {
                model: s.model.clone(),
                input_tokens: s.total_input_tokens,
                output_tokens: s.total_output_tokens,
                total_tokens: s.total_input_tokens + s.total_output_tokens,
                requests: s.total_requests,
                errors: s.total_errors,
                duration_ms: s.total_duration_ms,
                estimated_cost_usd: cost,
            }
        })
        .collect();

    let total_cost: f64 = models.iter().map(|m| m.estimated_cost_usd).sum();

    Ok(Json(vec![SessionSummary {
        session_id: session_label,
        models,
        total_estimated_cost_usd: total_cost,
    }]))
}

/// POST /api/v1/metering/reset — reset real-time counters
async fn reset(State(state): State<Arc<AppState>>) -> StatusCode {
    state.agent_supervisor.metering_arc().reset();
    StatusCode::NO_CONTENT
}
