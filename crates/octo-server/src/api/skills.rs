//! Skill catalog REST API
//!
//! GET    /api/v1/skills              list all loaded skills
//! GET    /api/v1/skills/:name        get skill details
//! POST   /api/v1/skills/:name/execute  trigger skill execution (placeholder)
//! DELETE /api/v1/skills/:name        unload a skill

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

#[derive(Serialize)]
struct SkillListResponse {
    skills: Vec<SkillInfo>,
    total: usize,
}

#[derive(Serialize)]
struct SkillInfo {
    name: String,
    description: String,
    version: Option<String>,
    user_invocable: bool,
    allowed_tools: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct ExecuteRequest {
    #[serde(default)]
    args: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct ExecuteResponse {
    status: String,
    message: String,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/skills", get(list_skills))
        .route(
            "/skills/{name}",
            get(get_skill).delete(delete_skill),
        )
        .route("/skills/{name}/execute", post(execute_skill))
}

async fn list_skills(State(state): State<Arc<AppState>>) -> Json<SkillListResponse> {
    let skills: Vec<SkillInfo> = state
        .agent_supervisor
        .skill_registry()
        .map(|reg| {
            reg.list_all()
                .into_iter()
                .map(|s| SkillInfo {
                    name: s.name,
                    description: s.description,
                    version: s.version,
                    user_invocable: s.user_invocable,
                    allowed_tools: s.allowed_tools,
                })
                .collect()
        })
        .unwrap_or_default();

    let total = skills.len();
    Json(SkillListResponse { skills, total })
}

async fn get_skill(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<SkillInfo>, StatusCode> {
    let registry = state
        .agent_supervisor
        .skill_registry()
        .ok_or(StatusCode::NOT_FOUND)?;

    let skill = registry.get(&name).ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(SkillInfo {
        name: skill.name,
        description: skill.description,
        version: skill.version,
        user_invocable: skill.user_invocable,
        allowed_tools: skill.allowed_tools,
    }))
}

async fn execute_skill(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(_body): Json<ExecuteRequest>,
) -> Result<Json<ExecuteResponse>, StatusCode> {
    // Verify the skill exists before accepting
    let registry = state
        .agent_supervisor
        .skill_registry()
        .ok_or(StatusCode::NOT_FOUND)?;

    if registry.get(&name).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }

    // Placeholder: actual execution will be wired to SkillRuntime later
    Ok(Json(ExecuteResponse {
        status: "accepted".to_string(),
        message: format!("Skill '{}' execution queued", name),
    }))
}

async fn delete_skill(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<ExecuteResponse>, StatusCode> {
    let registry = state
        .agent_supervisor
        .skill_registry()
        .ok_or(StatusCode::NOT_FOUND)?;

    if registry.remove(&name).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(Json(ExecuteResponse {
        status: "ok".to_string(),
        message: format!("Skill '{}' unloaded", name),
    }))
}
