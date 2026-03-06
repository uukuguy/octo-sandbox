use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::id::SandboxId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSource {
    BuiltIn,
    Mcp(String),   // MCP server name
    Skill(String), // Skill name
    Plugin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub output: String,
    pub is_error: bool,
}

impl ToolResult {
    pub fn success(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            is_error: false,
        }
    }

    pub fn error(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            is_error: true,
        }
    }
}

/// Trait for validating file paths against security policies.
pub trait PathValidator: Send + Sync + std::fmt::Debug {
    fn check_path(&self, path: &Path) -> Result<(), String>;
}

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub sandbox_id: SandboxId,
    pub working_dir: PathBuf,
    pub path_validator: Option<Arc<dyn PathValidator>>,
}
