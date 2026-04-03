//! LLM tools for team management: team_create, team_add_member, team_dissolve.
//!
//! Teams coordinate 3+ specialized agents working on different aspects of a problem.
//! For simple parent/child delegation, use session_create instead.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use octo_types::{ToolContext, ToolOutput, ToolSource};
use serde_json::json;

use crate::agent::team::TeamManager;
use crate::tools::traits::Tool;

// ─── team_create ────────────────────────────────────────────────────────────

pub struct TeamCreateTool {
    team_manager: Arc<TeamManager>,
}

impl TeamCreateTool {
    pub fn new(team_manager: Arc<TeamManager>) -> Self {
        Self { team_manager }
    }
}

#[async_trait]
impl Tool for TeamCreateTool {
    fn name(&self) -> &str {
        "team_create"
    }

    fn description(&self) -> &str {
        r#"Create a named team for coordinating multiple specialized agents.

## When to use teams
- 3+ agents need to work on different aspects of the same problem (e.g., research + implement + test)
- Agents need to discover each other by role name
- You need leader/worker coordination with shared context
- A task is complex enough to benefit from parallel work by multiple agents

## When NOT to use teams
- Simple parent→child delegation (use session_create directly)
- Only 1-2 sub-agents needed
- Tasks are independent and don't need coordination

## Team Workflow
1. `team_create` — you become the team leader
2. `session_create` with `team_name` — spawn worker agents (they auto-join the team with teammate communication instructions)
3. `task_create` with `team` — create tasks for the team's task board
4. `session_message` — assign tasks and coordinate work
5. Receive results from workers via session_message
6. `session_stop` each worker when done
7. `team_dissolve` — clean up team resources

## Task Coordination
- Use `task_create` to define work items visible to all team members
- Use `task_update` to assign ownership and track status
- Workers should claim unassigned tasks and mark them completed
- Prefer assigning tasks in order (earlier tasks set up context for later ones)

## Communication Rules
- Your plain text output is NOT visible to team members — always use `session_message`
- Workers communicate back via `session_message` — their results arrive automatically
- Do not poll session_status repeatedly — trust workers to report back

## Parameters
- name (required): Unique team name (e.g., "backend-team", "review-squad")
- description (optional): What this team is working on

## Example
{"name": "refactor-team", "description": "Coordinate the auth system refactor"}"#
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Unique team name"
                },
                "description": {
                    "type": "string",
                    "description": "What this team is working on"
                }
            },
            "required": ["name"]
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let name = params["name"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: name"))?;
        let description = params["description"].as_str();

        // Use sandbox_id as a proxy for the current session ID
        let leader_session_id = octo_types::SessionId::from_string(ctx.sandbox_id.as_str());

        match self.team_manager.create_team(name, leader_session_id, description) {
            Ok(team) => {
                let result = json!({
                    "team": team.name,
                    "description": team.description,
                    "leader_session_id": team.leader_session_id.as_str(),
                    "members": team.members.len(),
                });
                Ok(ToolOutput::success(serde_json::to_string_pretty(&result)?))
            }
            Err(e) => Ok(ToolOutput::error(e)),
        }
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn category(&self) -> &str {
        "coordination"
    }
}

// ─── team_add_member ────────────────────────────────────────────────────────

pub struct TeamAddMemberTool {
    team_manager: Arc<TeamManager>,
}

impl TeamAddMemberTool {
    pub fn new(team_manager: Arc<TeamManager>) -> Self {
        Self { team_manager }
    }
}

#[async_trait]
impl Tool for TeamAddMemberTool {
    fn name(&self) -> &str {
        "team_add_member"
    }

    fn description(&self) -> &str {
        r#"Add a new agent member to an existing team.

## Parameters
- team (required): Team name to add member to
- name (required): Role name for this member (e.g., "coder", "reviewer", "tester")
- session_id (required): Session ID of the agent to add
- agent_type (optional): Type of agent (for discovery/routing)

## Usage pattern
1. Create the agent session first with session_create
2. Then add it to the team with team_add_member
3. Use session_message to communicate with the member

## Example
{"team": "refactor-team", "name": "coder", "session_id": "sess-abc123", "agent_type": "coder"}"#
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "team": {
                    "type": "string",
                    "description": "Team name"
                },
                "name": {
                    "type": "string",
                    "description": "Role name for this member"
                },
                "session_id": {
                    "type": "string",
                    "description": "Session ID of the agent"
                },
                "agent_type": {
                    "type": "string",
                    "description": "Type of agent"
                }
            },
            "required": ["team", "name", "session_id"]
        })
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let team = params["team"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: team"))?;
        let name = params["name"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: name"))?;
        let session_id_str = params["session_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: session_id"))?;
        let agent_type = params["agent_type"].as_str();

        let session_id = octo_types::SessionId::from_string(session_id_str);
        match self.team_manager.add_member(team, name, session_id, agent_type) {
            Ok(()) => {
                let result = json!({
                    "status": "added",
                    "team": team,
                    "member": name,
                    "session_id": session_id_str,
                });
                Ok(ToolOutput::success(serde_json::to_string_pretty(&result)?))
            }
            Err(e) => Ok(ToolOutput::error(e)),
        }
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn category(&self) -> &str {
        "coordination"
    }
}

// ─── team_dissolve ──────────────────────────────────────────────────────────

pub struct TeamDissolveTool {
    team_manager: Arc<TeamManager>,
}

impl TeamDissolveTool {
    pub fn new(team_manager: Arc<TeamManager>) -> Self {
        Self { team_manager }
    }
}

#[async_trait]
impl Tool for TeamDissolveTool {
    fn name(&self) -> &str {
        "team_dissolve"
    }

    fn description(&self) -> &str {
        r#"Dissolve a team and return the session IDs of all members.

## When to use
- Team's work is complete
- Need to clean up resources
- Restructuring teams

## Parameters
- team (required): Team name to dissolve

## Behavior
Returns a list of member session IDs. Does NOT automatically stop sessions —
use session_stop on each returned ID if you want to terminate them.

## Example
{"team": "refactor-team"}"#
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "team": {
                    "type": "string",
                    "description": "Team name to dissolve"
                }
            },
            "required": ["team"]
        })
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let team = params["team"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: team"))?;

        match self.team_manager.dissolve_team(team) {
            Ok(session_ids) => {
                let ids: Vec<&str> = session_ids.iter().map(|s| s.as_str()).collect();
                let result = json!({
                    "status": "dissolved",
                    "team": team,
                    "member_session_ids": ids,
                    "hint": "Use session_stop on each ID to terminate the agents."
                });
                Ok(ToolOutput::success(serde_json::to_string_pretty(&result)?))
            }
            Err(e) => Ok(ToolOutput::error(e)),
        }
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn is_destructive(&self) -> bool {
        true
    }

    fn category(&self) -> &str {
        "coordination"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::team::TeamManager;
    use octo_types::{SandboxId, SessionId, ToolContext};
    use std::path::PathBuf;

    fn test_ctx() -> ToolContext {
        ToolContext {
            sandbox_id: SandboxId::from_string("test-session"),
            user_id: octo_types::UserId::from_string(octo_types::id::DEFAULT_USER_ID),
            working_dir: PathBuf::from("/tmp"),
            path_validator: None,
        }
    }

    #[tokio::test]
    async fn test_team_create_tool() {
        let mgr = Arc::new(TeamManager::new());
        let tool = TeamCreateTool::new(mgr.clone());
        let result = tool
            .execute(
                json!({"name": "alpha", "description": "Test team"}),
                &test_ctx(),
            )
            .await
            .unwrap();
        assert!(result.content.contains("alpha"));
        assert!(result.content.contains("test-session"));
    }

    #[tokio::test]
    async fn test_team_add_member_tool() {
        let mgr = Arc::new(TeamManager::new());
        mgr.create_team("alpha", SessionId::from_string("s-1"), None).unwrap();
        let tool = TeamAddMemberTool::new(mgr);
        let result = tool
            .execute(
                json!({"team": "alpha", "name": "coder", "session_id": "s-2"}),
                &test_ctx(),
            )
            .await
            .unwrap();
        assert!(result.content.contains("added"));
    }

    #[tokio::test]
    async fn test_team_dissolve_tool() {
        let mgr = Arc::new(TeamManager::new());
        mgr.create_team("alpha", SessionId::from_string("s-1"), None).unwrap();
        mgr.add_member("alpha", "coder", SessionId::from_string("s-2"), None).unwrap();
        let tool = TeamDissolveTool::new(mgr);
        let result = tool
            .execute(json!({"team": "alpha"}), &test_ctx())
            .await
            .unwrap();
        assert!(result.content.contains("dissolved"));
        assert!(result.content.contains("s-1"));
        assert!(result.content.contains("s-2"));
    }
}
