use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::debug;

use octo_types::{ApprovalRequirement, RiskLevel, ToolContext, ToolOutput, ToolSource};

use super::traits::Tool;

pub struct FileEditTool;

impl Default for FileEditTool {
    fn default() -> Self {
        Self::new()
    }
}

impl FileEditTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for FileEditTool {
    fn name(&self) -> &str {
        "file_edit"
    }

    fn description(&self) -> &str {
        super::prompts::FILE_EDIT_DESCRIPTION
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File path (absolute or relative to working directory)"
                },
                "old_string": {
                    "type": "string",
                    "description": "The exact string to find and replace"
                },
                "new_string": {
                    "type": "string",
                    "description": "The replacement string"
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "Replace all occurrences (default: false)"
                }
            },
            "required": ["path", "old_string", "new_string"]
        })
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let path_str = params["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'path' parameter"))?;
        let old_string = params["old_string"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'old_string' parameter"))?;
        let new_string = params["new_string"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'new_string' parameter"))?;
        let replace_all = params["replace_all"].as_bool().unwrap_or(false);

        let path = if std::path::Path::new(path_str).is_absolute() {
            std::path::PathBuf::from(path_str)
        } else {
            ctx.working_dir.join(path_str)
        };

        // Security: validate path against policy
        if let Some(ref validator) = ctx.path_validator {
            if let Err(e) = validator.check_path(&path) {
                return Ok(ToolOutput::error(format!("Path validation failed: {e}")));
            }
        }

        // Symlink defense: reject symbolic links
        if let Some(output) = super::path_safety::reject_symlink(&path) {
            return Ok(output);
        }

        debug!(
            ?path,
            old_len = old_string.len(),
            new_len = new_string.len(),
            "editing file"
        );

        if !path.exists() {
            return Ok(ToolOutput::error(format!(
                "File not found: {}",
                path.display()
            )));
        }

        let content = match tokio::fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => return Ok(ToolOutput::error(format!("Failed to read file: {e}"))),
        };

        let count = content.matches(old_string).count();
        if count == 0 {
            return Ok(ToolOutput::error(
                "old_string not found in file. Make sure it matches exactly (including whitespace and indentation)."
                    .to_string(),
            ));
        }

        if !replace_all && count > 1 {
            return Ok(ToolOutput::error(format!(
                "old_string found {count} times. Provide more context to make it unique, or set replace_all=true."
            )));
        }

        let new_content = if replace_all {
            content.replace(old_string, new_string)
        } else {
            content.replacen(old_string, new_string, 1)
        };

        match tokio::fs::write(&path, &new_content).await {
            Ok(()) => Ok(ToolOutput::success(format!(
                "Replaced {} occurrence(s) in {}",
                if replace_all { count } else { 1 },
                path.display()
            ))),
            Err(e) => Ok(ToolOutput::error(format!("Failed to write file: {e}"))),
        }
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::HighRisk
    }

    fn approval(&self) -> ApprovalRequirement {
        ApprovalRequirement::AutoApprovable
    }

    fn is_concurrency_safe(&self) -> bool {
        false
    }
}
