use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::debug;

use octo_types::{RiskLevel, ToolContext, ToolOutput, ToolSource};

use super::traits::Tool;

const MAX_RESULTS: usize = 100;

pub struct GrepTool;

impl Default for GrepTool {
    fn default() -> Self {
        Self::new()
    }
}

impl GrepTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        super::prompts::GREP_DESCRIPTION
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The regex pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "Directory or file to search in (default: working directory)"
                },
                "include": {
                    "type": "string",
                    "description": "Glob pattern to filter files (e.g. '*.rs', '*.py')"
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let pattern = params["pattern"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'pattern' parameter"))?;

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

        let include = params["include"].as_str();

        debug!(?pattern, ?search_path, ?include, "running grep");

        let mut cmd = tokio::process::Command::new("grep");
        cmd.stdin(std::process::Stdio::null())
            .arg("-rn").arg("-E").arg("--color=never");

        if let Some(glob) = include {
            cmd.arg("--include").arg(glob);
        }

        cmd.arg("--").arg(pattern).arg(&search_path);

        let output = tokio::time::timeout(std::time::Duration::from_secs(30), cmd.output()).await;

        match output {
            Ok(Ok(out)) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let lines: Vec<&str> = stdout.lines().collect();
                let total = lines.len();

                let result_text = if total > MAX_RESULTS {
                    let truncated: String = lines[..MAX_RESULTS].join("\n");
                    format!(
                        "{truncated}\n\n[... {total} total matches, showing first {MAX_RESULTS}]"
                    )
                } else if total == 0 {
                    "No matches found.".to_string()
                } else {
                    lines.join("\n")
                };

                Ok(ToolOutput::success(result_text))
            }
            Ok(Err(e)) => Ok(ToolOutput::error(format!("grep failed: {e}"))),
            Err(_) => Ok(ToolOutput::error(
                "grep timed out after 30 seconds".to_string(),
            )),
        }
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }
}
