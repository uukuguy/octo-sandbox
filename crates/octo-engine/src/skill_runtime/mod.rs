use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::sandbox::SandboxProfile;

pub mod nodejs;
pub mod python;
pub mod shell;
pub mod traits;
#[cfg(feature = "sandbox-wasm")]
pub mod wasm;

pub use nodejs::NodeJsRuntime;
pub use python::PythonRuntime;
pub use shell::ShellRuntime;
pub use traits::{RuntimeType, SkillRuntime};
#[cfg(feature = "sandbox-wasm")]
pub use wasm::WasmSkillRuntime;

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
    /// Sandbox profile controlling execution environment.
    pub sandbox_profile: Option<SandboxProfile>,
}

impl SkillContext {
    pub fn new(skill_name: String, working_dir: PathBuf) -> Self {
        Self {
            skill_name,
            tools: Vec::new(),
            working_dir,
            sandbox_profile: None,
        }
    }

    pub fn with_tools(mut self, tools: Vec<ToolInfo>) -> Self {
        self.tools = tools;
        self
    }

    pub fn with_sandbox_profile(mut self, profile: SandboxProfile) -> Self {
        self.sandbox_profile = Some(profile);
        self
    }

    /// Get the effective timeout for this context based on the sandbox profile.
    pub fn effective_timeout_secs(&self) -> u64 {
        self.sandbox_profile
            .as_ref()
            .map(|p| p.timeout_secs())
            .unwrap_or(30)
    }

    /// Whether environment variables should be passed through to the subprocess.
    pub fn env_passthrough(&self) -> bool {
        self.sandbox_profile
            .as_ref()
            .map(|p| p.env_passthrough())
            .unwrap_or(true)
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
