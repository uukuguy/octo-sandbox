use std::sync::Arc;

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use serde_json::json;
use tokio::fs;

use octo_types::{ToolContext, ToolResult, ToolSource};

use crate::skill_runtime::SkillContext;
use crate::skills::runtime_bridge::SkillRuntimeBridge;
use crate::tools::Tool;

/// Wraps a user-invocable Skill as a callable Tool.
///
/// Supports three actions:
/// - `activate` (default): Returns the skill's instructions for the agent to follow.
/// - `list_scripts`: Lists available scripts in the skill's `scripts/` directory.
/// - `run_script`: Executes a script file via the `SkillRuntimeBridge`.
pub struct SkillTool {
    skill: octo_types::SkillDefinition,
    runtime_bridge: Option<Arc<SkillRuntimeBridge>>,
}

impl SkillTool {
    pub fn new(skill: octo_types::SkillDefinition) -> Self {
        Self {
            skill,
            runtime_bridge: None,
        }
    }

    pub fn with_runtime_bridge(mut self, bridge: Arc<SkillRuntimeBridge>) -> Self {
        self.runtime_bridge = Some(bridge);
        self
    }
}

#[async_trait]
impl Tool for SkillTool {
    fn name(&self) -> &str {
        &self.skill.name
    }

    fn description(&self) -> &str {
        &self.skill.description
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "Action to perform: activate (default), run_script, list_scripts",
                    "enum": ["activate", "run_script", "list_scripts"]
                },
                "args": {
                    "type": "string",
                    "description": "Optional arguments for the skill or script name for run_script"
                }
            },
            "required": []
        })
    }

    fn source(&self) -> ToolSource {
        ToolSource::Skill(self.skill.name.clone())
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("activate");

        match action {
            "activate" => {
                // Return the skill's instructions as the tool result.
                // The Agent will follow these instructions.
                Ok(ToolResult::success(&self.skill.body))
            }
            "list_scripts" => {
                let scripts_dir = self.skill.base_dir.join("scripts");
                if !scripts_dir.exists() {
                    return Ok(ToolResult::success("No scripts directory found."));
                }
                let mut scripts = Vec::new();
                let mut entries = fs::read_dir(&scripts_dir)
                    .await
                    .context("Failed to read scripts directory")?;
                while let Some(entry) = entries.next_entry().await? {
                    if let Some(name) = entry.file_name().to_str() {
                        scripts.push(name.to_string());
                    }
                }
                if scripts.is_empty() {
                    Ok(ToolResult::success(
                        "No scripts found in scripts/ directory.",
                    ))
                } else {
                    Ok(ToolResult::success(format!(
                        "Available scripts:\n{}",
                        scripts.join("\n")
                    )))
                }
            }
            "run_script" => {
                let bridge = self
                    .runtime_bridge
                    .as_ref()
                    .context("No runtime bridge configured for this skill")?;
                let script_name = params
                    .get("args")
                    .and_then(|v| v.as_str())
                    .context("run_script requires 'args' parameter with script name")?;
                let script_path = self.skill.base_dir.join("scripts").join(script_name);
                if !script_path.exists() {
                    return Ok(ToolResult::error(format!(
                        "Script not found: {}",
                        script_name
                    )));
                }
                let skill_ctx =
                    SkillContext::new(self.skill.name.clone(), self.skill.base_dir.clone());
                let result = bridge
                    .execute_script_file(&script_path, json!({}), &skill_ctx)
                    .await?;
                Ok(ToolResult::success(result.to_string()))
            }
            other => Ok(ToolResult::error(format!(
                "Unknown action: '{}'. Valid actions: activate, run_script, list_scripts",
                other
            ))),
        }
    }
}
