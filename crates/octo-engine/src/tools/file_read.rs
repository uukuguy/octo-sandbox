use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::debug;

use octo_types::{ToolContext, ToolResult, ToolSource};

use super::traits::Tool;

const MAX_FILE_SIZE: u64 = 1_024 * 1_024; // 1 MB

pub struct FileReadTool;

impl FileReadTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &str {
        "file_read"
    }

    fn description(&self) -> &str {
        "Read the contents of a file. Returns the file content with line numbers."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The file path to read (absolute or relative to working directory)"
                },
                "offset": {
                    "type": "integer",
                    "description": "Line number to start reading from (1-based, default: 1)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of lines to read (default: 2000)"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let path_str = params["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'path' parameter"))?;

        let offset = params["offset"].as_u64().unwrap_or(1).max(1) as usize;
        let limit = params["limit"].as_u64().unwrap_or(2000) as usize;

        let path = if std::path::Path::new(path_str).is_absolute() {
            std::path::PathBuf::from(path_str)
        } else {
            ctx.working_dir.join(path_str)
        };

        // Security: validate path against policy
        if let Some(ref validator) = ctx.path_validator {
            if let Err(e) = validator.check_path(&path) {
                return Ok(ToolResult::error(format!("Path validation failed: {e}")));
            }
        }

        debug!(?path, offset, limit, "reading file");

        // Check file exists
        if !path.exists() {
            return Ok(ToolResult::error(format!(
                "File not found: {}",
                path.display()
            )));
        }

        // Check file size
        let metadata = tokio::fs::metadata(&path).await?;
        if metadata.len() > MAX_FILE_SIZE {
            return Ok(ToolResult::error(format!(
                "File too large: {} bytes (max: {MAX_FILE_SIZE} bytes)",
                metadata.len()
            )));
        }

        // Read file
        let content = tokio::fs::read_to_string(&path).await;
        match content {
            Ok(text) => {
                let lines: Vec<&str> = text.lines().collect();
                let total_lines = lines.len();

                let start_idx = (offset - 1).min(total_lines);
                let end_idx = (start_idx + limit).min(total_lines);

                let mut output = String::new();
                for (i, line) in lines[start_idx..end_idx].iter().enumerate() {
                    let line_num = start_idx + i + 1;
                    output.push_str(&format!("{:>6}\t{}\n", line_num, line));
                }

                if end_idx < total_lines {
                    output.push_str(&format!(
                        "\n[... {} more lines, {total_lines} total]",
                        total_lines - end_idx
                    ));
                }

                Ok(ToolResult::success(output))
            }
            Err(e) => Ok(ToolResult::error(format!("Failed to read file: {e}"))),
        }
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }
}
