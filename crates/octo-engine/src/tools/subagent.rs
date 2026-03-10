//! SubAgent tools — spawn and query sub-agents via run_agent_loop() (D4).

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use futures_util::StreamExt;
use serde_json::json;

use octo_types::{
    ChatMessage, ContentBlock, MessageRole, RiskLevel, ToolContext, ToolOutput, ToolSource,
};

use crate::agent::events::AgentEvent;
use crate::agent::harness::run_agent_loop;
use crate::agent::loop_config::AgentLoopConfig;
use crate::agent::subagent::{SubAgentManager, SubAgentStatus};

use super::traits::Tool;

/// Tool that spawns a sub-agent by recursively calling run_agent_loop().
pub struct SpawnSubAgentTool {
    subagent_manager: Arc<SubAgentManager>,
    /// Template config cloned for each child agent.
    parent_config: Arc<AgentLoopConfig>,
}

impl SpawnSubAgentTool {
    pub fn new(manager: Arc<SubAgentManager>, config: Arc<AgentLoopConfig>) -> Self {
        Self {
            subagent_manager: manager,
            parent_config: config,
        }
    }
}

#[async_trait]
impl Tool for SpawnSubAgentTool {
    fn name(&self) -> &str {
        "spawn_subagent"
    }

    fn description(&self) -> &str {
        "Spawn a sub-agent to handle a delegated task. The sub-agent runs asynchronously \
         and its result can be retrieved with query_subagent."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["task"],
            "properties": {
                "task": {
                    "type": "string",
                    "description": "Description of the task for the sub-agent"
                },
                "max_iterations": {
                    "type": "integer",
                    "description": "Max LLM iterations for the sub-agent (default: 10)"
                },
                "tools_whitelist": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional list of tool names the sub-agent can use"
                }
            }
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::HighRisk
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let task = params["task"]
            .as_str()
            .unwrap_or("No task specified")
            .to_string();

        let max_iterations = params["max_iterations"].as_u64().unwrap_or(10) as u32;

        // Check recursion depth
        let child_mgr = match self.subagent_manager.child() {
            Ok(mgr) => Arc::new(mgr),
            Err(e) => {
                return Ok(ToolOutput::error(format!("Cannot spawn sub-agent: {e}")));
            }
        };

        // Check concurrent limit
        if !self.subagent_manager.can_spawn().await {
            return Ok(ToolOutput::error("Maximum concurrent sub-agents reached"));
        }

        // Generate sub-agent ID
        let subagent_id = format!("sa-{}", uuid::Uuid::new_v4());

        // Register in manager
        if let Err(e) = self
            .subagent_manager
            .register(subagent_id.clone(), task.clone())
            .await
        {
            return Ok(ToolOutput::error(format!("Failed to register sub-agent: {e}")));
        }

        // Build child config from parent template
        let tools = if let Some(whitelist) = params["tools_whitelist"].as_array() {
            let names: Vec<String> = whitelist
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            self.parent_config
                .tools
                .as_ref()
                .map(|t| Arc::new(t.snapshot_filtered(&names)))
        } else {
            self.parent_config.tools.clone()
        };

        let child_config = AgentLoopConfig {
            max_iterations,
            provider: self.parent_config.provider.clone(),
            tools,
            memory: self.parent_config.memory.clone(),
            model: self.parent_config.model.clone(),
            session_id: octo_types::SessionId::from_string(subagent_id.clone()),
            user_id: self.parent_config.user_id.clone(),
            sandbox_id: self.parent_config.sandbox_id.clone(),
            tool_ctx: self.parent_config.tool_ctx.clone(),
            ..AgentLoopConfig::default()
        };

        // Build context messages for the sub-agent
        let messages = vec![ChatMessage::user(&task)];

        // Spawn async task
        let mgr = self.subagent_manager.clone();
        let sa_id = subagent_id.clone();
        tokio::spawn(async move {
            let mut stream = run_agent_loop(child_config, messages);
            let mut final_output = String::new();

            while let Some(event) = stream.next().await {
                if let AgentEvent::Completed(result) = event {
                    // Extract assistant response from final_messages
                    final_output = result
                        .final_messages
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
                        .unwrap_or_default();
                }
            }

            if final_output.is_empty() {
                let _ = mgr
                    .fail(&sa_id, "No output produced".to_string())
                    .await;
            } else {
                let _ = mgr.complete(&sa_id, Some(final_output)).await;
            }
        });

        Ok(ToolOutput::success(json!({
                "session_id": subagent_id,
                "status": "spawned",
                "depth": child_mgr.depth(),
            })
            .to_string()))
    }
}

/// Tool that queries the status/result of a previously spawned sub-agent.
pub struct QuerySubAgentTool {
    subagent_manager: Arc<SubAgentManager>,
}

impl QuerySubAgentTool {
    pub fn new(manager: Arc<SubAgentManager>) -> Self {
        Self {
            subagent_manager: manager,
        }
    }
}

#[async_trait]
impl Tool for QuerySubAgentTool {
    fn name(&self) -> &str {
        "query_subagent"
    }

    fn description(&self) -> &str {
        "Query the status and result of a previously spawned sub-agent."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["session_id"],
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "The session_id returned by spawn_subagent"
                }
            }
        })
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let session_id = params["session_id"]
            .as_str()
            .unwrap_or("")
            .to_string();

        if session_id.is_empty() {
            return Ok(ToolOutput::error("Missing session_id parameter"));
        }

        let agents = self.subagent_manager.list().await;
        if let Some(handle) = agents.iter().find(|a| a.id == session_id) {
            let status_str = match &handle.status {
                SubAgentStatus::Running => "running",
                SubAgentStatus::Completed => "completed",
                SubAgentStatus::Failed(_) => "failed",
                SubAgentStatus::Cancelled => "cancelled",
            };

            let error_msg = if let SubAgentStatus::Failed(e) = &handle.status {
                Some(e.clone())
            } else {
                None
            };

            Ok(ToolOutput::success(json!({
                    "session_id": session_id,
                    "status": status_str,
                    "description": handle.description,
                    "error": error_msg,
                })
                .to_string()))
        } else {
            Ok(ToolOutput::error(json!({
                    "session_id": session_id,
                    "status": "not_found",
                })
                .to_string()))
        }
    }
}
