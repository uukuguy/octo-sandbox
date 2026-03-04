//! Background Tasks API
//!
//! POST   /api/tasks              submit a background task
//! GET    /api/tasks              list all background tasks
//! GET    /api/tasks/:id          get task status and result
//! DELETE /api/tasks/:id         cancel a running task

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Utc};
use octo_types::{ChatMessage, ContentBlock, MessageRole, ToolContext, UserId};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::state::AppState;

/// Task status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Running,
    Success,
    Failed,
    Cancelled,
}

/// Background task
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundTask {
    pub id: String,
    pub status: TaskStatus,
    pub prompt: String,
    pub model: Option<String>,
    pub result: Option<String>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

impl BackgroundTask {
    pub fn new(prompt: String, model: Option<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            status: TaskStatus::Pending,
            prompt,
            model,
            result: None,
            error: None,
            created_at: Utc::now(),
            finished_at: None,
        }
    }
}

/// In-memory task store
pub struct TaskStore {
    tasks: RwLock<HashMap<String, BackgroundTask>>,
    /// Cancel tokens for running tasks
    cancel_tokens: RwLock<HashMap<String, broadcast::Sender<()>>>,
}

impl TaskStore {
    pub fn new() -> Self {
        Self {
            tasks: RwLock::new(HashMap::new()),
            cancel_tokens: RwLock::new(HashMap::new()),
        }
    }

    pub async fn create_task(&self, task: BackgroundTask) -> BackgroundTask {
        let mut tasks = self.tasks.write().await;
        tasks.insert(task.id.clone(), task.clone());
        task
    }

    pub async fn get_task(&self, id: &str) -> Option<BackgroundTask> {
        let tasks = self.tasks.read().await;
        tasks.get(id).cloned()
    }

    pub async fn list_tasks(&self) -> Vec<BackgroundTask> {
        let tasks = self.tasks.read().await;
        let mut list: Vec<BackgroundTask> = tasks.values().cloned().collect();
        // Sort by created_at descending (most recent first)
        list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        list
    }

    pub async fn update_task(&self, task: BackgroundTask) -> Option<BackgroundTask> {
        let mut tasks = self.tasks.write().await;
        tasks.insert(task.id.clone(), task.clone());
        Some(task)
    }

    pub async fn register_cancel_token(&self, task_id: &str, sender: broadcast::Sender<()>) {
        let mut tokens = self.cancel_tokens.write().await;
        tokens.insert(task_id.to_string(), sender);
    }

    pub async fn cancel_task(&self, id: &str) -> bool {
        // Mark task as cancelled
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.get_mut(id) {
            if task.status == TaskStatus::Running {
                task.status = TaskStatus::Cancelled;
                task.finished_at = Some(Utc::now());
                // Send cancel signal
                let tokens = self.cancel_tokens.read().await;
                if let Some(sender) = tokens.get(id) {
                    let _ = sender.send(());
                }
                return true;
            }
        }
        false
    }
}

impl Default for TaskStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Request to create a background task
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskRequest {
    pub prompt: String,
    pub model: Option<String>,
    /// Optional max rounds (default: 50)
    #[serde(default = "default_max_rounds")]
    pub max_rounds: u32,
    /// Optional timeout in seconds (default: 300)
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

fn default_max_rounds() -> u32 {
    50
}

fn default_timeout_secs() -> u64 {
    300
}

/// Response for a background task
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskResponse {
    pub id: String,
    pub status: TaskStatus,
    pub result: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub finished_at: Option<String>,
}

impl From<BackgroundTask> for TaskResponse {
    fn from(t: BackgroundTask) -> Self {
        Self {
            id: t.id,
            status: t.status,
            result: t.result,
            error: t.error,
            created_at: t.created_at.to_rfc3339(),
            finished_at: t.finished_at.map(|d| d.to_rfc3339()),
        }
    }
}

/// Execute a background task using the agent runtime
/// This runs the agent loop with the given prompt and returns the result
async fn execute_background_task(
    agent_runtime: Arc<octo_engine::AgentRuntime>,
    task_id: String,
    prompt: String,
    model: String,
    _max_rounds: u32,
    _timeout_secs: u64,
    task_store: Arc<TaskStore>,
    mut cancel_rx: broadcast::Receiver<()>,
) {
    // Create session for the task
    let user_id = UserId::from_string("api-task");
    let session = agent_runtime.session_store().create_session_with_user(&user_id).await;
    let session_id = session.session_id.clone();
    let sandbox_id = session.sandbox_id.clone();

    // Prepare initial message
    let user_message = ChatMessage::user(prompt.clone());
    let mut messages = vec![user_message];

    // Create tool context
    let tool_ctx = ToolContext {
        sandbox_id: sandbox_id.clone(),
        working_dir: PathBuf::from("/tmp"),
    };

    // Create event channel
    let (tx, _) = broadcast::channel::<octo_engine::agent::AgentEvent>(100);

    // Build tool registry snapshot
    let tools_snapshot = {
        let tools_guard = agent_runtime.tools().lock().unwrap();
        let mut registry = octo_engine::tools::ToolRegistry::new();
        for (name, tool) in tools_guard.iter() {
            registry.register_arc(name.clone(), tool);
        }
        Arc::new(registry)
    };

    // Create and configure agent loop
    let mut agent_loop = octo_engine::agent::AgentLoop::new(
        agent_runtime.provider().clone(),
        tools_snapshot,
        agent_runtime.memory().clone(),
    )
    .with_model(model);

    // Run with timeout and cancellation
    let result = tokio::select! {
        result = agent_loop.run(
            &session_id,
            &user_id,
            &sandbox_id,
            &mut messages,
            tx,
            tool_ctx,
            None,
        ) => {
            result.map(|_| {
                messages
                    .iter()
                    .rev()
                    .find(|m| m.role == MessageRole::Assistant)
                    .and_then(|m| {
                        m.content.iter().find_map(|c| {
                            if let ContentBlock::Text { text } = c {
                                Some(text.clone())
                            } else {
                                None
                            }
                        })
                    })
                    .unwrap_or_else(|| "Task completed".to_string())
            })
        }
        _ = cancel_rx.recv() => {
            // Task was cancelled
            let mut tasks = task_store.tasks.write().await;
            if let Some(t) = tasks.get_mut(&task_id) {
                t.status = TaskStatus::Cancelled;
                t.finished_at = Some(Utc::now());
            }
            return;
        }
    };

    // Update task result
    let mut tasks = task_store.tasks.write().await;
    if let Some(t) = tasks.get_mut(&task_id) {
        match result {
            Ok(response) => {
                t.status = TaskStatus::Success;
                t.result = Some(response);
            }
            Err(e) => {
                t.status = TaskStatus::Failed;
                t.error = Some(e.to_string());
            }
        }
        t.finished_at = Some(Utc::now());
    }
}

/// POST /api/tasks - Submit a new background task
pub async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<(StatusCode, Json<TaskResponse>), StatusCode> {
    let task_store = state
        .task_store
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    // Create initial task
    let mut task = BackgroundTask::new(req.prompt.clone(), req.model.clone());
    task.status = TaskStatus::Running;

    let task_id = task.id.clone();
    let task_store = Arc::clone(task_store);

    // Create the task in store
    let task = task_store.create_task(task).await;

    // Create cancel token for this task
    let (cancel_tx, cancel_rx) = broadcast::channel::<()>(1);
    task_store
        .register_cancel_token(&task_id, cancel_tx)
        .await;

    // Get the model to use
    let model = req
        .model
        .clone()
        .unwrap_or_else(|| "claude-3-5-sonnet-20241022".to_string());

    // Clone all data needed for the spawned task
    let prompt = req.prompt.clone();
    let max_rounds = req.max_rounds;
    let timeout_secs = req.timeout_secs;
    let agent_runtime = Arc::clone(&state.agent_supervisor);

    // Spawn background task execution
    tokio::spawn(async move {
        execute_background_task(
            agent_runtime,
            task_id,
            prompt,
            model,
            max_rounds,
            timeout_secs,
            task_store,
            cancel_rx,
        )
        .await;
    });

    tracing::info!(task_id = %task.id, "Background task submitted");

    Ok((StatusCode::CREATED, Json(task.into())))
}

/// GET /api/tasks - List all background tasks
pub async fn list_tasks(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<TaskResponse>>, StatusCode> {
    let task_store = state
        .task_store
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let tasks = task_store.list_tasks().await;
    let responses: Vec<TaskResponse> = tasks.into_iter().map(|t| t.into()).collect();

    Ok(Json(responses))
}

/// GET /api/tasks/:id - Get task status and result
pub async fn get_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<TaskResponse>, StatusCode> {
    let task_store = state
        .task_store
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let task = task_store
        .get_task(&id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(task.into()))
}

/// DELETE /api/tasks/:id - Cancel a running task
pub async fn cancel_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let task_store = state
        .task_store
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let cancelled = task_store.cancel_task(&id).await;

    if cancelled {
        tracing::info!(task_id = %id, "Background task cancelled");
        Ok(StatusCode::NO_CONTENT)
    } else {
        // Task not found or not in cancellable state
        Err(StatusCode::NOT_FOUND)
    }
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/tasks", post(create_task).get(list_tasks))
        .route("/tasks/{id}", get(get_task).delete(cancel_task))
}
