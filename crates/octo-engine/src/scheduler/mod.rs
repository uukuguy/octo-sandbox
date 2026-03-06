//! Scheduler module for periodic task execution

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use cron::Schedule;
use std::path::PathBuf;

use octo_types::{ChatMessage, ContentBlock, MessageRole, PathValidator, ToolContext, UserId};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::{broadcast, Semaphore};
use uuid::Uuid;

use crate::agent::{AgentEvent, AgentLoop};
use crate::memory::WorkingMemory;
use crate::providers::Provider;
use crate::session::SessionStore;
use crate::tools::ToolRegistry;

/// Scheduled task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub id: String,
    pub user_id: Option<String>,
    pub name: String,
    pub cron: String,
    pub agent_config: AgentTaskConfig,
    pub enabled: bool,
    pub last_run: Option<DateTime<Utc>>,
    pub next_run: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Agent task configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTaskConfig {
    pub system_prompt: String,
    pub input: String,
    pub max_rounds: u32,
    pub timeout_secs: u64,
    /// Model to use for this task (e.g., "claude-3-5-sonnet-20241022")
    #[serde(default = "default_model")]
    pub model: String,
}

fn default_model() -> String {
    "claude-3-5-sonnet-20241022".to_string()
}

impl Default for AgentTaskConfig {
    fn default() -> Self {
        Self {
            system_prompt: String::new(),
            input: String::new(),
            max_rounds: 50,
            timeout_secs: 300,
            model: default_model(),
        }
    }
}

/// Task execution record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecution {
    pub id: String,
    pub task_id: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub status: ExecutionStatus,
    pub result: Option<String>,
    pub error: Option<String>,
}

/// Execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Running,
    Success,
    Failed,
    Timeout,
    Cancelled,
}

/// Scheduler errors
#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
    #[error("Task not found: {0}")]
    TaskNotFound(String),
    #[error("Invalid cron expression: {0}")]
    InvalidCron(String),
    #[error("Task already running: {0}")]
    TaskAlreadyRunning(String),
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Configuration error: {0}")]
    Config(String),
}

impl Serialize for SchedulerError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

/// Scheduler storage trait
#[async_trait]
pub trait SchedulerStorage: Send + Sync {
    async fn save_task(&self, task: &ScheduledTask) -> Result<(), SchedulerError>;
    async fn get_task(&self, task_id: &str) -> Result<Option<ScheduledTask>, SchedulerError>;
    async fn list_tasks(&self, user_id: Option<&str>)
        -> Result<Vec<ScheduledTask>, SchedulerError>;
    async fn delete_task(&self, task_id: &str) -> Result<(), SchedulerError>;
    async fn update_timing(
        &self,
        task_id: &str,
        last_run: Option<DateTime<Utc>>,
        next_run: Option<DateTime<Utc>>,
    ) -> Result<(), SchedulerError>;
    async fn save_execution(&self, execution: &TaskExecution) -> Result<(), SchedulerError>;
    async fn get_executions(
        &self,
        task_id: &str,
        limit: usize,
    ) -> Result<Vec<TaskExecution>, SchedulerError>;
    async fn get_due_tasks(&self) -> Result<Vec<ScheduledTask>, SchedulerError>;
}

pub mod storage;
pub use storage::SqliteSchedulerStorage;

/// Cron parser helper
pub struct CronParser;

impl CronParser {
    pub fn new() -> Self {
        Self
    }

    /// Parse cron expression and calculate next run time
    pub fn parse_next(
        &self,
        cron_expr: &str,
        from: DateTime<Utc>,
    ) -> Result<DateTime<Utc>, SchedulerError> {
        // Cron expression uses standard 5-field format: minute hour day month weekday
        let schedule = Schedule::from_str(cron_expr)
            .map_err(|e| SchedulerError::InvalidCron(e.to_string()))?;

        let next = schedule
            .after(&from)
            .next()
            .ok_or_else(|| SchedulerError::InvalidCron("No next occurrence found".to_string()))?;

        Ok(next.with_timezone(&Utc))
    }

    /// Validate cron expression
    pub fn validate(&self, cron_expr: &str) -> Result<(), SchedulerError> {
        Schedule::from_str(cron_expr).map_err(|e| SchedulerError::InvalidCron(e.to_string()))?;
        Ok(())
    }
}

impl Default for CronParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Scheduler configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    pub enabled: bool,
    pub check_interval_secs: u64,
    pub max_concurrent: usize,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            check_interval_secs: 60,
            max_concurrent: 5,
        }
    }
}

/// Scheduler core
pub struct Scheduler {
    config: SchedulerConfig,
    storage: Arc<dyn SchedulerStorage>,
    cron_parser: CronParser,
    running: Arc<AtomicBool>,
    semaphore: Arc<Semaphore>,
    // Agent execution dependencies
    provider: Arc<dyn Provider>,
    tools: Arc<StdMutex<ToolRegistry>>,
    memory: Arc<dyn WorkingMemory>,
    session_store: Arc<dyn SessionStore>,
    path_validator: Option<Arc<dyn PathValidator>>,
}

impl Scheduler {
    pub fn new(
        config: SchedulerConfig,
        storage: Arc<dyn SchedulerStorage>,
        provider: Arc<dyn Provider>,
        tools: Arc<StdMutex<ToolRegistry>>,
        memory: Arc<dyn WorkingMemory>,
        session_store: Arc<dyn SessionStore>,
        path_validator: Option<Arc<dyn PathValidator>>,
    ) -> Self {
        let max_concurrent = config.max_concurrent;
        Self {
            config,
            storage,
            cron_parser: CronParser::new(),
            running: Arc::new(AtomicBool::new(false)),
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            provider,
            tools,
            memory,
            session_store,
            path_validator,
        }
    }

    /// Start the scheduler loop
    pub async fn start(&self) {
        if !self.config.enabled {
            return;
        }

        self.running.store(true, Ordering::SeqCst);
        tracing::info!(
            "Scheduler started with {}s interval",
            self.config.check_interval_secs
        );

        while self.running.load(Ordering::SeqCst) {
            self.tick().await;
            tokio::time::sleep(tokio::time::Duration::from_secs(
                self.config.check_interval_secs,
            ))
            .await;
        }
    }

    /// Stop the scheduler
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Execute one tick - check and run due tasks
    async fn tick(&self) {
        let tasks = match self.storage.get_due_tasks().await {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("Failed to get due tasks: {}", e);
                return;
            }
        };

        for task in tasks {
            if let Err(e) = self.execute_task(&task).await {
                tracing::error!("Task {} execution failed: {}", task.id, e);
            }
        }
    }

    /// Execute a single task and return the execution result
    async fn execute_task(&self, task: &ScheduledTask) -> Result<TaskExecution, SchedulerError> {
        // Check concurrency limit
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|e| SchedulerError::ExecutionFailed(e.to_string()))?;

        let execution_id = Uuid::new_v4().to_string();
        let now = Utc::now();

        // Create execution record
        let mut execution = TaskExecution {
            id: execution_id.clone(),
            task_id: task.id.clone(),
            started_at: now,
            finished_at: None,
            status: ExecutionStatus::Running,
            result: None,
            error: None,
        };

        // Execute agent task
        match self.run_agent_task(task).await {
            Ok(response) => {
                execution.status = ExecutionStatus::Success;
                execution.result = Some(response);
            }
            Err(e) => {
                execution.status = ExecutionStatus::Failed;
                execution.error = Some(e.to_string());
                tracing::error!("Agent task {} failed: {}", task.id, e);
            }
        }

        execution.finished_at = Some(Utc::now());

        // Calculate next run
        let next_run = self.cron_parser.parse_next(&task.cron, Utc::now()).ok();

        // Update task timing
        self.storage
            .update_timing(&task.id, Some(now), next_run)
            .await?;

        // Save execution
        self.storage.save_execution(&execution).await?;

        tracing::info!("Task {} executed, next run: {:?}", task.id, next_run);

        Ok(execution)
    }

    /// Run an agent task
    async fn run_agent_task(&self, task: &ScheduledTask) -> Result<String, SchedulerError> {
        let config = &task.agent_config;

        // Create session for the task
        let user_id = task
            .user_id
            .as_ref()
            .map(|u| UserId::from_string(u.clone()))
            .unwrap_or_else(|| UserId::from_string("scheduler".to_string()));

        let session = self.session_store.create_session_with_user(&user_id).await;
        let session_id = session.session_id.clone();
        let sandbox_id = session.sandbox_id.clone();

        // Prepare initial message with the task input
        let user_message = ChatMessage::user(config.input.clone());
        let mut messages = vec![user_message];

        // Create tool context with path validation
        let tool_ctx = ToolContext {
            sandbox_id: sandbox_id.clone(),
            working_dir: PathBuf::from("/tmp"),
            path_validator: self.path_validator.clone(),
        };

        // Create event channel (discard events)
        let (_tx, _) = broadcast::channel::<AgentEvent>(100);

        // Build a snapshot of the tool registry for this task execution
        let tools_snapshot = {
            let guard = self.tools.lock().unwrap_or_else(|e| e.into_inner());
            let mut snapshot = ToolRegistry::new();
            for (name, tool) in guard.iter() {
                snapshot.register_arc(name.clone(), tool);
            }
            Arc::new(snapshot)
        };

        // Create and configure agent loop
        let mut agent_loop =
            AgentLoop::new(self.provider.clone(), tools_snapshot, self.memory.clone())
                .with_model(config.model.clone());

        // Run agent with timeout
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(config.timeout_secs),
            agent_loop.run(
                &session_id,
                &user_id,
                &sandbox_id,
                &mut messages,
                _tx.clone(),
                tool_ctx,
                None,
            ),
        )
        .await;

        match result {
            Ok(Ok(_)) => {
                // Extract response from last assistant message
                let response = messages
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
                    .unwrap_or_else(|| "Task completed".to_string());

                tracing::info!(
                    task_id = %task.id,
                    session_id = %session_id,
                    "Scheduled task completed successfully"
                );

                Ok(response)
            }
            Ok(Err(e)) => {
                tracing::error!(task_id = %task.id, error = %e, "Agent execution error");
                Err(SchedulerError::ExecutionFailed(e.to_string()))
            }
            Err(_) => {
                tracing::error!(task_id = %task.id, "Agent execution timed out");
                Err(SchedulerError::ExecutionFailed(format!(
                    "Timeout after {} seconds",
                    config.timeout_secs
                )))
            }
        }
    }

    // === Public API ===

    /// Create a new task
    pub async fn create_task(
        &self,
        user_id: Option<String>,
        name: String,
        cron: String,
        agent_config: AgentTaskConfig,
        enabled: bool,
    ) -> Result<ScheduledTask, SchedulerError> {
        // Validate cron
        self.cron_parser.validate(&cron)?;

        let now = Utc::now();
        let next_run = self.cron_parser.parse_next(&cron, now)?;

        let task = ScheduledTask {
            id: Uuid::new_v4().to_string(),
            user_id,
            name,
            cron,
            agent_config,
            enabled,
            last_run: None,
            next_run: Some(next_run),
            created_at: now,
            updated_at: now,
        };

        self.storage.save_task(&task).await?;

        Ok(task)
    }

    /// List tasks
    pub async fn list_tasks(
        &self,
        user_id: Option<&str>,
    ) -> Result<Vec<ScheduledTask>, SchedulerError> {
        self.storage.list_tasks(user_id).await
    }

    /// Get task by ID
    pub async fn get_task(&self, task_id: &str) -> Result<Option<ScheduledTask>, SchedulerError> {
        self.storage.get_task(task_id).await
    }

    /// Delete task
    pub async fn delete_task(&self, task_id: &str) -> Result<(), SchedulerError> {
        self.storage.delete_task(task_id).await
    }

    /// Update task
    pub async fn update_task(
        &self,
        task_id: &str,
        name: Option<String>,
        cron: Option<String>,
        agent_config: Option<AgentTaskConfig>,
        enabled: Option<bool>,
    ) -> Result<ScheduledTask, SchedulerError> {
        let mut task = self
            .storage
            .get_task(task_id)
            .await?
            .ok_or_else(|| SchedulerError::TaskNotFound(task_id.to_string()))?;

        if let Some(n) = name {
            task.name = n;
        }
        if let Some(c) = cron {
            self.cron_parser.validate(&c)?;
            task.cron = c;
            task.next_run = self.cron_parser.parse_next(&task.cron, Utc::now()).ok();
        }
        if let Some(ac) = agent_config {
            task.agent_config = ac;
        }
        if let Some(e) = enabled {
            task.enabled = e;
            if e {
                task.next_run = self.cron_parser.parse_next(&task.cron, Utc::now()).ok();
            }
        }

        task.updated_at = Utc::now();
        self.storage.save_task(&task).await?;

        Ok(task)
    }

    /// Run task immediately (manual trigger)
    pub async fn run_now(
        &self,
        task_id: &str,
        user_id: Option<&str>,
    ) -> Result<TaskExecution, SchedulerError> {
        let task = self
            .storage
            .get_task(task_id)
            .await?
            .ok_or_else(|| SchedulerError::TaskNotFound(task_id.to_string()))?;

        // Check user ownership
        if let (Some(req_user), Some(task_user)) = (user_id, &task.user_id) {
            if req_user != task_user {
                return Err(SchedulerError::TaskNotFound(task_id.to_string()));
            }
        }

        // Execute the task for real
        let execution = self.execute_task(&task).await?;

        Ok(execution)
    }

    /// Get task executions
    pub async fn get_executions(
        &self,
        task_id: &str,
        limit: usize,
    ) -> Result<Vec<TaskExecution>, SchedulerError> {
        self.storage.get_executions(task_id, limit).await
    }
}
