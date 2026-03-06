//! Scheduled task execution methods for AgentRuntime.

use std::sync::Arc;

use octo_types::{ChatMessage, ContentBlock, MessageRole, ToolContext, UserId};

use crate::scheduler::ScheduledTask;

use super::runtime::AgentRuntime;
use super::{AgentError, AgentEvent, AgentLoop};

impl AgentRuntime {
    /// Execute a scheduled task: create session, run agent, return result.
    /// Reuses provider/tools/memory from this AgentRuntime.
    pub async fn execute_scheduled_task(&self, task: &ScheduledTask) -> Result<String, AgentError> {
        let config = &task.agent_config;

        // Create session for the task using the session store
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

        // Create tool context with security policy for path validation
        let tool_ctx = ToolContext {
            sandbox_id: sandbox_id.clone(),
            working_dir: self.working_dir.clone(),
            path_validator: Some(self.security_policy.clone() as std::sync::Arc<dyn octo_types::PathValidator>),
        };

        // Create event channel (discard events)
        let (tx, _) = tokio::sync::broadcast::channel::<AgentEvent>(100);

        // Create and configure agent loop using this runtime's dependencies
        let tools = {
            let tools_guard = self.tools.lock().unwrap_or_else(|e| e.into_inner());
            Arc::new(tools_guard.snapshot())
        };

        let mut agent_loop = AgentLoop::new(self.provider.clone(), tools, self.memory.clone())
            .with_model(config.model.clone());

        // Run agent with timeout
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(config.timeout_secs),
            agent_loop.run(
                &session_id,
                &user_id,
                &sandbox_id,
                &mut messages,
                tx,
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
                Err(AgentError::ScheduledTask(e.to_string()))
            }
            Err(_) => {
                tracing::error!(task_id = %task.id, "Agent execution timed out");
                Err(AgentError::ScheduledTask(format!(
                    "Timeout after {} seconds",
                    config.timeout_secs
                )))
            }
        }
    }
}
