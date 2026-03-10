//! Tools commands implementation — list/info/invoke via ToolRegistry

use crate::commands::{AppState, ToolsCommands};
use crate::output::{self, TextOutput};
use crate::ui::table::Table;
use anyhow::Result;
use serde::Serialize;

/// Handle tools commands
pub async fn handle_tools(action: ToolsCommands, state: &AppState) -> Result<()> {
    match action {
        ToolsCommands::List => list_tools(state).await?,
        ToolsCommands::Invoke { tool_name, args } => invoke_tool(tool_name, args, state).await?,
        ToolsCommands::Info { tool_name } => show_tool_info(tool_name, state).await?,
    }
    Ok(())
}

// ── Output types ──────────────────────────────────────────────

#[derive(Serialize)]
struct ToolListOutput {
    tools: Vec<ToolRow>,
}

#[derive(Serialize)]
struct ToolRow {
    name: String,
    description: String,
    source: String,
}

impl TextOutput for ToolListOutput {
    fn to_text(&self) -> String {
        if self.tools.is_empty() {
            return "No tools registered.".to_string();
        }
        let mut t = Table::new(vec!["Name", "Source", "Description"]);
        for tool in &self.tools {
            t.add_row(vec![
                tool.name.clone(),
                tool.source.clone(),
                truncate(&tool.description, 60),
            ]);
        }
        format!("{} tools available:\n\n{}", self.tools.len(), t.render())
    }
}

#[derive(Serialize)]
struct ToolInfoOutput {
    name: String,
    description: String,
    source: String,
    parameters: serde_json::Value,
}

impl TextOutput for ToolInfoOutput {
    fn to_text(&self) -> String {
        let mut out = format!("Tool: {}\n", self.name);
        out.push_str(&format!("Source: {}\n", self.source));
        out.push_str(&format!("Description: {}\n", self.description));
        out.push_str(&format!(
            "\nParameters:\n{}",
            serde_json::to_string_pretty(&self.parameters).unwrap_or_else(|_| "{}".to_string())
        ));
        out
    }
}

#[derive(Serialize)]
struct ToolInvokeOutput {
    tool_name: String,
    success: bool,
    output: String,
}

impl TextOutput for ToolInvokeOutput {
    fn to_text(&self) -> String {
        let icon = if self.success { "OK" } else { "FAIL" };
        format!("[{}] {}: {}", icon, self.tool_name, self.output)
    }
}

// ── Handlers ──────────────────────────────────────────────────

async fn list_tools(state: &AppState) -> Result<()> {
    let registry = state.agent_runtime.tools();
    let guard = registry.lock().unwrap_or_else(|e| e.into_inner());
    let specs = guard.specs();

    let out = ToolListOutput {
        tools: specs
            .into_iter()
            .map(|s| ToolRow {
                name: s.name.clone(),
                description: s.description.clone(),
                source: "built-in".to_string(),
            })
            .collect(),
    };
    drop(guard);
    output::print_output(&out, &state.output_config);
    Ok(())
}

async fn invoke_tool(tool_name: String, args: Option<String>, state: &AppState) -> Result<()> {
    let registry = state.agent_runtime.tools();
    let guard = registry.lock().unwrap_or_else(|e| e.into_inner());
    let tool = guard.get(&tool_name);
    drop(guard);

    let tool = match tool {
        Some(t) => t,
        None => {
            let out = ToolInvokeOutput {
                tool_name,
                success: false,
                output: "Tool not found".to_string(),
            };
            output::print_output(&out, &state.output_config);
            return Ok(());
        }
    };

    let params: serde_json::Value = match args {
        Some(json_str) => serde_json::from_str(&json_str)
            .map_err(|e| anyhow::anyhow!("Invalid JSON args: {}", e))?,
        None => serde_json::json!({}),
    };

    let ctx = octo_types::ToolContext {
        sandbox_id: octo_types::SandboxId::from_string("default"),
        working_dir: state.working_dir.clone(),
        path_validator: None,
    };

    let result = tool.execute(params, &ctx).await;
    let out = match result {
        Ok(output) => ToolInvokeOutput {
            tool_name,
            success: !output.is_error,
            output: output.content,
        },
        Err(e) => ToolInvokeOutput {
            tool_name,
            success: false,
            output: format!("Error: {}", e),
        },
    };
    output::print_output(&out, &state.output_config);
    Ok(())
}

async fn show_tool_info(tool_name: String, state: &AppState) -> Result<()> {
    let registry = state.agent_runtime.tools();
    let guard = registry.lock().unwrap_or_else(|e| e.into_inner());
    let tool = guard.get(&tool_name);

    match tool {
        Some(t) => {
            let spec = t.spec();
            let source = format!("{:?}", t.source());
            drop(guard);
            let out = ToolInfoOutput {
                name: spec.name,
                description: spec.description,
                source,
                parameters: spec.input_schema,
            };
            output::print_output(&out, &state.output_config);
        }
        None => {
            drop(guard);
            eprintln!("Tool not found: {}", tool_name);
        }
    }
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    let s = s.replace('\n', " ");
    if s.len() <= max {
        s
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_list_output_empty() {
        let out = ToolListOutput { tools: vec![] };
        assert!(out.to_text().contains("No tools"));
    }

    #[test]
    fn test_tool_list_output_with_tools() {
        let out = ToolListOutput {
            tools: vec![ToolRow {
                name: "bash".to_string(),
                description: "Execute bash commands".to_string(),
                source: "built-in".to_string(),
            }],
        };
        let text = out.to_text();
        assert!(text.contains("bash"));
        assert!(text.contains("1 tools"));
    }

    #[test]
    fn test_tool_info_output() {
        let out = ToolInfoOutput {
            name: "file_read".to_string(),
            description: "Read a file".to_string(),
            source: "BuiltIn".to_string(),
            parameters: serde_json::json!({"type": "object"}),
        };
        let text = out.to_text();
        assert!(text.contains("file_read"));
        assert!(text.contains("Read a file"));
    }

    #[test]
    fn test_tool_invoke_output_success() {
        let out = ToolInvokeOutput {
            tool_name: "bash".to_string(),
            success: true,
            output: "hello world".to_string(),
        };
        assert!(out.to_text().contains("[OK]"));
    }

    #[test]
    fn test_tool_invoke_output_fail() {
        let out = ToolInvokeOutput {
            tool_name: "bash".to_string(),
            success: false,
            output: "error".to_string(),
        };
        assert!(out.to_text().contains("[FAIL]"));
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("short", 10), "short");
        let long = "a".repeat(100);
        let result = truncate(&long, 20);
        assert!(result.ends_with("..."));
    }
}
