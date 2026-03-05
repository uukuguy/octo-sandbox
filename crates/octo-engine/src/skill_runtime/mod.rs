use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub mod python;
pub mod traits;

pub use python::PythonRuntime;
pub use traits::{RuntimeType, SkillRuntime};

/// Information about a tool available to the skill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Context passed to a skill when executed.
#[derive(Debug, Clone)]
pub struct SkillContext {
    /// Name of the skill being executed.
    pub skill_name: String,
    /// List of tools available to the skill.
    pub tools: Vec<ToolInfo>,
    /// Working directory for the skill execution.
    pub working_dir: PathBuf,
}

impl SkillContext {
    pub fn new(skill_name: String, working_dir: PathBuf) -> Self {
        Self {
            skill_name,
            tools: Vec::new(),
            working_dir,
        }
    }

    pub fn with_tools(mut self, tools: Vec<ToolInfo>) -> Self {
        self.tools = tools;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_context_creation() {
        let ctx = SkillContext::new("test_skill".to_string(), PathBuf::from("/tmp"));
        assert_eq!(ctx.skill_name, "test_skill");
        assert_eq!(ctx.working_dir, PathBuf::from("/tmp"));
        assert!(ctx.tools.is_empty());
    }

    #[test]
    fn test_skill_context_with_tools() {
        let tools = vec![ToolInfo {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        }];
        let ctx = SkillContext::new("test_skill".to_string(), PathBuf::from("/tmp"))
            .with_tools(tools.clone());
        assert_eq!(ctx.tools, tools);
    }

    #[test]
    fn test_tool_info_clone() {
        let tool = ToolInfo {
            name: "test".to_string(),
            description: "test desc".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        };
        let cloned = tool.clone();
        assert_eq!(cloned.name, tool.name);
    }
}
