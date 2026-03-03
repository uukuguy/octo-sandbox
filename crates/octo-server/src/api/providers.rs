use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{delete, get, post},
    Json, Router,
};
use octo_engine::providers::LlmInstance;
use serde::{Deserialize, Serialize};

use crate::state::AppState;

/// List response
#[derive(Serialize)]
pub struct ListProvidersResponse {
    pub policy: String,
    pub current_instance_id: Option<String>,
    pub instances: Vec<ProviderInstance>,
}

#[derive(Serialize)]
pub struct ProviderInstance {
    pub id: String,
    pub provider: String,
    pub model: String,
    pub priority: u8,
    pub health: String,
    pub enabled: bool,
}

/// Add instance request
#[derive(Deserialize)]
pub struct AddProviderRequest {
    pub id: String,
    pub provider: String,
    pub api_key: String,
    pub base_url: Option<String>,
    pub model: String,
    pub priority: u8,
    pub max_rpm: Option<u32>,
    pub enabled: Option<bool>,
}

/// List all instances
pub async fn list_providers(State(state): State<Arc<AppState>>) -> Json<ListProvidersResponse> {
    let chain = state.provider_chain.as_ref();

    let policy = match chain {
        Some(c) => format!("{:?}", c.policy()),
        None => "none".to_string(),
    };

    let instances = match chain {
        Some(c) => {
            let instance_list = c.list_instances().await;
            let mut result = Vec::with_capacity(instance_list.len());
            for i in instance_list {
                let health = c.get_health(&i.id).await;
                result.push(ProviderInstance {
                    id: i.id,
                    provider: i.provider,
                    model: i.model,
                    priority: i.priority,
                    health: format!("{:?}", health),
                    enabled: i.enabled,
                });
            }
            result
        }
        None => vec![],
    };

    let current = match chain {
        Some(c) => c.get_current_selection().await,
        None => None,
    };

    Json(ListProvidersResponse {
        policy,
        current_instance_id: current,
        instances,
    })
}

/// Manually select an instance
pub async fn select_provider(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<()>, String> {
    let chain = state.provider_chain.as_ref();

    match chain {
        Some(c) => c.select_instance(&id).await.map_err(|e| e.to_string())?,
        None => return Err("Provider chain not configured".to_string()),
    };

    Ok(Json(()))
}

/// Reset instance health status
pub async fn reset_provider(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<()>, String> {
    let chain = state.provider_chain.as_ref();

    match chain {
        Some(c) => c.reset_health(&id).await.map_err(|e| e.to_string())?,
        None => return Err("Provider chain not configured".to_string()),
    };

    Ok(Json(()))
}

/// Clear selection
pub async fn clear_selection(State(state): State<Arc<AppState>>) -> Result<Json<()>, String> {
    let chain = state.provider_chain.as_ref();

    match chain {
        Some(c) => c.clear_selection().await,
        None => return Err("Provider chain not configured".to_string()),
    };

    Ok(Json(()))
}

/// Add an instance
pub async fn add_provider(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AddProviderRequest>,
) -> Result<Json<()>, String> {
    let chain = state.provider_chain.as_ref();

    let instance = LlmInstance {
        id: req.id,
        provider: req.provider,
        api_key: req.api_key,
        base_url: req.base_url,
        model: req.model,
        priority: req.priority,
        max_rpm: req.max_rpm,
        enabled: req.enabled.unwrap_or(true),
    };

    match chain {
        Some(c) => c.add_instance(instance).await,
        None => return Err("Provider chain not configured".to_string()),
    };

    Ok(Json(()))
}

/// Delete an instance
pub async fn delete_provider(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<()>, String> {
    let chain = state.provider_chain.as_ref();

    match chain {
        Some(c) => c.remove_instance(&id).await.map_err(|e| e.to_string())?,
        None => return Err("Provider chain not configured".to_string()),
    };

    Ok(Json(()))
}

/// Register routes
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/providers", get(list_providers))
        .route("/providers", post(add_provider))
        .route("/providers/{id}", delete(delete_provider))
        .route("/providers/{id}/select", post(select_provider))
        .route("/providers/{id}/reset", post(reset_provider))
        .route("/providers/selection", delete(clear_selection))
}
