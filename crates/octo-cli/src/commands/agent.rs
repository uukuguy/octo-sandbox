//! Agent commands implementation

use crate::commands::{AgentCommands, AppState};
use crate::ui::table::Table;
use anyhow::Result;
use octo_engine::agent::config::AgentConfig;
use octo_engine::{AgentId, AgentManifest, AgentStatus};

/// Handle agent commands
pub async fn handle_agent(action: AgentCommands, state: &AppState) -> Result<()> {
    match action {
        AgentCommands::List => list_agents(state).await?,
        AgentCommands::Info { agent_id } => show_agent_info(agent_id, state).await?,
        AgentCommands::Create { name, role, goal } => {
            create_agent(name, role, goal, state).await?
        }
        AgentCommands::Start { agent_id } => {
            update_agent_state(agent_id, AgentStatus::Running, "start", state).await?
        }
        AgentCommands::Pause { agent_id } => {
            update_agent_state(agent_id, AgentStatus::Paused, "pause", state).await?
        }
        AgentCommands::Stop { agent_id } => {
            update_agent_state(agent_id, AgentStatus::Stopped, "stop", state).await?
        }
        AgentCommands::Delete { agent_id } => delete_agent(agent_id, state).await?,
    }
    Ok(())
}

/// List all available agents in table format
async fn list_agents(state: &AppState) -> Result<()> {
    let agents = state.agent_catalog.list_all();
    if agents.is_empty() {
        println!("No agents found.");
        return Ok(());
    }

    let mut table = Table::new(vec!["ID", "Name", "Status", "Role", "Created"]);
    for agent in &agents {
        let created = chrono::DateTime::from_timestamp(agent.created_at, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| agent.created_at.to_string());
        table.add_row(vec![
            agent.id.to_string(),
            agent.manifest.name.clone(),
            agent.state.to_string(),
            agent.manifest.role.clone().unwrap_or_default(),
            created,
        ]);
    }
    table.print();
    println!("\n{} agent(s) total", agents.len());
    Ok(())
}

/// Show agent details
async fn show_agent_info(agent_id: String, state: &AppState) -> Result<()> {
    let aid = AgentId(agent_id.clone());
    match state.agent_catalog.get(&aid) {
        Some(entry) => {
            println!("Agent: {}", entry.id);
            println!("  Name:    {}", entry.manifest.name);
            println!("  Status:  {}", entry.state);
            println!("  Tenant:  {}", entry.tenant_id);
            if let Some(role) = &entry.manifest.role {
                println!("  Role:    {}", role);
            }
            if let Some(goal) = &entry.manifest.goal {
                println!("  Goal:    {}", goal);
            }
            if let Some(backstory) = &entry.manifest.backstory {
                println!("  Story:   {}", backstory);
            }
            if !entry.manifest.tags.is_empty() {
                println!("  Tags:    {}", entry.manifest.tags.join(", "));
            }
            if let Some(model) = &entry.manifest.model {
                println!("  Model:   {}", model);
            }
            let created = chrono::DateTime::from_timestamp(entry.created_at, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                .unwrap_or_else(|| entry.created_at.to_string());
            println!("  Created: {}", created);
        }
        None => {
            eprintln!("Agent not found: {}", agent_id);
        }
    }
    Ok(())
}

/// Create a new agent and register it in the catalog
async fn create_agent(
    name: String,
    role: Option<String>,
    goal: Option<String>,
    state: &AppState,
) -> Result<()> {
    let manifest = AgentManifest {
        name: name.clone(),
        role,
        goal,
        tags: Vec::new(),
        backstory: None,
        system_prompt: None,
        model: None,
        tool_filter: Vec::new(),
        config: AgentConfig::default(),
        max_concurrent_tasks: 0,
        priority: None,
        coordinator: false,
        worker_allowed_tools: Vec::new(),
    };
    let agent_id = state.agent_catalog.register(manifest, None);
    println!("Created agent: {} (ID: {})", name, agent_id);
    Ok(())
}

/// Update agent state (start/pause/stop)
async fn update_agent_state(
    agent_id: String,
    new_state: AgentStatus,
    action: &str,
    state: &AppState,
) -> Result<()> {
    let aid = AgentId(agent_id.clone());
    if state.agent_catalog.get(&aid).is_some() {
        state.agent_catalog.update_state(&aid, new_state.clone());
        println!("Agent {} is now {}", agent_id, new_state);
    } else {
        eprintln!("Agent not found: {}", agent_id);
        anyhow::bail!("cannot {} agent: not found", action);
    }
    Ok(())
}

/// Delete an agent from the catalog
async fn delete_agent(agent_id: String, state: &AppState) -> Result<()> {
    let aid = AgentId(agent_id.clone());
    if state.agent_catalog.unregister(&aid).is_some() {
        println!("Deleted agent: {}", agent_id);
    } else {
        eprintln!("Agent not found: {}", agent_id);
        anyhow::bail!("cannot delete agent: not found");
    }
    Ok(())
}
