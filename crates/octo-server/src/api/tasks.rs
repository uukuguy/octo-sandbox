//! Background Tasks API

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::state::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Running,
    Success,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskResponse {
    pub id: String,
    pub status: TaskStatus,
    pub result: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskExecutionResponse {
    pub id: String,
    pub task_id: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub status: String,
    pub result: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDetailResponse {
    pub task: TaskResponse,
    pub executions: Vec<TaskExecutionResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskRequest {
    pub prompt: String,
    pub model: Option<String>,
    #[serde(default)]
    pub max_rounds: u32,
    #[serde(default)]
    pub timeout_secs: u64,
}

async fn submit_task(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<Json<TaskResponse>, StatusCode> {
    // Input validation with limits
    if req.prompt.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    if req.prompt.len() > 100_000 {
        return Err(StatusCode::BAD_REQUEST);
    }
    if req.max_rounds > 100 {
        return Err(StatusCode::BAD_REQUEST);
    }
    // timeout_secs already has minimum of 60 enforced below

    // Get scheduler
    let scheduler = state
        .scheduler
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    // Get model from config or use default
    let model = req
        .model
        .or_else(|| state.config.provider.model.clone())
        .unwrap_or_else(|| "claude-sonnet-4-20250514".to_string());

    let config = octo_engine::scheduler::AgentTaskConfig {
        system_prompt: String::new(),
        input: req.prompt,
        max_rounds: req.max_rounds.max(1),
        timeout_secs: req.timeout_secs.max(60),
        model,
    };

    // Create scheduled task via scheduler
    let scheduled = match scheduler
        .create_task(
            Some("api-task".to_string()),
            format!("ad-hoc-{}", Uuid::new_v4().to_string()[..8].to_string()),
            "0 0 1 1 2099".to_string(), // Far future - won't auto-run
            config,
            true,
        )
        .await
    {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("create task error: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Spawn background task to execute the agent (non-blocking)
    let task_id = scheduled.id.clone();
    let scheduler_clone = state.scheduler.clone();
    tokio::spawn(async move {
        if let Some(scheduler) = scheduler_clone.as_ref() {
            let result = scheduler.run_now(&task_id, Some("api-task")).await;
            if let Err(e) = result {
                tracing::error!("background task {} execution error: {}", task_id, e);
            }
        }
    });

    // Return immediately with pending status
    Ok(Json(TaskResponse {
        id: scheduled.id,
        status: TaskStatus::Pending,
        result: None,
        error: None,
    }))
}

/// Map ExecutionStatus to TaskStatus
fn map_execution_status(status: &octo_engine::scheduler::ExecutionStatus) -> TaskStatus {
    match status {
        octo_engine::scheduler::ExecutionStatus::Running => TaskStatus::Running,
        octo_engine::scheduler::ExecutionStatus::Success => TaskStatus::Success,
        octo_engine::scheduler::ExecutionStatus::Failed
        | octo_engine::scheduler::ExecutionStatus::Timeout
        | octo_engine::scheduler::ExecutionStatus::Cancelled => TaskStatus::Failed,
    }
}

/// Convert ScheduledTask to TaskResponse
fn scheduled_task_to_response(
    task: &octo_engine::scheduler::ScheduledTask,
    execution: Option<&octo_engine::scheduler::TaskExecution>,
) -> TaskResponse {
    let status = if let Some(exec) = execution {
        map_execution_status(&exec.status)
    } else {
        // No executions yet - task is pending
        TaskStatus::Pending
    };

    TaskResponse {
        id: task.id.clone(),
        status,
        result: execution.and_then(|e| e.result.clone()),
        error: execution.and_then(|e| e.error.clone()),
    }
}

async fn list_tasks(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<TaskResponse>>, StatusCode> {
    let scheduler = state
        .scheduler
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let tasks = scheduler.list_tasks(None).await.map_err(|e| {
        tracing::error!("list_tasks error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut responses = Vec::with_capacity(tasks.len());

    for task in tasks {
        // Get the latest execution for each task
        let executions = scheduler.get_executions(&task.id, 1).await.map_err(|e| {
            tracing::error!("get_executions error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        let latest_execution = executions.first();
        responses.push(scheduled_task_to_response(&task, latest_execution));
    }

    Ok(Json(responses))
}

async fn get_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<TaskDetailResponse>, StatusCode> {
    let scheduler = state
        .scheduler
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let task = scheduler.get_task(&id).await.map_err(|e| {
        tracing::error!("get_task error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let task = match task {
        Some(t) => t,
        None => return Err(StatusCode::NOT_FOUND),
    };

    let executions = scheduler.get_executions(&id, 10).await.map_err(|e| {
        tracing::error!("get_executions error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let latest_execution = executions.first();

    let task_response = scheduled_task_to_response(&task, latest_execution);

    let execution_responses: Vec<TaskExecutionResponse> = executions
        .iter()
        .map(|e| TaskExecutionResponse {
            id: e.id.clone(),
            task_id: e.task_id.clone(),
            started_at: e.started_at.to_rfc3339(),
            finished_at: e.finished_at.map(|dt| dt.to_rfc3339()),
            status: format!("{:?}", e.status),
            result: e.result.clone(),
            error: e.error.clone(),
        })
        .collect();

    Ok(Json(TaskDetailResponse {
        task: task_response,
        executions: execution_responses,
    }))
}

async fn cancel_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let scheduler = state
        .scheduler
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    // Check if task exists first
    let task = scheduler.get_task(&id).await.map_err(|e| {
        tracing::error!("get_task error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if task.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }

    // Delete the task
    scheduler.delete_task(&id).await.map_err(|e| {
        tracing::error!("delete_task error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/tasks", post(submit_task).get(list_tasks))
        .route("/tasks/{id}", get(get_task).delete(cancel_task))
}
