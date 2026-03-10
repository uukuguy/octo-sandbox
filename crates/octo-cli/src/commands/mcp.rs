//! MCP server management commands — list/add/remove/status/logs via McpManager

use crate::commands::{AppState, McpCommands};
use crate::output::{self, TextOutput};
use crate::ui::table::Table;
use anyhow::Result;
use serde::Serialize;
use std::collections::HashMap;

/// Handle MCP commands
pub async fn handle_mcp(action: McpCommands, state: &AppState) -> Result<()> {
    match action {
        McpCommands::List => list_servers(state).await?,
        McpCommands::Add {
            name,
            command,
            args,
        } => add_server(name, command, args, state).await?,
        McpCommands::Remove { name } => remove_server(name, state).await?,
        McpCommands::Status { name } => show_status(name, state).await?,
        McpCommands::Logs { name, lines } => show_logs(name, lines, state).await?,
    }
    Ok(())
}

// ── Output types ──────────────────────────────────────────────

#[derive(Serialize)]
struct McpListOutput {
    servers: Vec<McpServerRow>,
}

#[derive(Serialize)]
struct McpServerRow {
    name: String,
    status: String,
    tools: usize,
}

impl TextOutput for McpListOutput {
    fn to_text(&self) -> String {
        if self.servers.is_empty() {
            return "No MCP servers configured.".to_string();
        }
        let mut t = Table::new(vec!["Name", "Status", "Tools"]);
        for s in &self.servers {
            t.add_row(vec![
                s.name.clone(),
                s.status.clone(),
                s.tools.to_string(),
            ]);
        }
        format!("{} MCP server(s):\n\n{}", self.servers.len(), t.render())
    }
}

#[derive(Serialize)]
struct McpAddOutput {
    name: String,
    tools_count: usize,
    tool_names: Vec<String>,
}

impl TextOutput for McpAddOutput {
    fn to_text(&self) -> String {
        let mut out = format!(
            "Added MCP server '{}' ({} tools)\n",
            self.name, self.tools_count
        );
        for name in &self.tool_names {
            out.push_str(&format!("  - {}\n", name));
        }
        out
    }
}

#[derive(Serialize)]
struct McpRemoveOutput {
    name: String,
}

impl TextOutput for McpRemoveOutput {
    fn to_text(&self) -> String {
        format!("Removed MCP server '{}'", self.name)
    }
}

#[derive(Serialize)]
struct McpStatusOutput {
    servers: Vec<McpStatusRow>,
}

#[derive(Serialize)]
struct McpStatusRow {
    name: String,
    state: String,
    tools: Vec<String>,
}

impl TextOutput for McpStatusOutput {
    fn to_text(&self) -> String {
        if self.servers.is_empty() {
            return "No MCP servers found.".to_string();
        }
        let mut out = String::new();
        for s in &self.servers {
            out.push_str(&format!("{} [{}]\n", s.name, s.state));
            for t in &s.tools {
                out.push_str(&format!("  - {}\n", t));
            }
        }
        out
    }
}

#[derive(Serialize)]
struct McpLogsOutput {
    name: String,
    message: String,
}

impl TextOutput for McpLogsOutput {
    fn to_text(&self) -> String {
        format!("{}: {}", self.name, self.message)
    }
}

// ── Handlers ──────────────────────────────────────────────────

async fn list_servers(state: &AppState) -> Result<()> {
    let mgr = state.agent_runtime.mcp_manager();
    let guard = mgr.lock().await;
    let states = guard.all_runtime_states();
    let servers: Vec<McpServerRow> = states
        .iter()
        .map(|(name, runtime_state)| {
            let status = format!("{:?}", runtime_state);
            let tools = guard.get_tool_count(name);
            McpServerRow {
                name: name.clone(),
                status,
                tools,
            }
        })
        .collect();
    drop(guard);

    let out = McpListOutput { servers };
    output::print_output(&out, &state.output_config);
    Ok(())
}

async fn add_server(
    name: String,
    command: String,
    args: Vec<String>,
    state: &AppState,
) -> Result<()> {
    use octo_engine::mcp::McpServerConfig;

    let config = McpServerConfig {
        name: name.clone(),
        command,
        args,
        env: HashMap::new(),
    };

    let mgr = state.agent_runtime.mcp_manager();
    let mut guard = mgr.lock().await;
    let tools = guard.add_server(config).await?;

    let out = McpAddOutput {
        name,
        tools_count: tools.len(),
        tool_names: tools.iter().map(|t| t.name.clone()).collect(),
    };
    drop(guard);
    output::print_output(&out, &state.output_config);
    Ok(())
}

async fn remove_server(name: String, state: &AppState) -> Result<()> {
    let mgr = state.agent_runtime.mcp_manager();
    let mut guard = mgr.lock().await;
    guard.remove_server(&name).await?;
    drop(guard);

    let out = McpRemoveOutput { name };
    output::print_output(&out, &state.output_config);
    Ok(())
}

async fn show_status(name: Option<String>, state: &AppState) -> Result<()> {
    let mgr = state.agent_runtime.mcp_manager();
    let guard = mgr.lock().await;
    let all_states = guard.all_runtime_states();

    let servers: Vec<McpStatusRow> = match name {
        Some(ref n) => {
            let state_val = guard.get_runtime_state(n);
            let tools = guard
                .get_tool_infos(n)
                .unwrap_or_default()
                .iter()
                .map(|t| t.name.clone())
                .collect();
            vec![McpStatusRow {
                name: n.clone(),
                state: format!("{:?}", state_val),
                tools,
            }]
        }
        None => all_states
            .keys()
            .map(|n| {
                let state_val = guard.get_runtime_state(n);
                let tools = guard
                    .get_tool_infos(n)
                    .unwrap_or_default()
                    .iter()
                    .map(|t| t.name.clone())
                    .collect();
                McpStatusRow {
                    name: n.clone(),
                    state: format!("{:?}", state_val),
                    tools,
                }
            })
            .collect(),
    };
    drop(guard);

    let out = McpStatusOutput { servers };
    output::print_output(&out, &state.output_config);
    Ok(())
}

async fn show_logs(name: String, _lines: usize, state: &AppState) -> Result<()> {
    // MCP log retrieval is not directly exposed by McpManager.
    // Show status info as a fallback.
    let mgr = state.agent_runtime.mcp_manager();
    let guard = mgr.lock().await;
    let runtime_state = guard.get_runtime_state(&name);
    drop(guard);

    let out = McpLogsOutput {
        name,
        message: format!(
            "Server state: {:?} (live log streaming not yet available)",
            runtime_state
        ),
    };
    output::print_output(&out, &state.output_config);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_list_empty() {
        let out = McpListOutput { servers: vec![] };
        assert!(out.to_text().contains("No MCP servers"));
    }

    #[test]
    fn test_mcp_list_with_servers() {
        let out = McpListOutput {
            servers: vec![McpServerRow {
                name: "test-server".to_string(),
                status: "Running".to_string(),
                tools: 3,
            }],
        };
        let text = out.to_text();
        assert!(text.contains("test-server"));
        assert!(text.contains("1 MCP"));
    }

    #[test]
    fn test_mcp_add_output() {
        let out = McpAddOutput {
            name: "my-server".to_string(),
            tools_count: 2,
            tool_names: vec!["tool_a".to_string(), "tool_b".to_string()],
        };
        let text = out.to_text();
        assert!(text.contains("my-server"));
        assert!(text.contains("2 tools"));
        assert!(text.contains("tool_a"));
    }

    #[test]
    fn test_mcp_remove_output() {
        let out = McpRemoveOutput {
            name: "old-server".to_string(),
        };
        assert!(out.to_text().contains("Removed"));
    }

    #[test]
    fn test_mcp_status_empty() {
        let out = McpStatusOutput { servers: vec![] };
        assert!(out.to_text().contains("No MCP servers"));
    }

    #[test]
    fn test_mcp_status_with_server() {
        let out = McpStatusOutput {
            servers: vec![McpStatusRow {
                name: "srv".to_string(),
                state: "Running".to_string(),
                tools: vec!["t1".to_string()],
            }],
        };
        let text = out.to_text();
        assert!(text.contains("srv"));
        assert!(text.contains("Running"));
        assert!(text.contains("t1"));
    }

    #[test]
    fn test_mcp_logs_output() {
        let out = McpLogsOutput {
            name: "srv".to_string(),
            message: "state info".to_string(),
        };
        assert!(out.to_text().contains("srv"));
    }
}
