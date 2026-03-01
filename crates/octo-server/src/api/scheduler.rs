use std::sync::Arc;

use axum::{
    extract::{Extension, Path, Query, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use octo_engine::auth::UserContext;
use serde::{Deserialize, Serialize};

use crate::state::AppState;

pub fn create_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/tasks", get(list_tasks))
        .route("/tasks", post(create_task))
        .route("/tasks/:id", get(get_task))
        .route("/tasks/:id", put(update_task))
        .route("/tasks/:id", delete(delete_task))
        .route("/tasks/:id/run", post(run_task))
        .route("/tasks/:id/executions", get(list_executions))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskRequest {
    pub name: String,
    pub cron: String,
    pub agent_config: octo_engine::scheduler::AgentTaskConfig,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTaskRequest {
    pub name: Option<String>,
    pub cron: Option<String>,
    pub agent_config: Option<octo_engine::scheduler::AgentTaskConfig>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskResponse {
    pub id: String,
    pub user_id: Option<String>,
    pub name: String,
    pub cron: String,
    pub agent_config: octo_engine::scheduler::AgentTaskConfig,
    pub enabled: bool,
    pub last_run: Option<String>,
    pub next_run: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<octo_engine::scheduler::ScheduledTask> for TaskResponse {
    fn from(t: octo_engine::scheduler::ScheduledTask) -> Self {
        Self {
            id: t.id,
            user_id: t.user_id,
            name: t.name,
            cron: t.cron,
            agent_config: t.agent_config,
            enabled: t.enabled,
            last_run: t.last_run.map(|d| d.to_rfc3339()),
            next_run: t.next_run.map(|d| d.to_rfc3339()),
            created_at: t.created_at.to_rfc3339(),
            updated_at: t.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct TaskListResponse {
    pub tasks: Vec<TaskResponse>,
    pub total: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionResponse {
    pub id: String,
    pub task_id: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub status: String,
    pub result: Option<String>,
    pub error: Option<String>,
}

impl From<octo_engine::scheduler::TaskExecution> for ExecutionResponse {
    fn from(e: octo_engine::scheduler::TaskExecution) -> Self {
        Self {
            id: e.id,
            task_id: e.task_id,
            started_at: e.started_at.to_rfc3339(),
            finished_at: e.finished_at.map(|d| d.to_rfc3339()),
            status: serde_json::to_string(&e.status).unwrap_or_default().trim_matches('"').to_string(),
            result: e.result,
            error: e.error,
        }
    }
}

async fn list_tasks(
    State(state): State<Arc<AppState>>,
    Extension(user_ctx): Extension<UserContext>,
) -> Result<Json<TaskListResponse>, axum::http::StatusCode> {
    let user_id = user_ctx.user_id.as_deref();
    let tasks = state
        .scheduler
        .as_ref()
        .ok_or(axum::http::StatusCode::NOT_FOUND)?
        .list_tasks(user_id)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let total = tasks.len();
    Ok(Json(TaskListResponse {
        tasks: tasks.into_iter().map(|t| t.into()).collect(),
        total,
    }))
}

async fn create_task(
    State(state): State<Arc<AppState>>,
    Extension(user_ctx): Extension<UserContext>,
    Json(payload): Json<CreateTaskRequest>,
) -> Result<Json<TaskResponse>, axum::http::StatusCode> {
    let scheduler = state.scheduler.as_ref().ok_or(axum::http::StatusCode::NOT_FOUND)?;

    let task = scheduler
        .create_task(
            user_ctx.user_id.clone(),
            payload.name,
            payload.cron,
            payload.agent_config,
            payload.enabled.unwrap_or(true),
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to create task: {}", e);
            axum::http::StatusCode::BAD_REQUEST
        })?;

    Ok(Json(task.into()))
}

async fn get_task(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
    Extension(user_ctx): Extension<UserContext>,
) -> Result<Json<TaskResponse>, axum::http::StatusCode> {
    let scheduler = state.scheduler.as_ref().ok_or(axum::http::StatusCode::NOT_FOUND)?;

    let task = scheduler
        .get_task(&task_id)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(axum::http::StatusCode::NOT_FOUND)?;

    // Check ownership
    if let (Some(req_user), Some(task_user)) = (&user_ctx.user_id, &task.user_id) {
        if req_user != task_user {
            return Err(axum::http::StatusCode::NOT_FOUND);
        }
    }

    Ok(Json(task.into()))
}

async fn update_task(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
    Extension(user_ctx): Extension<UserContext>,
    Json(payload): Json<UpdateTaskRequest>,
) -> Result<Json<TaskResponse>, axum::http::StatusCode> {
    let scheduler = state.scheduler.as_ref().ok_or(axum::http::StatusCode::NOT_FOUND)?;

    let task = scheduler
        .get_task(&task_id)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(axum::http::StatusCode::NOT_FOUND)?;

    // Check ownership
    if let (Some(req_user), Some(task_user)) = (&user_ctx.user_id, &task.user_id) {
        if req_user != task_user {
            return Err(axum::http::StatusCode::NOT_FOUND);
        }
    }

    let updated = scheduler
        .update_task(
            &task_id,
            payload.name,
            payload.cron,
            payload.agent_config,
            payload.enabled,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to update task: {}", e);
            axum::http::StatusCode::BAD_REQUEST
        })?;

    Ok(Json(updated.into()))
}

async fn delete_task(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
    Extension(user_ctx): Extension<UserContext>,
) -> Result<axum::http::StatusCode, axum::http::StatusCode> {
    let scheduler = state.scheduler.as_ref().ok_or(axum::http::StatusCode::NOT_FOUND)?;

    let task = scheduler
        .get_task(&task_id)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(axum::http::StatusCode::NOT_FOUND)?;

    // Check ownership
    if let (Some(req_user), Some(task_user)) = (&user_ctx.user_id, &task.user_id) {
        if req_user != task_user {
            return Err(axum::http::StatusCode::NOT_FOUND);
        }
    }

    scheduler
        .delete_task(&task_id)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

async fn run_task(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
    Extension(user_ctx): Extension<UserContext>,
) -> Result<Json<ExecutionResponse>, axum::http::StatusCode> {
    let scheduler = state.scheduler.as_ref().ok_or(axum::http::StatusCode::NOT_FOUND)?;

    let execution = scheduler
        .run_now(&task_id, user_ctx.user_id.as_deref())
        .await
        .map_err(|e| {
            tracing::error!("Failed to run task: {}", e);
            axum::http::StatusCode::BAD_REQUEST
        })?;

    Ok(Json(execution.into()))
}

async fn list_executions(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
    Query(params): Query<ExecutionQuery>,
    Extension(user_ctx): Extension<UserContext>,
) -> Result<Json<Vec<ExecutionResponse>>, axum::http::StatusCode> {
    let scheduler = state.scheduler.as_ref().ok_or(axum::http::StatusCode::NOT_FOUND)?;

    let task = scheduler
        .get_task(&task_id)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(axum::http::StatusCode::NOT_FOUND)?;

    // Check ownership
    if let (Some(req_user), Some(task_user)) = (&user_ctx.user_id, &task.user_id) {
        if req_user != task_user {
            return Err(axum::http::StatusCode::NOT_FOUND);
        }
    }

    let executions = scheduler
        .get_executions(&task_id, params.limit.unwrap_or(10))
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(
        executions
            .into_iter()
            .map(|e| e.into())
            .collect(),
    ))
}

#[derive(Debug, Deserialize)]
pub struct ExecutionQuery {
    pub limit: Option<usize>,
}
