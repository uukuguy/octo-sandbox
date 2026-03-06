use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::debug;

use octo_types::{ToolContext, ToolResult, ToolSource};

use super::traits::Tool;

const MAX_RESULTS: usize = 200;

pub struct GlobTool;

impl GlobTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }

    fn description(&self) -> &str {
        "Find files matching a glob pattern. Returns file paths sorted by modification time (newest first). Useful for discovering files by name or extension."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern (e.g. '**/*.rs', 'src/**/*.ts', '*.json')"
                },
                "path": {
                    "type": "string",
                    "description": "Base directory for the pattern (default: working directory)"
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let pattern = params["pattern"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'pattern' parameter"))?;

        let base_dir = params["path"]
            .as_str()
            .map(|p| {
                if std::path::Path::new(p).is_absolute() {
                    std::path::PathBuf::from(p)
                } else {
                    ctx.working_dir.join(p)
                }
            })
            .unwrap_or_else(|| ctx.working_dir.clone());

        // Security: validate base directory against policy
        if let Some(ref validator) = ctx.path_validator {
            if let Err(e) = validator.check_path(&base_dir) {
                return Ok(ToolResult::error(format!("Path validation failed: {e}")));
            }
        }

        let full_pattern = base_dir.join(pattern);
        let pattern_str = full_pattern.to_string_lossy().to_string();

        debug!(?pattern_str, "running glob");

        // Run in blocking task since glob is synchronous
        let result = tokio::task::spawn_blocking(move || {
            let mut entries: Vec<(std::path::PathBuf, std::time::SystemTime)> = Vec::new();

            match glob::glob(&pattern_str) {
                Ok(paths) => {
                    for entry in paths.flatten() {
                        let mtime = entry
                            .metadata()
                            .and_then(|m| m.modified())
                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                        entries.push((entry, mtime));
                    }
                }
                Err(e) => return Err(format!("Invalid glob pattern: {e}")),
            }

            // Sort by modification time, newest first
            entries.sort_by(|a, b| b.1.cmp(&a.1));

            Ok(entries)
        })
        .await
        .map_err(|e| anyhow::anyhow!("glob task failed: {e}"))?;

        match result {
            Ok(entries) => {
                let total = entries.len();
                let display_entries = &entries[..total.min(MAX_RESULTS)];

                let output: String = display_entries
                    .iter()
                    .map(|(p, _)| p.to_string_lossy().to_string())
                    .collect::<Vec<_>>()
                    .join("\n");

                let result_text = if total > MAX_RESULTS {
                    format!("{output}\n\n[... {total} total matches, showing first {MAX_RESULTS}]")
                } else if total == 0 {
                    "No files found matching pattern.".to_string()
                } else {
                    format!("{output}\n\n[{total} files found]")
                };

                Ok(ToolResult::success(result_text))
            }
            Err(e) => Ok(ToolResult::error(e)),
        }
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }
}
