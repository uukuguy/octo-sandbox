use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

#[derive(Deserialize)]
pub struct AuditQuery {
    pub event_type: Option<String>,
    pub user_id: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Serialize)]
pub struct AuditResponse {
    pub logs: Vec<AuditRecordResponse>,
    pub total: i64,
}

#[derive(serde::Serialize)]
pub struct AuditRecordResponse {
    pub id: i64,
    pub timestamp: String,
    pub event_type: String,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub resource_id: Option<String>,
    pub action: String,
    pub result: String,
    pub metadata: Option<String>,
    pub ip_address: Option<String>,
}

impl From<octo_engine::audit::AuditRecord> for AuditRecordResponse {
    fn from(record: octo_engine::audit::AuditRecord) -> Self {
        Self {
            id: record.id,
            timestamp: record.timestamp,
            event_type: record.event_type,
            user_id: record.user_id,
            session_id: record.session_id,
            resource_id: record.resource_id,
            action: record.action,
            result: record.result,
            metadata: record.metadata,
            ip_address: record.ip_address,
        }
    }
}

pub async fn list_audit(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AuditQuery>,
) -> Json<AuditResponse> {
    let limit = query.limit.unwrap_or(50).min(100);
    let offset = query.offset.unwrap_or(0);

    // Get audit storage on-demand
    let Some(audit_storage) = state.audit_storage() else {
        tracing::error!("Failed to create audit storage");
        return Json(AuditResponse {
            logs: vec![],
            total: 0,
        });
    };

    // Get total count first
    let total = audit_storage
        .count(query.event_type.as_deref(), query.user_id.as_deref())
        .unwrap_or(0);

    let logs_result = audit_storage.query(
        query.event_type.as_deref(),
        query.user_id.as_deref(),
        limit,
        offset,
    );

    let logs: Vec<AuditRecordResponse> = logs_result
        .map(|records| records.into_iter().map(AuditRecordResponse::from).collect())
        .unwrap_or_default();

    Json(AuditResponse { logs, total })
}

// ── AO-T9: Audit Enhancement ─────────────────────────────────────────

/// Query params for audit export
#[derive(Deserialize)]
pub struct AuditExportQuery {
    pub since: Option<String>,
    pub until: Option<String>,
    #[serde(default = "default_export_limit")]
    pub limit: u32,
}

fn default_export_limit() -> u32 {
    10000
}

/// GET /audit/export — export audit records with date range filtering
pub async fn export_audit(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AuditExportQuery>,
) -> Json<Vec<AuditRecordResponse>> {
    let Some(audit_storage) = state.audit_storage() else {
        return Json(vec![]);
    };

    let limit = query.limit.min(50000);
    let records = audit_storage
        .export(query.since.as_deref(), query.until.as_deref(), limit)
        .unwrap_or_default();

    Json(records.into_iter().map(AuditRecordResponse::from).collect())
}

/// Query params for audit cleanup
#[derive(Deserialize)]
pub struct AuditDeleteQuery {
    pub before: String,
}

/// Response for audit cleanup
#[derive(Serialize)]
pub struct AuditDeleteResponse {
    pub deleted_count: usize,
}

/// DELETE /audit — clean up old audit records
pub async fn delete_audit(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AuditDeleteQuery>,
) -> Result<Json<AuditDeleteResponse>, StatusCode> {
    let Some(audit_storage) = state.audit_storage() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };

    let deleted_count = audit_storage
        .delete_before(&query.before)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(AuditDeleteResponse { deleted_count }))
}

/// GET /audit/stats — aggregate audit statistics
pub async fn audit_stats(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let Some(audit_storage) = state.audit_storage() else {
        return Json(serde_json::json!({
            "total": 0,
            "by_event_type": {},
            "by_result": {},
        }));
    };

    match audit_storage.stats() {
        Ok(stats) => Json(serde_json::to_value(stats).unwrap_or_default()),
        Err(_) => Json(serde_json::json!({
            "total": 0,
            "by_event_type": {},
            "by_result": {},
        })),
    }
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/audit", get(list_audit).delete(delete_audit))
        .route("/audit/export", get(export_audit))
        .route("/audit/stats", get(audit_stats))
}
