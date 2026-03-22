//! `execute_skill` tool — allows the Agent to execute a skill by name.
//!
//! For KNOWLEDGE skills, returns the skill body as guidance text.
//! For PLAYBOOK skills, spawns a SubAgent with isolated context.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use futures_util::StreamExt;
use serde_json::json;

use octo_types::skill::ExecutionMode;
use octo_types::{ContentBlock, MessageRole, ToolContext, ToolOutput, ToolSource};

use crate::agent::events::AgentEvent;
use crate::agent::harness::run_agent_loop;
use crate::agent::loop_config::AgentLoopConfig;
use crate::agent::subagent::SubAgentManager;
use crate::skills::constraint::ToolConstraintEnforcer;
use crate::skills::registry::SkillRegistry;
use crate::tools::traits::Tool;

/// Tool that executes a skill by name.
///
/// - KNOWLEDGE: returns the skill body as instructions for the agent to follow.
/// - PLAYBOOK: spawns an isolated SubAgent that follows the skill's instructions
///   with a constrained tool set.
pub struct ExecuteSkillTool {
    skill_registry: Arc<SkillRegistry>,
    subagent_manager: Option<Arc<SubAgentManager>>,
    parent_config: Option<Arc<AgentLoopConfig>>,
}

impl ExecuteSkillTool {
    pub fn new(skill_registry: Arc<SkillRegistry>) -> Self {
        Self {
            skill_registry,
            subagent_manager: None,
            parent_config: None,
        }
    }

    pub fn with_subagent(
        mut self,
        manager: Arc<SubAgentManager>,
        config: Arc<AgentLoopConfig>,
    ) -> Self {
        self.subagent_manager = Some(manager);
        self.parent_config = Some(config);
        self
    }
}

#[async_trait]
impl Tool for ExecuteSkillTool {
    fn name(&self) -> &str {
        "execute_skill"
    }

    fn description(&self) -> &str {
        "Execute a skill by name. For knowledge skills, returns guidance instructions. \
         For playbook skills, spawns an isolated sub-agent to execute the skill's operations."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["skill_name", "request"],
            "properties": {
                "skill_name": {
                    "type": "string",
                    "description": "Name of the skill to execute"
                },
                "request": {
                    "type": "string",
                    "description": "Natural language description of what you want the skill to do"
                }
            }
        })
    }

    fn risk_level(&self) -> octo_types::RiskLevel {
        octo_types::RiskLevel::HighRisk
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let skill_name = params["skill_name"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let request = params["request"]
            .as_str()
            .unwrap_or("")
            .to_string();

        if skill_name.is_empty() {
            return Ok(ToolOutput::error("Missing required parameter: skill_name"));
        }
        if request.is_empty() {
            return Ok(ToolOutput::error("Missing required parameter: request"));
        }

        // Look up the skill
        let skill = match self.skill_registry.get(&skill_name) {
            Some(s) => s,
            None => {
                // List available skills for helpful error
                let available: Vec<String> = self
                    .skill_registry
                    .list_all()
                    .iter()
                    .map(|s| s.name.clone())
                    .collect();
                return Ok(ToolOutput::error(format!(
                    "Skill '{}' not found. Available skills: {}",
                    skill_name,
                    available.join(", ")
                )));
            }
        };

        match skill.execution_mode {
            ExecutionMode::Knowledge => {
                // Return the skill body as guidance text
                let output = if skill.body.is_empty() {
                    format!(
                        "Skill '{}' activated: {}\n\n(No additional instructions provided)",
                        skill.name, skill.description
                    )
                } else {
                    format!(
                        "## Skill: {}\n\n{}\n\n---\nNow apply these instructions to: {}",
                        skill.name, skill.body, request
                    )
                };
                Ok(ToolOutput::success(output))
            }
            ExecutionMode::Playbook => {
                self.execute_playbook(&skill, &request).await
            }
        }
    }
}

impl ExecuteSkillTool {
    async fn execute_playbook(
        &self,
        skill: &octo_types::SkillDefinition,
        request: &str,
    ) -> Result<ToolOutput> {
        let manager = match &self.subagent_manager {
            Some(m) => m,
            None => {
                return Ok(ToolOutput::error(
                    "Cannot execute playbook skill: no SubAgent manager configured",
                ));
            }
        };
        let parent_config = match &self.parent_config {
            Some(c) => c,
            None => {
                return Ok(ToolOutput::error(
                    "Cannot execute playbook skill: no parent config available",
                ));
            }
        };

        // Check recursion depth
        let child_mgr = match manager.child() {
            Ok(mgr) => Arc::new(mgr),
            Err(e) => {
                return Ok(ToolOutput::error(format!(
                    "SubAgent depth limit reached: {}",
                    e
                )));
            }
        };

        // Check concurrent limit
        if !manager.can_spawn().await {
            return Ok(ToolOutput::error(
                "Maximum concurrent sub-agents reached",
            ));
        }

        // Generate sub-agent ID
        let subagent_id = format!("skill-{}-{}", skill.name, uuid::Uuid::new_v4());

        // Register in manager
        if let Err(e) = manager
            .register(subagent_id.clone(), format!("Skill: {}", skill.name))
            .await
        {
            return Ok(ToolOutput::error(format!(
                "Failed to register sub-agent: {}",
                e
            )));
        }

        // Build filtered tool registry based on skill's allowed_tools
        let tools = if let Some(ref parent_tools) = parent_config.tools {
            if skill.allowed_tools.is_some() {
                let enforcer =
                    ToolConstraintEnforcer::from_active_skills(&[skill.clone()]);
                let all_names: Vec<String> = parent_tools.names();
                let filtered = enforcer.filter_tools(&all_names);
                Some(Arc::new(parent_tools.snapshot_filtered(&filtered)))
            } else {
                Some(parent_tools.clone())
            }
        } else {
            None
        };

        // Build system prompt from skill body
        let system_prompt = format!(
            "You are executing the '{}' skill.\n\n{}\n\n## Your Task\n{}",
            skill.name,
            if skill.body.is_empty() {
                &skill.description
            } else {
                &skill.body
            },
            request
        );

        // Build child config
        let child_config = AgentLoopConfig {
            max_iterations: 10,
            provider: parent_config.provider.clone(),
            tools,
            memory: None, // Isolated — no shared memory
            model: parent_config.model.clone(),
            session_id: octo_types::SessionId::from_string(subagent_id.clone()),
            user_id: parent_config.user_id.clone(),
            sandbox_id: parent_config.sandbox_id.clone(),
            tool_ctx: parent_config.tool_ctx.clone(),
            manifest: Some(crate::agent::entry::AgentManifest {
                name: format!("skill-{}", skill.name),
                tags: vec![],
                role: None,
                goal: None,
                backstory: None,
                system_prompt: Some(system_prompt),
                model: skill.model.clone(),
                tool_filter: vec![],
                config: crate::agent::config::AgentConfig::default(),
                max_concurrent_tasks: 0,
                priority: None,
            }),
            subagent_manager: Some(child_mgr),
            ..AgentLoopConfig::default()
        };

        // Run synchronously — wait for SubAgent to complete
        let messages = vec![octo_types::ChatMessage::user(request)];
        let mut stream = run_agent_loop(child_config, messages);
        let mut final_output = String::new();
        let mut iterations_used = 0u32;

        while let Some(event) = stream.next().await {
            match event {
                AgentEvent::Completed(result) => {
                    iterations_used = result.rounds;
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
                AgentEvent::Error { message } => {
                    let _ = manager.fail(&subagent_id, message.clone()).await;
                    return Ok(ToolOutput::error(format!(
                        "Skill '{}' execution failed: {}",
                        skill.name, message
                    )));
                }
                _ => {}
            }
        }

        if final_output.is_empty() {
            let _ = manager
                .fail(&subagent_id, "No output produced".to_string())
                .await;
            Ok(ToolOutput::error(format!(
                "Skill '{}' produced no output",
                skill.name
            )))
        } else {
            let _ = manager
                .complete(&subagent_id, Some(final_output.clone()))
                .await;
            Ok(ToolOutput::success(format!(
                "## Skill '{}' Result (iterations: {})\n\n{}",
                skill.name, iterations_used, final_output
            )))
        }
    }
}
