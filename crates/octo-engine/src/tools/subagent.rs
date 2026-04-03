//! SubAgent tools — spawn and query sub-agents via run_agent_loop() (D4).

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use futures_util::StreamExt;
use serde_json::json;

use octo_types::{
    ChatMessage, ContentBlock, MessageRole, RiskLevel, ToolContext, ToolOutput, ToolProgress,
    ToolSource,
};

use crate::agent::catalog::AgentCatalog;
use crate::agent::entry::AgentManifest;
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
    /// Agent catalog for looking up built-in/YAML agent definitions (Phase AX).
    catalog: Option<Arc<AgentCatalog>>,
}

impl SpawnSubAgentTool {
    pub fn new(manager: Arc<SubAgentManager>, config: Arc<AgentLoopConfig>) -> Self {
        Self {
            subagent_manager: manager,
            parent_config: config,
            catalog: None,
        }
    }

    /// Attach an agent catalog for agent_type lookup.
    pub fn with_catalog(mut self, catalog: Arc<AgentCatalog>) -> Self {
        self.catalog = Some(catalog);
        self
    }

    /// Resolve tool set for the child agent.
    /// Priority: manifest (tool_filter + disallowed_tools) → coordinator → LLM whitelist
    fn resolve_tools(
        &self,
        params: &serde_json::Value,
        manifest: &Option<AgentManifest>,
    ) -> Option<Arc<super::ToolRegistry>> {
        let parent_tools = self.parent_config.tools.as_ref()?;

        if let Some(manifest) = manifest {
            // Step 1: Apply tool_filter (whitelist) — empty = all
            let base = if manifest.tool_filter.is_empty() {
                parent_tools.snapshot()
            } else {
                parent_tools.snapshot_filtered(&manifest.tool_filter)
            };
            // Step 2: Apply disallowed_tools (blacklist)
            let filtered = if manifest.disallowed_tools.is_empty() {
                base
            } else {
                base.snapshot_excluded(&manifest.disallowed_tools)
            };
            Some(Arc::new(filtered))
        } else {
            // No manifest — fallback to existing coordinator + LLM whitelist logic
            let coordinator_filter = self
                .parent_config
                .manifest
                .as_ref()
                .filter(|m| m.coordinator)
                .map(|m| {
                    if m.worker_allowed_tools.is_empty() {
                        crate::agent::coordinator::CoordinatorConfig::default_worker_tools()
                    } else {
                        m.worker_allowed_tools.clone()
                    }
                });

            if let Some(whitelist) = params["tools_whitelist"].as_array() {
                let mut names: Vec<String> = whitelist
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();
                if let Some(ref allowed) = coordinator_filter {
                    names.retain(|n| allowed.contains(n));
                }
                Some(Arc::new(parent_tools.snapshot_filtered(&names)))
            } else if let Some(ref allowed) = coordinator_filter {
                Some(Arc::new(parent_tools.snapshot_filtered(allowed)))
            } else {
                self.parent_config.tools.clone()
            }
        }
    }
}

#[async_trait]
impl Tool for SpawnSubAgentTool {
    fn name(&self) -> &str {
        "spawn_subagent"
    }

    fn description(&self) -> &str {
        super::prompts::SUBAGENT_DESCRIPTION
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
                "agent_type": {
                    "type": "string",
                    "description": "Optional agent type name. When provided, uses the agent's \
                        configured tools, model, and system prompt from the agent catalog. \
                        Built-in types: general-purpose, explore, plan, coder, reviewer, verification"
                },
                "max_iterations": {
                    "type": "integer",
                    "description": "Max LLM iterations for the sub-agent (default: 10)"
                },
                "tools_whitelist": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional list of tool names the sub-agent can use (overridden when agent_type is provided)"
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

        let agent_type = params["agent_type"].as_str().map(String::from);
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

        // Resolve manifest from agent_type (if provided)
        let manifest: Option<AgentManifest> = if let Some(ref at) = agent_type {
            match &self.catalog {
                Some(catalog) => catalog.get_by_name(at).map(|e| e.manifest),
                None => {
                    tracing::warn!(agent_type = %at, "agent_type specified but no catalog available");
                    None
                }
            }
        } else {
            None
        };

        // Generate sub-agent ID
        let subagent_id = format!("sa-{}", uuid::Uuid::new_v4());

        // Build description with agent type prefix
        let desc = if let Some(ref m) = manifest {
            format!("[{}] {}", m.name, &task[..task.len().min(80)])
        } else {
            task.clone()
        };

        // Register in manager
        if let Err(e) = self
            .subagent_manager
            .register(subagent_id.clone(), desc)
            .await
        {
            return Ok(ToolOutput::error(format!("Failed to register sub-agent: {e}")));
        }

        // Resolve tools: manifest overrides → coordinator filter → LLM whitelist
        let tools = self.resolve_tools(&params, &manifest);

        // Resolve model: manifest.model → parent model
        let model = manifest
            .as_ref()
            .and_then(|m| m.model.as_ref())
            .filter(|m| m.as_str() != "inherit")
            .cloned()
            .unwrap_or_else(|| self.parent_config.model.clone());

        // Resolve max_iterations: manifest.max_turns → param → default
        let max_iter = manifest
            .as_ref()
            .and_then(|m| m.max_turns)
            .unwrap_or(max_iterations);

        // Build manifest for child config (with task prepended to system prompt)
        let child_manifest = manifest.map(|mut m| {
            if let Some(ref sp) = m.system_prompt {
                m.system_prompt = Some(format!("{}\n\n## Your Task\n{}", sp, task));
            }
            m
        });

        let child_config = AgentLoopConfig {
            max_iterations: max_iter,
            provider: self.parent_config.provider.clone(),
            tools,
            memory: self.parent_config.memory.clone(),
            model,
            session_id: octo_types::SessionId::from_string(subagent_id.clone()),
            user_id: self.parent_config.user_id.clone(),
            sandbox_id: self.parent_config.sandbox_id.clone(),
            tool_ctx: self.parent_config.tool_ctx.clone(),
            manifest: child_manifest,
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
                "agent_type": agent_type,
                "depth": child_mgr.depth(),
            })
            .to_string()))
    }

    async fn execute_with_progress(
        &self,
        params: serde_json::Value,
        ctx: &ToolContext,
        on_progress: Option<super::traits::ProgressCallback>,
    ) -> Result<ToolOutput> {
        if let Some(ref cb) = on_progress {
            cb(ToolProgress::indeterminate("spawning sub-agent..."));
        }
        let result = self.execute(params, ctx).await;
        if let Some(ref cb) = on_progress {
            cb(ToolProgress::percent(1.0, "sub-agent spawned"));
        }
        result
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

    async fn execute_with_progress(
        &self,
        params: serde_json::Value,
        ctx: &ToolContext,
        on_progress: Option<super::traits::ProgressCallback>,
    ) -> Result<ToolOutput> {
        if let Some(ref cb) = on_progress {
            cb(ToolProgress::indeterminate("querying sub-agent..."));
        }
        self.execute(params, ctx).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::catalog::AgentCatalog;
    use crate::agent::entry::{AgentManifest, AgentSource};
    use crate::tools::ToolRegistry;

    fn make_registry() -> ToolRegistry {
        // Minimal registry with a few mock tool names for testing
        let mut reg = ToolRegistry::new();
        reg.register(crate::tools::sleep::SleepTool);
        reg.register(crate::tools::doctor::DoctorTool);
        reg.register(crate::tools::notifier::NotifierTool);
        reg
    }

    #[test]
    fn test_snapshot_excluded_basic() {
        let reg = make_registry();
        let excluded = reg.snapshot_excluded(&["sleep".to_string()]);
        assert!(excluded.get("sleep").is_none());
        assert!(excluded.get("doctor").is_some());
        assert!(excluded.get("notify").is_some());
    }

    #[test]
    fn test_snapshot_excluded_empty_list() {
        let reg = make_registry();
        let excluded = reg.snapshot_excluded(&[]);
        assert_eq!(excluded.names().len(), reg.names().len());
    }

    #[test]
    fn test_resolve_tools_with_manifest_blacklist() {
        let reg = make_registry();
        let config = AgentLoopConfig {
            tools: Some(Arc::new(reg)),
            ..Default::default()
        };
        let mgr = Arc::new(SubAgentManager::new(4, 3));
        let tool = SpawnSubAgentTool::new(mgr, Arc::new(config));

        let manifest = Some(AgentManifest {
            disallowed_tools: vec!["sleep".to_string()],
            ..Default::default()
        });

        let params = json!({"task": "test"});
        let resolved = tool.resolve_tools(&params, &manifest).unwrap();
        assert!(resolved.get("sleep").is_none());
        assert!(resolved.get("doctor").is_some());
    }

    #[test]
    fn test_resolve_tools_with_manifest_whitelist() {
        let reg = make_registry();
        let config = AgentLoopConfig {
            tools: Some(Arc::new(reg)),
            ..Default::default()
        };
        let mgr = Arc::new(SubAgentManager::new(4, 3));
        let tool = SpawnSubAgentTool::new(mgr, Arc::new(config));

        let manifest = Some(AgentManifest {
            tool_filter: vec!["sleep".to_string(), "doctor".to_string()],
            ..Default::default()
        });

        let params = json!({"task": "test"});
        let resolved = tool.resolve_tools(&params, &manifest).unwrap();
        assert!(resolved.get("sleep").is_some());
        assert!(resolved.get("doctor").is_some());
        assert!(resolved.get("notifier").is_none());
    }

    #[test]
    fn test_resolve_tools_whitelist_and_blacklist() {
        let reg = make_registry();
        let config = AgentLoopConfig {
            tools: Some(Arc::new(reg)),
            ..Default::default()
        };
        let mgr = Arc::new(SubAgentManager::new(4, 3));
        let tool = SpawnSubAgentTool::new(mgr, Arc::new(config));

        // Whitelist: sleep + doctor; then blacklist: doctor
        let manifest = Some(AgentManifest {
            tool_filter: vec!["sleep".to_string(), "doctor".to_string()],
            disallowed_tools: vec!["doctor".to_string()],
            ..Default::default()
        });

        let params = json!({"task": "test"});
        let resolved = tool.resolve_tools(&params, &manifest).unwrap();
        assert!(resolved.get("sleep").is_some());
        assert!(resolved.get("doctor").is_none());
        assert!(resolved.get("notifier").is_none());
    }

    #[test]
    fn test_resolve_tools_no_manifest_uses_parent() {
        let reg = make_registry();
        let config = AgentLoopConfig {
            tools: Some(Arc::new(reg)),
            ..Default::default()
        };
        let mgr = Arc::new(SubAgentManager::new(4, 3));
        let tool = SpawnSubAgentTool::new(mgr, Arc::new(config));

        let params = json!({"task": "test"});
        let resolved = tool.resolve_tools(&params, &None).unwrap();
        // Should get all 3 tools from parent
        assert_eq!(resolved.names().len(), 3);
    }

    #[test]
    fn test_with_catalog_lookup() {
        let catalog = AgentCatalog::new();
        catalog.register(
            AgentManifest {
                name: "my-agent".into(),
                system_prompt: Some("Hello".into()),
                source: AgentSource::BuiltIn,
                ..Default::default()
            },
            None,
        );

        let config = AgentLoopConfig::default();
        let mgr = Arc::new(SubAgentManager::new(4, 3));
        let tool = SpawnSubAgentTool::new(mgr, Arc::new(config))
            .with_catalog(Arc::new(catalog));

        assert!(tool.catalog.is_some());
    }
}
