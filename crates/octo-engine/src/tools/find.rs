use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::debug;

use octo_types::{ToolContext, ToolResult, ToolSource};

use super::traits::Tool;

const MAX_RESULTS: usize = 200;

pub struct FindTool;

impl FindTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for FindTool {
    fn name(&self) -> &str {
        "find"
    }

    fn description(&self) -> &str {
        "Search for files and directories by name pattern. Uses the system find command. Good for locating files when you know part of the name."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory to search in (default: working directory)"
                },
                "name": {
                    "type": "string",
                    "description": "File name pattern (supports wildcards: *.rs, test_*)"
                },
                "type": {
                    "type": "string",
                    "enum": ["f", "d"],
                    "description": "Type filter: 'f' for files, 'd' for directories"
                }
            },
            "required": ["name"]
        })
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let name = params["name"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'name' parameter"))?;

        let search_path = params["path"]
            .as_str()
            .map(|p| {
                if std::path::Path::new(p).is_absolute() {
                    std::path::PathBuf::from(p)
                } else {
                    ctx.working_dir.join(p)
                }
            })
            .unwrap_or_else(|| ctx.working_dir.clone());

        // Security: validate search path against policy
        if let Some(ref validator) = ctx.path_validator {
            if let Err(e) = validator.check_path(&search_path) {
                return Ok(ToolResult::error(format!("Path validation failed: {e}")));
            }
        }

        let type_filter = params["type"].as_str();

        debug!(?name, ?search_path, ?type_filter, "running find");

        let mut cmd = tokio::process::Command::new("find");
        cmd.arg(&search_path);

        // Exclude common directories
        cmd.args([
            "-not",
            "-path",
            "*/node_modules/*",
            "-not",
            "-path",
            "*/.git/*",
            "-not",
            "-path",
            "*/target/*",
            "-not",
            "-path",
            "*/__pycache__/*",
        ]);

        if let Some(t) = type_filter {
            cmd.arg("-type").arg(t);
        }

        cmd.arg("-name").arg(name);

        let output = tokio::time::timeout(std::time::Duration::from_secs(30), cmd.output()).await;

        match output {
            Ok(Ok(out)) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let lines: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
                let total = lines.len();

                let result_text = if total > MAX_RESULTS {
                    let truncated: String = lines[..MAX_RESULTS].join("\n");
                    format!(
                        "{truncated}\n\n[... {total} total results, showing first {MAX_RESULTS}]"
                    )
                } else if total == 0 {
                    "No files found.".to_string()
                } else {
                    format!("{}\n\n[{total} results]", lines.join("\n"))
                };

                Ok(ToolResult::success(result_text))
            }
            Ok(Err(e)) => Ok(ToolResult::error(format!("find failed: {e}"))),
            Err(_) => Ok(ToolResult::error(
                "find timed out after 30 seconds".to_string(),
            )),
        }
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }
}
