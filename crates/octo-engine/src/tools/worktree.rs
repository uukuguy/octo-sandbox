//! Worktree tools — git worktree isolation for development.
//!
//! Aligns with CC-OSS EnterWorktreeTool / ExitWorktreeTool:
//! creates isolated git worktrees for parallel development branches.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use octo_types::{RiskLevel, ToolContext, ToolOutput, ToolSource};
use serde_json::json;
use tokio::sync::Mutex;

use super::traits::Tool;

/// State for an active worktree session.
#[derive(Debug, Clone)]
pub struct WorktreeState {
    pub original_cwd: PathBuf,
    pub worktree_path: PathBuf,
    pub worktree_branch: String,
}

/// Shared worktree state — None when not in a worktree.
pub type WorktreeStore = Arc<Mutex<Option<WorktreeState>>>;

// ─── enter_worktree ────────────────────────────────────────────────────────

pub struct EnterWorktreeTool {
    state: WorktreeStore,
}

impl EnterWorktreeTool {
    pub fn new(state: WorktreeStore) -> Self {
        Self { state }
    }
}

#[async_trait]
impl Tool for EnterWorktreeTool {
    fn name(&self) -> &str {
        "enter_worktree"
    }

    fn description(&self) -> &str {
        "Create and enter an isolated git worktree for development.\n\
         \n\
         ## Parameters\n\
         - name (optional): Worktree name (letters, digits, dots, underscores, dashes; max 64 chars). Auto-generated if omitted.\n\
         \n\
         ## Behavior\n\
         - Creates a git worktree in a sibling directory\n\
         - Creates a new branch `worktree/{name}`\n\
         - Returns the worktree path for use in subsequent operations\n\
         \n\
         ## Restrictions\n\
         - Cannot nest worktrees (enter while already in one)"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Worktree name (auto-generated if omitted)"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        // Check not already in worktree
        {
            let state = self.state.lock().await;
            if state.is_some() {
                return Ok(ToolOutput::error(
                    "Already in a worktree session. Exit current worktree first.",
                ));
            }
        }

        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("wt-{}", uuid::Uuid::new_v4().as_simple()));

        // Validate name
        if name.len() > 64 {
            return Ok(ToolOutput::error("Worktree name must be at most 64 characters"));
        }
        if !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '.' || c == '_' || c == '-')
        {
            return Ok(ToolOutput::error(
                "Worktree name must contain only letters, digits, dots, underscores, or dashes",
            ));
        }

        let cwd = &ctx.working_dir;
        let branch_name = format!("worktree/{}", name);

        // Find git root
        let git_root_output = tokio::process::Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .current_dir(cwd)
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to find git root: {}", e))?;

        if !git_root_output.status.success() {
            return Ok(ToolOutput::error(
                "Not in a git repository. Worktrees require git.",
            ));
        }

        let git_root = PathBuf::from(
            String::from_utf8_lossy(&git_root_output.stdout)
                .trim()
                .to_string(),
        );

        // Worktree path as sibling of git root
        let worktree_path = git_root
            .parent()
            .unwrap_or(&git_root)
            .join(format!("octo-worktree-{}", name));

        // Create worktree
        let create_output = tokio::process::Command::new("git")
            .args([
                "worktree",
                "add",
                worktree_path.to_str().unwrap_or(""),
                "-b",
                &branch_name,
            ])
            .current_dir(&git_root)
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create worktree: {}", e))?;

        if !create_output.status.success() {
            let stderr = String::from_utf8_lossy(&create_output.stderr);
            return Ok(ToolOutput::error(format!(
                "Failed to create worktree: {}",
                stderr.trim()
            )));
        }

        // Save state
        {
            let mut state = self.state.lock().await;
            *state = Some(WorktreeState {
                original_cwd: cwd.clone(),
                worktree_path: worktree_path.clone(),
                worktree_branch: branch_name.clone(),
            });
        }

        let result = json!({
            "worktree_path": worktree_path.to_string_lossy(),
            "worktree_branch": branch_name,
            "message": format!("Created worktree at {} on branch {}", worktree_path.display(), branch_name),
        });
        Ok(ToolOutput::success(serde_json::to_string_pretty(&result)?))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::HighRisk
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn category(&self) -> &str {
        "git"
    }
}

// ─── exit_worktree ─────────────────────────────────────────────────────────

pub struct ExitWorktreeTool {
    state: WorktreeStore,
}

impl ExitWorktreeTool {
    pub fn new(state: WorktreeStore) -> Self {
        Self { state }
    }
}

#[async_trait]
impl Tool for ExitWorktreeTool {
    fn name(&self) -> &str {
        "exit_worktree"
    }

    fn description(&self) -> &str {
        "Exit the current git worktree session.\n\
         \n\
         ## Parameters\n\
         - action (required): \"keep\" to preserve worktree on disk, \"remove\" to delete it\n\
         - discard_changes (optional): Must be true when action=remove and there are uncommitted changes\n\
         \n\
         ## Safety\n\
         - Refuses to remove worktree with uncommitted changes unless discard_changes=true\n\
         - Always restores original working directory"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["keep", "remove"],
                    "description": "keep = preserve worktree, remove = delete it"
                },
                "discard_changes": {
                    "type": "boolean",
                    "description": "Required true when removing with uncommitted changes"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let action = params["action"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: action"))?;
        let discard = params
            .get("discard_changes")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let ws = {
            let state = self.state.lock().await;
            match state.as_ref() {
                Some(s) => s.clone(),
                None => {
                    return Ok(ToolOutput::error(
                        "Not in a worktree session. Use enter_worktree first.",
                    ));
                }
            }
        };

        if action == "remove" {
            // Check for uncommitted changes
            let status_output = tokio::process::Command::new("git")
                .args(["status", "--porcelain"])
                .current_dir(&ws.worktree_path)
                .output()
                .await;

            let has_changes = status_output
                .as_ref()
                .ok()
                .map(|o| !o.stdout.is_empty())
                .unwrap_or(true); // fail-closed: unknown = has changes

            if has_changes && !discard {
                return Ok(ToolOutput::error(
                    "Worktree has uncommitted changes. Set discard_changes=true to force removal.",
                ));
            }

            // Remove worktree
            let remove_output = tokio::process::Command::new("git")
                .args(["worktree", "remove", "--force", ws.worktree_path.to_str().unwrap_or("")])
                .current_dir(&ws.original_cwd)
                .output()
                .await;

            if let Ok(out) = &remove_output {
                if !out.status.success() {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    return Ok(ToolOutput::error(format!(
                        "Failed to remove worktree: {}",
                        stderr.trim()
                    )));
                }
            }

            // Delete branch
            let _ = tokio::process::Command::new("git")
                .args(["branch", "-D", &ws.worktree_branch])
                .current_dir(&ws.original_cwd)
                .output()
                .await;
        }

        // Clear state
        {
            let mut state = self.state.lock().await;
            *state = None;
        }

        let result = json!({
            "action": action,
            "original_cwd": ws.original_cwd.to_string_lossy(),
            "worktree_path": ws.worktree_path.to_string_lossy(),
            "worktree_branch": ws.worktree_branch,
            "message": if action == "keep" {
                format!("Exited worktree (kept at {})", ws.worktree_path.display())
            } else {
                "Exited and removed worktree".to_string()
            },
        });
        Ok(ToolOutput::success(serde_json::to_string_pretty(&result)?))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::HighRisk
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn category(&self) -> &str {
        "git"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_ctx() -> ToolContext {
        ToolContext {
            sandbox_id: octo_types::SandboxId::default(),
            user_id: octo_types::UserId::from_string(octo_types::id::DEFAULT_USER_ID),
            working_dir: PathBuf::from("/tmp"),
            path_validator: None,
        }
    }

    #[tokio::test]
    async fn test_enter_worktree_rejects_nested() {
        let state: WorktreeStore = Arc::new(Mutex::new(Some(WorktreeState {
            original_cwd: PathBuf::from("/original"),
            worktree_path: PathBuf::from("/wt"),
            worktree_branch: "worktree/test".to_string(),
        })));
        let tool = EnterWorktreeTool::new(state);
        let result = tool
            .execute(json!({"name": "test2"}), &test_ctx())
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("Already in a worktree"));
    }

    #[tokio::test]
    async fn test_enter_worktree_validates_name() {
        let state: WorktreeStore = Arc::new(Mutex::new(None));
        let tool = EnterWorktreeTool::new(state);

        // Invalid chars
        let result = tool
            .execute(json!({"name": "test/invalid"}), &test_ctx())
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("letters, digits"));
    }

    #[tokio::test]
    async fn test_enter_worktree_validates_length() {
        let state: WorktreeStore = Arc::new(Mutex::new(None));
        let tool = EnterWorktreeTool::new(state);

        let long_name = "a".repeat(65);
        let result = tool
            .execute(json!({"name": long_name}), &test_ctx())
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("64 characters"));
    }

    #[tokio::test]
    async fn test_exit_worktree_not_in_session() {
        let state: WorktreeStore = Arc::new(Mutex::new(None));
        let tool = ExitWorktreeTool::new(state);
        let result = tool
            .execute(json!({"action": "keep"}), &test_ctx())
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("Not in a worktree"));
    }

    #[tokio::test]
    async fn test_exit_worktree_keep_clears_state() {
        let state: WorktreeStore = Arc::new(Mutex::new(Some(WorktreeState {
            original_cwd: PathBuf::from("/original"),
            worktree_path: PathBuf::from("/tmp/octo-worktree-test"),
            worktree_branch: "worktree/test".to_string(),
        })));
        let tool = ExitWorktreeTool::new(state.clone());
        let result = tool
            .execute(json!({"action": "keep"}), &test_ctx())
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("keep"));

        // State should be cleared
        let s = state.lock().await;
        assert!(s.is_none());
    }
}
