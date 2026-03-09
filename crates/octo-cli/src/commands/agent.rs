//! Agent commands implementation

use crate::commands::{AgentCommands, AppState};
use anyhow::Result;

/// Handle agent commands
pub async fn handle_agent(action: AgentCommands, state: &AppState) -> Result<()> {
    match action {
        AgentCommands::List => list_agents(state).await?,
        AgentCommands::Run { agent_id } => run_agent(agent_id, state).await?,
        AgentCommands::Info { agent_id } => show_agent_info(agent_id, state).await?,
    }
    Ok(())
}

/// List all available agents
async fn list_agents(state: &AppState) -> Result<()> {
    let agents = state.agent_catalog.list_all();
    if agents.is_empty() {
        println!("No agents found.");
    } else {
        println!("Available agents:");
        for agent in agents {
            println!("  - {} (status: {:?})", agent.id, agent.state);
        }
    }
    Ok(())
}

/// Run an agent for interactive conversation
async fn run_agent(agent_id: Option<String>, state: &AppState) -> Result<()> {
    let agent_id = agent_id.unwrap_or_else(|| {
        state
            .agent_catalog
            .list_all()
            .into_iter()
            .next()
            .map(|e| e.id.to_string())
            .unwrap_or_default()
    });

    if agent_id.is_empty() {
        println!("No agent available. Please specify an agent ID or register one first.");
        return Ok(());
    }

    println!("Starting interactive session with agent: {}", agent_id);
    println!("(Interactive mode not yet implemented - use octo-server for full functionality)");

    // TODO: Implement interactive mode using dialoguer
    Ok(())
}

/// Show agent details
async fn show_agent_info(agent_id: String, state: &AppState) -> Result<()> {
    let agents = state.agent_catalog.list_all();
    let found = agents.into_iter().find(|a| a.id.to_string() == agent_id);

    match found {
        Some(entry) => {
            println!("Agent ID: {}", entry.id);
            println!("  Status: {:?}", entry.state);
            println!("  Tenant ID: {}", entry.tenant_id);
            println!("  Created: {}", entry.created_at);
            println!("  Manifest:");
            println!("    Name: {}", entry.manifest.name);
            if let Some(role) = &entry.manifest.role {
                println!("    Role: {}", role);
            }
            if let Some(goal) = &entry.manifest.goal {
                println!("    Goal: {}", goal);
            }
            if let Some(backstory) = &entry.manifest.backstory {
                println!("    Backstory: {}", backstory);
            }
            if !entry.manifest.tags.is_empty() {
                println!("    Tags: {:?}", entry.manifest.tags);
            }
        }
        None => {
            println!("Agent not found: {}", agent_id);
        }
    }
    Ok(())
}
