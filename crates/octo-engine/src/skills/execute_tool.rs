//! `execute_skill` tool — allows the Agent to execute a skill by name.
//!
//! For KNOWLEDGE skills, returns the skill body as guidance text.
//! For PLAYBOOK skills, spawns a SubAgent with isolated context.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use futures_util::StreamExt;
use serde_json::json;
use tokio::sync::broadcast;

use octo_types::skill::ExecutionMode;
use octo_types::{ContentBlock, MessageRole, ToolContext, ToolOutput, ToolSource};

use crate::agent::events::AgentEvent;
use crate::agent::harness::run_agent_loop;
use crate::agent::loop_config::AgentLoopConfig;
use crate::agent::subagent::SubAgentManager;
use crate::providers::Provider;
use crate::skills::constraint::ToolConstraintEnforcer;
use crate::skills::registry::SkillRegistry;
use crate::tools::traits::Tool;
use crate::tools::ToolRegistry;

/// Context needed to spawn SubAgent for playbook skill execution.
/// Avoids circular dependency with AgentLoopConfig (which contains tools).
pub struct SubAgentContext {
    pub manager: Arc<SubAgentManager>,
    pub provider: Arc<dyn Provider>,
    pub tools: Arc<ToolRegistry>,
    pub model: String,
    pub user_id: octo_types::UserId,
    pub sandbox_id: octo_types::SandboxId,
    pub tool_ctx: Option<octo_types::ToolContext>,
    /// Optional broadcast sender to forward sub-agent streaming events to the
    /// parent agent's event stream, enabling real-time TUI rendering.
    pub event_sender: Option<broadcast::Sender<AgentEvent>>,
}

/// Tool that executes a skill by name.
///
/// - KNOWLEDGE: returns the skill body as instructions for the agent to follow.
/// - PLAYBOOK: spawns an isolated SubAgent that follows the skill's instructions
///   with a constrained tool set.
pub struct ExecuteSkillTool {
    skill_registry: Arc<SkillRegistry>,
    subagent_ctx: Option<SubAgentContext>,
}

impl ExecuteSkillTool {
    pub fn new(skill_registry: Arc<SkillRegistry>) -> Self {
        Self {
            skill_registry,
            subagent_ctx: None,
        }
    }

    /// Configure SubAgent execution context for playbook skills.
    pub fn with_subagent_ctx(mut self, ctx: SubAgentContext) -> Self {
        self.subagent_ctx = Some(ctx);
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
        let ctx = match &self.subagent_ctx {
            Some(c) => c,
            None => {
                return Ok(ToolOutput::error(
                    "Cannot execute playbook skill: no SubAgent manager configured",
                ));
            }
        };

        // Check recursion depth
        let child_mgr = match ctx.manager.child() {
            Ok(mgr) => Arc::new(mgr),
            Err(e) => {
                return Ok(ToolOutput::error(format!(
                    "SubAgent depth limit reached: {}",
                    e
                )));
            }
        };

        // Check concurrent limit
        if !ctx.manager.can_spawn().await {
            return Ok(ToolOutput::error(
                "Maximum concurrent sub-agents reached",
            ));
        }

        // Generate sub-agent ID
        let subagent_id = format!("skill-{}-{}", skill.name, uuid::Uuid::new_v4());

        // Register in manager
        if let Err(e) = ctx
            .manager
            .register(subagent_id.clone(), format!("Skill: {}", skill.name))
            .await
        {
            return Ok(ToolOutput::error(format!(
                "Failed to register sub-agent: {}",
                e
            )));
        }

        // Build filtered tool registry based on skill's allowed_tools
        let tools = if skill.allowed_tools.is_some() {
            let enforcer =
                ToolConstraintEnforcer::from_active_skills(&[skill.clone()]);
            let all_names: Vec<String> = ctx.tools.names();
            let filtered = enforcer.filter_tools(&all_names);
            Some(Arc::new(ctx.tools.snapshot_filtered(&filtered)))
        } else {
            Some(ctx.tools.clone())
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
            max_iterations: if skill.max_rounds > 0 { skill.max_rounds } else { 30 },
            provider: Some(ctx.provider.clone()),
            tools,
            memory: None, // Isolated — no shared memory
            model: ctx.model.clone(),
            session_id: octo_types::SessionId::from_string(subagent_id.clone()),
            user_id: ctx.user_id.clone(),
            sandbox_id: ctx.sandbox_id.clone(),
            tool_ctx: ctx.tool_ctx.clone(),
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

        // Short display name for TUI (e.g. "skill-review" from "skill-review-<uuid>")
        let display_id = format!("skill-{}", skill.name);

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
                    // Forward completion summary to parent as SubAgentEvent
                    if let Some(ref sender) = ctx.event_sender {
                        let _ = sender.send(AgentEvent::SubAgentEvent {
                            source_id: display_id.clone(),
                            inner: Box::new(AgentEvent::Completed(result)),
                        });
                    }
                }
                AgentEvent::Error { message } => {
                    let _ = ctx.manager.fail(&subagent_id, message.clone()).await;
                    // Forward error to parent before returning
                    if let Some(ref sender) = ctx.event_sender {
                        let _ = sender.send(AgentEvent::SubAgentEvent {
                            source_id: display_id.clone(),
                            inner: Box::new(AgentEvent::Error { message: message.clone() }),
                        });
                    }
                    return Ok(ToolOutput::error(format!(
                        "Skill '{}' execution failed: {}",
                        skill.name, message
                    )));
                }
                AgentEvent::Done => {
                    // Sub-agent stream ended — don't forward
                }
                other => {
                    // Wrap and forward streaming events to parent for TUI rendering
                    if let Some(ref sender) = ctx.event_sender {
                        let _ = sender.send(AgentEvent::SubAgentEvent {
                            source_id: display_id.clone(),
                            inner: Box::new(other),
                        });
                    }
                }
            }
        }

        if final_output.is_empty() {
            let _ = ctx
                .manager
                .fail(&subagent_id, "No output produced".to_string())
                .await;
            Ok(ToolOutput::error(format!(
                "Skill '{}' produced no output",
                skill.name
            )))
        } else {
            let _ = ctx
                .manager
                .complete(&subagent_id, Some(final_output.clone()))
                .await;
            Ok(ToolOutput::success(format!(
                "## Skill '{}' Result (iterations: {})\n\n{}",
                skill.name, iterations_used, final_output
            )))
        }
    }
}
