use std::sync::Arc;

use axum::{extract::State, routing::get, Json, Router};
use serde::Serialize;

use crate::state::AppState;

/// Context budget snapshot response (AO-T10).
#[derive(Serialize)]
pub struct ContextSnapshotResponse {
    pub total_budget: usize,
    pub system_tokens: usize,
    pub message_tokens: usize,
    pub tool_tokens: usize,
    pub remaining: usize,
    pub usage_pct: f32,
    pub needs_pruning: bool,
    pub degradation_level: String,
}

/// Zone information in context breakdown.
#[derive(Serialize)]
pub struct ZoneInfo {
    pub name: String,
    pub tokens: usize,
    pub description: String,
}

/// Context zones breakdown response.
#[derive(Serialize)]
pub struct ContextZonesResponse {
    pub zone_a: ZoneInfo,
    pub zone_b: ZoneInfo,
    pub zone_c: ZoneInfo,
    pub zone_d: ZoneInfo,
}

fn degradation_label(pct: f32) -> &'static str {
    if pct < 0.60 {
        "none"
    } else if pct < 0.70 {
        "soft_trim"
    } else if pct < 0.90 {
        "auto_compaction"
    } else if pct < 0.95 {
        "overflow_compaction"
    } else if pct < 0.99 {
        "tool_result_truncation"
    } else {
        "final_error"
    }
}

/// GET /context/snapshot — current context budget snapshot
pub async fn context_snapshot(
    State(_state): State<Arc<AppState>>,
) -> Json<ContextSnapshotResponse> {
    // Build a lightweight ContextManager with default 200k context window
    let cm = octo_engine::context::ContextManager::with_default_counter(200_000);

    // Snapshot with empty messages — shows baseline budget allocation.
    // Per-session snapshots would require session_id parameter (future enhancement).
    let snapshot = cm.budget_snapshot("", &[]);
    let needs_pruning = cm.needs_pruning(&snapshot);

    Json(ContextSnapshotResponse {
        total_budget: snapshot.total_budget,
        system_tokens: snapshot.system_tokens,
        message_tokens: snapshot.message_tokens,
        tool_tokens: snapshot.tool_tokens,
        remaining: snapshot.remaining,
        usage_pct: snapshot.usage_pct,
        needs_pruning,
        degradation_level: degradation_label(snapshot.usage_pct).to_string(),
    })
}

/// GET /context/zones — zone breakdown
pub async fn context_zones(
    State(_state): State<Arc<AppState>>,
) -> Json<ContextZonesResponse> {
    let cm = octo_engine::context::ContextManager::with_default_counter(200_000);
    let snapshot = cm.budget_snapshot("", &[]);

    Json(ContextZonesResponse {
        zone_a: ZoneInfo {
            name: "System Prompt".to_string(),
            tokens: snapshot.system_tokens,
            description: "Static system prompt (cacheable)".to_string(),
        },
        zone_b: ZoneInfo {
            name: "Dynamic Context".to_string(),
            tokens: 0, // Dynamic context not measurable without active request
            description: "Date, MCP status, session state, user context".to_string(),
        },
        zone_c: ZoneInfo {
            name: "Conversation History".to_string(),
            tokens: snapshot.message_tokens,
            description: "Active conversation messages".to_string(),
        },
        zone_d: ZoneInfo {
            name: "Tool Definitions".to_string(),
            tokens: snapshot.tool_tokens,
            description: "Tool schemas and specifications (reserved)".to_string(),
        },
    })
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/context/snapshot", get(context_snapshot))
        .route("/context/zones", get(context_zones))
}
