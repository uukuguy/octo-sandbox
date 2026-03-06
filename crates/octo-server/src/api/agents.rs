//! Agent catalog REST API
//!
//! POST   /api/v1/agents              register new agent
//! GET    /api/v1/agents              list all agents
//! GET    /api/v1/agents/:id          get agent by id
//! POST   /api/v1/agents/:id/start    start agent
//! POST   /api/v1/agents/:id/stop     stop agent
//! POST   /api/v1/agents/:id/pause    pause agent
//! POST   /api/v1/agents/:id/resume   resume agent
//! DELETE /api/v1/agents/:id          unregister agent

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use octo_engine::{AgentEntry, AgentError, AgentId, AgentManifest};

use crate::state::AppState;

fn agent_err_to_status(e: AgentError) -> StatusCode {
    match e {
        AgentError::NotFound(_) => StatusCode::NOT_FOUND,
        AgentError::InvalidTransition { .. } => StatusCode::CONFLICT,
        AgentError::ScheduledTask(_) => StatusCode::INTERNAL_SERVER_ERROR,
        AgentError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        AgentError::McpNotInitialized => StatusCode::SERVICE_UNAVAILABLE,
        AgentError::McpError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        AgentError::McpServerNotFound(_) => StatusCode::NOT_FOUND,
        AgentError::PermissionDenied(_) => StatusCode::FORBIDDEN,
    }
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/agents", get(list_agents).post(create_agent))
        .route("/agents/{id}", get(get_agent).delete(delete_agent))
        .route("/agents/{id}/start", post(start_agent))
        .route("/agents/{id}/stop", post(stop_agent))
        .route("/agents/{id}/pause", post(pause_agent))
        .route("/agents/{id}/resume", post(resume_agent))
}

async fn list_agents(State(s): State<Arc<AppState>>) -> Json<Vec<AgentEntry>> {
    Json(s.agent_supervisor.catalog().list_all())
}

async fn create_agent(
    State(s): State<Arc<AppState>>,
    Json(manifest): Json<AgentManifest>,
) -> Result<(StatusCode, Json<AgentEntry>), StatusCode> {
    // Get tenant_id from runtime's tenant context (single-user workbench)
    let tenant_id = s
        .agent_supervisor
        .tenant_context()
        .map(|ctx| ctx.tenant_id.clone());
    let id = s.agent_supervisor.catalog().register(manifest, tenant_id);
    let entry = s
        .agent_supervisor
        .catalog()
        .get(&id)
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(entry)))
}

async fn get_agent(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AgentEntry>, StatusCode> {
    s.agent_supervisor
        .catalog()
        .get(&AgentId(id))
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn start_agent(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AgentEntry>, StatusCode> {
    use octo_types::{SandboxId, SessionId, UserId};
    let agent_id = AgentId(id);
    let session_id = SessionId::new();
    let user_id = UserId::from_string("api");
    let sandbox_id = SandboxId::from_string("default");
    s.agent_supervisor
        .start(&agent_id, session_id, user_id, sandbox_id, vec![])
        .await
        .map_err(agent_err_to_status)?;
    s.agent_supervisor
        .catalog()
        .get(&agent_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn stop_agent(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AgentEntry>, StatusCode> {
    let agent_id = AgentId(id);
    s.agent_supervisor
        .stop(&agent_id)
        .await
        .map_err(agent_err_to_status)?;
    s.agent_supervisor
        .catalog()
        .get(&agent_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn pause_agent(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AgentEntry>, StatusCode> {
    let agent_id = AgentId(id);
    s.agent_supervisor
        .pause(&agent_id)
        .await
        .map_err(agent_err_to_status)?;
    s.agent_supervisor
        .catalog()
        .get(&agent_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn resume_agent(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AgentEntry>, StatusCode> {
    let agent_id = AgentId(id);
    s.agent_supervisor
        .resume(&agent_id)
        .map_err(agent_err_to_status)?;
    s.agent_supervisor
        .catalog()
        .get(&agent_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn delete_agent(State(s): State<Arc<AppState>>, Path(id): Path<String>) -> StatusCode {
    if s.agent_supervisor
        .catalog()
        .unregister(&AgentId(id))
        .is_some()
    {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}
