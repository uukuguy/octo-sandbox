//! Agent registry REST API
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
use octo_engine::{AgentEntry, AgentId, AgentManifest};

use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/agents", get(list_agents).post(create_agent))
        .route("/agents/:id", get(get_agent).delete(delete_agent))
        .route("/agents/:id/start", post(start_agent))
        .route("/agents/:id/stop", post(stop_agent))
        .route("/agents/:id/pause", post(pause_agent))
        .route("/agents/:id/resume", post(resume_agent))
}

async fn list_agents(State(s): State<Arc<AppState>>) -> Json<Vec<AgentEntry>> {
    Json(s.agent_runner.registry.list_all())
}

async fn create_agent(
    State(s): State<Arc<AppState>>,
    Json(manifest): Json<AgentManifest>,
) -> (StatusCode, Json<AgentEntry>) {
    let id = s.agent_runner.registry.register(manifest);
    let entry = s.agent_runner.registry.get(&id).unwrap();
    (StatusCode::CREATED, Json(entry))
}

async fn get_agent(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AgentEntry>, StatusCode> {
    s.agent_runner
        .registry
        .get(&AgentId(id))
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn start_agent(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AgentEntry>, StatusCode> {
    let agent_id = AgentId(id);
    s.agent_runner
        .start(&agent_id)
        .await
        .map_err(|_| StatusCode::CONFLICT)?;
    s.agent_runner
        .registry
        .get(&agent_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn stop_agent(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AgentEntry>, StatusCode> {
    let agent_id = AgentId(id);
    s.agent_runner
        .stop(&agent_id)
        .await
        .map_err(|_| StatusCode::CONFLICT)?;
    s.agent_runner
        .registry
        .get(&agent_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn pause_agent(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AgentEntry>, StatusCode> {
    let agent_id = AgentId(id);
    s.agent_runner
        .pause(&agent_id)
        .await
        .map_err(|_| StatusCode::CONFLICT)?;
    s.agent_runner
        .registry
        .get(&agent_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn resume_agent(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AgentEntry>, StatusCode> {
    let agent_id = AgentId(id);
    s.agent_runner
        .resume(&agent_id)
        .await
        .map_err(|_| StatusCode::CONFLICT)?;
    s.agent_runner
        .registry
        .get(&agent_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn delete_agent(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> StatusCode {
    if s.agent_runner.registry.unregister(&AgentId(id)).is_some() {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}
