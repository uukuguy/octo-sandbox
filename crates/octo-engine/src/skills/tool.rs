use anyhow::Result;
use async_trait::async_trait;

use octo_types::{ToolContext, ToolResult, ToolSource};

use crate::tools::Tool;

/// Wraps a user-invocable Skill as a callable Tool.
pub struct SkillTool {
    skill: octo_types::SkillDefinition,
}

impl SkillTool {
    pub fn new(skill: octo_types::SkillDefinition) -> Self {
        Self { skill }
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
        serde_json::json!({
            "type": "object",
            "properties": {
                "args": {
                    "type": "string",
                    "description": "Optional arguments for the skill"
                }
            },
            "required": []
        })
    }

    fn source(&self) -> ToolSource {
        ToolSource::Skill(self.skill.name.clone())
    }

    async fn execute(
        &self,
        _params: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolResult> {
        // Return the skill's instructions as the tool result.
        // The Agent will follow these instructions.
        Ok(ToolResult::success(&self.skill.body))
    }
}
