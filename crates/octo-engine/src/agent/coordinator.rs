//! AV-T5: Coordinator mode for multi-agent orchestration.
//!
//! A Coordinator agent delegates work to worker agents instead of executing
//! tools directly. It gets a specialized system prompt defining orchestration
//! tools and worker capabilities.

use serde::{Deserialize, Serialize};

/// Configuration for Coordinator mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinatorConfig {
    /// Tools available to worker agents (subset of full registry).
    pub worker_tools: Vec<String>,
    /// MCP servers accessible to workers.
    #[serde(default)]
    pub mcp_servers: Vec<String>,
}

impl CoordinatorConfig {
    /// Default tool subset for workers — excludes agent_spawn to prevent recursion.
    pub fn default_worker_tools() -> Vec<String> {
        vec![
            "bash",
            "file_read",
            "file_write",
            "file_edit",
            "grep",
            "glob",
            "web_fetch",
            "web_search",
        ]
        .into_iter()
        .map(String::from)
        .collect()
    }
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            worker_tools: Self::default_worker_tools(),
            mcp_servers: Vec::new(),
        }
    }
}

/// Build the coordinator system prompt section.
pub fn build_coordinator_prompt(config: &CoordinatorConfig) -> String {
    let worker_tools_list = config.worker_tools.join(", ");
    let mcp_list = if config.mcp_servers.is_empty() {
        "None".to_string()
    } else {
        config.mcp_servers.join(", ")
    };

    format!(
        r#"## Coordinator Mode

You are a task orchestrator that coordinates work across multiple worker agents.

### Your Tools
- `agent_spawn` — Create worker agents with specific tasks
- `send_message` — Continue a running worker agent
- `task_stop` — Stop a worker agent

### Worker Capabilities
Each worker agent has access to these tools: {worker_tools_list}
MCP servers available to workers: {mcp_list}

### Best Practices
1. **Parallel research**: Spawn multiple workers for independent investigation tasks
2. **Serial implementation**: Use one worker at a time for file modifications to avoid conflicts
3. **No worker-to-worker**: All coordination flows through you. Workers cannot spawn other workers.
4. **Synthesize before delegating**: Review worker results before assigning next task
5. **Clear task descriptions**: Give each worker a complete, self-contained task description
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coordinator_prompt_contains_role_definition() {
        let config = CoordinatorConfig {
            worker_tools: vec!["bash".into(), "file_read".into(), "grep".into()],
            mcp_servers: vec!["postgres".into()],
        };
        let prompt = build_coordinator_prompt(&config);
        assert!(prompt.contains("orchestrator"));
        assert!(prompt.contains("bash"));
        assert!(prompt.contains("postgres"));
        assert!(prompt.contains("agent_spawn"));
    }

    #[test]
    fn test_coordinator_default_worker_tools() {
        let defaults = CoordinatorConfig::default_worker_tools();
        assert!(defaults.contains(&"bash".to_string()));
        assert!(defaults.contains(&"file_read".to_string()));
        assert!(defaults.contains(&"grep".to_string()));
        assert!(!defaults.contains(&"agent_spawn".to_string()));
    }

    #[test]
    fn test_coordinator_config_default() {
        let config = CoordinatorConfig::default();
        assert!(!config.worker_tools.is_empty());
        assert!(config.mcp_servers.is_empty());
    }

    #[test]
    fn test_coordinator_prompt_no_mcp() {
        let config = CoordinatorConfig {
            worker_tools: vec!["bash".into()],
            mcp_servers: vec![],
        };
        let prompt = build_coordinator_prompt(&config);
        assert!(prompt.contains("None"));
    }
}
