use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use octo_types::{MemoryCategory, MemoryFilter, MemoryId, SandboxId, SearchOptions, UserId};

use crate::state::AppState;

#[derive(Deserialize)]
pub struct MemorySearchParams {
    pub q: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    20
}

pub async fn search_memories(
    State(state): State<Arc<AppState>>,
    Query(params): Query<MemorySearchParams>,
) -> Json<serde_json::Value> {
    let query = params.q.unwrap_or_default();
    if query.is_empty() {
        return Json(serde_json::json!([]));
    }

    let opts = SearchOptions {
        user_id: "default".to_string(),
        limit: params.limit.min(100),
        ..Default::default()
    };

    match state.memory_store.search(&query, opts).await {
        Ok(entries) => Json(serde_json::to_value(entries).unwrap_or_default()),
        Err(_) => Json(serde_json::json!([])),
    }
}

#[derive(Deserialize)]
pub struct WorkingMemoryParams {
    pub sandbox_id: Option<String>,
}

pub async fn get_working_memory(
    State(state): State<Arc<AppState>>,
    Query(params): Query<WorkingMemoryParams>,
) -> Json<serde_json::Value> {
    let user_id = UserId::from_string("default");
    let sandbox_id = SandboxId::from_string(
        params.sandbox_id.as_deref().unwrap_or("default"),
    );
    match state.memory.get_blocks(&user_id, &sandbox_id).await {
        Ok(blocks) => Json(serde_json::to_value(blocks).unwrap_or_default()),
        Err(_) => Json(serde_json::json!([])),
    }
}

pub async fn get_memory(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let mem_id = MemoryId::from_string(&id);
    match state.memory_store.get(&mem_id).await {
        Ok(Some(entry)) => Json(serde_json::to_value(entry).unwrap_or_default()),
        Ok(None) => Json(serde_json::json!({"error": "not found"})),
        Err(e) => Json(serde_json::json!({"error": e.to_string()})),
    }
}

pub async fn delete_memory(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let mem_id = MemoryId::from_string(&id);
    match state.memory_store.delete(&mem_id).await {
        Ok(()) => Json(serde_json::json!({"deleted": id})),
        Err(e) => Json(serde_json::json!({"error": e.to_string()})),
    }
}

#[derive(Deserialize)]
pub struct DeleteFilterParams {
    pub category: Option<String>,
    pub sandbox_id: Option<String>,
}

pub async fn delete_memories_by_filter(
    State(state): State<Arc<AppState>>,
    Query(params): Query<DeleteFilterParams>,
) -> Json<serde_json::Value> {
    let categories = params
        .category
        .as_deref()
        .and_then(MemoryCategory::from_str)
        .map(|c| vec![c]);

    let filter = MemoryFilter {
        user_id: "default".to_string(),
        sandbox_id: params.sandbox_id,
        categories,
        ..Default::default()
    };

    match state.memory_store.delete_by_filter(filter).await {
        Ok(count) => Json(serde_json::json!({"deleted": count})),
        Err(e) => Json(serde_json::json!({"error": e.to_string()})),
    }
}
