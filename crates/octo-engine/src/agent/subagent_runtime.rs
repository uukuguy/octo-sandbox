//! SubAgent Runtime — encapsulates the complete lifecycle of a sub-agent.
//!
//! Aligns with CC-OSS's runAgent() pattern: Build → Run → Cleanup (Drop).
//! Each sub-agent is an independent runtime entity with its own resources,
//! context, and cleanup logic.

use std::sync::Arc;

use anyhow::{bail, Result};
use futures_util::StreamExt;
use tokio::sync::broadcast;

use octo_types::{ChatMessage, ContentBlock, MessageRole, SessionId};

use super::builtin_agents::preload_skills_into_prompt;
use super::entry::AgentManifest;
use super::events::AgentEvent;
use super::harness::run_agent_loop;
use super::loop_config::AgentLoopConfig;
use super::subagent::{SubAgentManager, SubAgentStatus};
use super::CancellationToken;
use crate::skills::SkillRegistry;
use crate::tools::ToolRegistry;

/// Result from a SubAgentRuntime execution.
#[derive(Debug, Clone)]
pub struct SubAgentRuntimeResult {
    pub id: String,
    pub output: String,
    pub rounds: u32,
    pub status: SubAgentStatus,
}

/// SubAgent Runtime — owns the full lifecycle of a sub-agent execution.
#[allow(dead_code)]
/// Note: manual Debug impl below (broadcast::Sender is not Debug).
///
/// Build phase: resolves tools, model, system prompt from manifest.
/// Run phase: sync (wait for result) or async (fire-and-forget).
/// Cleanup: Drop guard ensures manager state is cleaned up.
pub struct SubAgentRuntime {
    /// Unique sub-agent ID.
    pub id: String,
    /// Agent type name (from manifest).
    pub agent_type: Option<String>,
    /// Agent manifest (if resolved from catalog).
    manifest: Option<AgentManifest>,
    /// Fully resolved config for run_agent_loop.
    /// Wrapped in Option so it can be taken out for consuming methods.
    config: Option<AgentLoopConfig>,
    /// The task description.
    task: String,
    /// Parent sub-agent manager (for status tracking).
    manager: Arc<SubAgentManager>,
    /// Event sender to forward streaming events to parent agent.
    event_sender: Option<broadcast::Sender<AgentEvent>>,
}

impl std::fmt::Debug for SubAgentRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubAgentRuntime")
            .field("id", &self.id)
            .field("agent_type", &self.agent_type)
            .field("task", &self.task)
            .finish_non_exhaustive()
    }
}

impl SubAgentRuntime {
    /// Build a SubAgentRuntime from task + optional manifest.
    ///
    /// Resolves tools, model, max_turns, and system prompt.
    /// Registers the sub-agent in the parent manager.
    pub async fn build(
        task: String,
        manifest: Option<AgentManifest>,
        parent_config: &AgentLoopConfig,
        manager: Arc<SubAgentManager>,
        event_sender: Option<broadcast::Sender<AgentEvent>>,
        skill_registry: Option<&Arc<SkillRegistry>>,
    ) -> Result<Self> {
        // 1. Check recursion depth
        let child_mgr = Arc::new(manager.child()?);

        // 2. Check concurrent limit
        if !manager.can_spawn().await {
            bail!("Maximum concurrent sub-agents reached");
        }

        // 3. Generate ID
        let id = format!("sa-{}", uuid::Uuid::new_v4());
        let agent_type = manifest.as_ref().map(|m| m.name.clone());

        // 4. Register
        let desc = if let Some(ref m) = manifest {
            format!("[{}] {}", m.name, &task[..task.len().min(80)])
        } else {
            task.clone()
        };
        manager.register(id.clone(), desc).await?;

        // 5. Resolve tools: manifest (whitelist + blacklist) -> parent
        let tools = Self::resolve_tools(&manifest, parent_config);

        // 6. Resolve model
        let model = manifest
            .as_ref()
            .and_then(|m| m.model.as_ref())
            .filter(|m| m.as_str() != "inherit")
            .cloned()
            .unwrap_or_else(|| parent_config.model.clone());

        // 7. Resolve max_turns
        let max_iter = manifest
            .as_ref()
            .and_then(|m| m.max_turns)
            .unwrap_or(10);

        // 8. Build manifest for child config (skill preloading + task prepend)
        let child_manifest = manifest.clone().map(|mut m| {
            // Skill preloading
            if !m.skills.is_empty() {
                if let Some(sr) = skill_registry {
                    let base = m.system_prompt.as_deref().unwrap_or("");
                    m.system_prompt = Some(preload_skills_into_prompt(base, &m.skills, sr));
                }
            }
            // Prepend task to system prompt
            if let Some(ref sp) = m.system_prompt {
                m.system_prompt = Some(format!("{}\n\n## Your Task\n{}", sp, task));
            }
            m
        });

        // 9. Resolve cancel_token (AY-D3): child gets its own token
        let cancel_token = CancellationToken::new();

        // 10. Resolve permission (AY-D6): per-instance ApprovalManager from manifest
        let approval_manager = manifest
            .as_ref()
            .and_then(|m| m.permission_mode.as_ref())
            .map(|mode| {
                use crate::tools::approval::{ApprovalManager, ApprovalPolicy};
                let policy = match mode.as_str() {
                    "auto" | "bypassPermissions" => ApprovalPolicy::AlwaysApprove,
                    "ask" | "default" => ApprovalPolicy::AlwaysAsk,
                    _ => ApprovalPolicy::AlwaysApprove,
                };
                Arc::new(ApprovalManager::new(policy))
            })
            .or_else(|| parent_config.approval_manager.clone());

        // 11. Resolve hook_registry (AY-D5): scope by manifest.hook_scope
        let hook_registry = if let Some(ref scope) = manifest.as_ref().and_then(|m| m.hook_scope.clone()) {
            if let Some(ref hr) = parent_config.hook_registry {
                Some(Arc::new(hr.scoped(scope).await))
            } else {
                None
            }
        } else {
            parent_config.hook_registry.clone()
        };

        // 12. Build AgentLoopConfig
        let config = AgentLoopConfig {
            max_iterations: max_iter,
            provider: parent_config.provider.clone(),
            tools,
            memory: parent_config.memory.clone(),
            model,
            session_id: SessionId::from_string(id.clone()),
            user_id: parent_config.user_id.clone(),
            sandbox_id: parent_config.sandbox_id.clone(),
            tool_ctx: parent_config.tool_ctx.clone(),
            manifest: child_manifest,
            subagent_manager: Some(child_mgr),
            hook_registry,
            cancel_token,
            approval_manager,
            // AY-D2: Inherit transcript writer from parent
            transcript_writer: parent_config.transcript_writer.clone(),
            // AY-D1: Inherit working directory (may be worktree path)
            working_dir: parent_config.working_dir.clone(),
            // Safety: inherit security pipeline
            safety_pipeline: parent_config.safety_pipeline.clone(),
            canary_token: parent_config.canary_token.clone(),
            // Observability: inherit recorder
            recorder: parent_config.recorder.clone(),
            // Guard: loop detection for sub-agents
            loop_guard: Some(super::loop_guard::LoopGuard::new()),
            ..AgentLoopConfig::default()
        };

        Ok(Self {
            id,
            agent_type,
            manifest,
            config: Some(config),
            task,
            manager,
            event_sender,
        })
    }

    /// Synchronous execution: wait for completion, forward events, return result.
    ///
    /// Consumes the runtime — cleanup is handled by the method itself.
    pub async fn run_sync(mut self) -> Result<SubAgentRuntimeResult> {
        let id = self.id.clone();
        let display_id = self
            .agent_type
            .clone()
            .unwrap_or_else(|| id.clone());
        let mgr = self.manager.clone();
        let event_sender = self.event_sender.clone();

        // Take ownership of config to pass to run_agent_loop
        let config = self.config.take().expect("SubAgentRuntime config already consumed");
        let messages = vec![ChatMessage::user(&self.task)];
        let mut stream = run_agent_loop(config, messages);
        let mut final_output = String::new();
        let mut rounds = 0u32;

        while let Some(event) = stream.next().await {
            match event {
                AgentEvent::Completed(result) => {
                    rounds = result.rounds;
                    final_output = Self::extract_text(&result.final_messages);
                    // Forward completion event
                    if let Some(ref sender) = event_sender {
                        let _ = sender.send(AgentEvent::SubAgentEvent {
                            source_id: display_id.clone(),
                            inner: Box::new(AgentEvent::Completed(result)),
                        });
                    }
                }
                AgentEvent::Error { message } => {
                    let _ = mgr.fail(&id, message.clone()).await;
                    if let Some(ref sender) = event_sender {
                        let _ = sender.send(AgentEvent::SubAgentEvent {
                            source_id: display_id.clone(),
                            inner: Box::new(AgentEvent::Error {
                                message: message.clone(),
                            }),
                        });
                    }
                    return Ok(SubAgentRuntimeResult {
                        id,
                        output: String::new(),
                        rounds,
                        status: SubAgentStatus::Failed(message),
                    });
                }
                AgentEvent::Done => {}
                other => {
                    // Forward intermediate events (TextDelta, ToolStart, etc.)
                    if let Some(ref sender) = event_sender {
                        let _ = sender.send(AgentEvent::SubAgentEvent {
                            source_id: display_id.clone(),
                            inner: Box::new(other),
                        });
                    }
                }
            }
        }

        if final_output.is_empty() {
            let _ = mgr.fail(&id, "No output".into()).await;
            Ok(SubAgentRuntimeResult {
                id,
                output: String::new(),
                rounds,
                status: SubAgentStatus::Failed("No output".into()),
            })
        } else {
            let _ = mgr.complete(&id, Some(final_output.clone())).await;
            Ok(SubAgentRuntimeResult {
                id,
                output: final_output,
                rounds,
                status: SubAgentStatus::Completed,
            })
        }
    }

    /// Asynchronous execution: fire-and-forget, returns session_id immediately.
    ///
    /// Consumes the runtime — cleanup is handled inside the spawned task.
    pub fn run_async(mut self) -> String {
        let id = self.id.clone();
        let mgr = self.manager.clone();
        let sa_id = id.clone();
        let task = self.task.clone();

        // Take ownership of config to move into the spawned task
        let config = self.config.take().expect("SubAgentRuntime config already consumed");
        tokio::spawn(async move {
            let messages = vec![ChatMessage::user(&task)];
            let mut stream = run_agent_loop(config, messages);
            let mut final_output = String::new();

            while let Some(event) = stream.next().await {
                if let AgentEvent::Completed(result) = event {
                    final_output = Self::extract_text(&result.final_messages);
                }
            }

            if final_output.is_empty() {
                let _ = mgr.fail(&sa_id, "No output produced".into()).await;
            } else {
                let _ = mgr.complete(&sa_id, Some(final_output)).await;
            }
        });

        id
    }

    /// Resolve tool set for the child agent.
    /// Priority: manifest (tool_filter + disallowed_tools) -> parent tools.
    fn resolve_tools(
        manifest: &Option<AgentManifest>,
        parent_config: &AgentLoopConfig,
    ) -> Option<Arc<ToolRegistry>> {
        let parent_tools = parent_config.tools.as_ref()?;

        if let Some(manifest) = manifest {
            // Step 1: Apply tool_filter (whitelist) -- empty = all
            let base = if manifest.tool_filter.is_empty() {
                parent_tools.snapshot()
            } else {
                parent_tools.snapshot_filtered(&manifest.tool_filter)
            };
            // Step 2: Apply disallowed_tools (blacklist)
            let filtered = if manifest.disallowed_tools.is_empty() {
                base
            } else {
                base.snapshot_excluded(&manifest.disallowed_tools)
            };
            Some(Arc::new(filtered))
        } else {
            parent_config.tools.clone()
        }
    }

    /// Extract text output from final messages.
    fn extract_text(messages: &[ChatMessage]) -> String {
        messages
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::Assistant)
            .and_then(|m| {
                m.content.iter().find_map(|c| {
                    if let ContentBlock::Text { text } = c {
                        Some(text.clone())
                    } else {
                        None
                    }
                })
            })
            .unwrap_or_default()
    }
}

impl Drop for SubAgentRuntime {
    fn drop(&mut self) {
        // If config was already taken (run_sync/run_async was called),
        // the run method handles cleanup. Only guard against premature drops.
        if self.config.is_none() {
            return;
        }
        // Runtime was dropped without running — cancel the registered agent.
        let mgr = self.manager.clone();
        let id = self.id.clone();
        tokio::spawn(async move {
            let agents = mgr.list().await;
            if let Some(h) = agents.iter().find(|a| a.id == id) {
                if h.status == SubAgentStatus::Running {
                    let _ = mgr.cancel(&id).await;
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::loop_config::AgentLoopConfig;
    use crate::agent::subagent::SubAgentManager;
    use crate::tools::ToolRegistry;

    fn make_registry() -> ToolRegistry {
        let mut reg = ToolRegistry::new();
        reg.register(crate::tools::sleep::SleepTool);
        reg.register(crate::tools::doctor::DoctorTool);
        reg.register(crate::tools::notifier::NotifierTool);
        reg
    }

    #[test]
    fn test_resolve_tools_with_manifest_blacklist() {
        let reg = make_registry();
        let config = AgentLoopConfig {
            tools: Some(Arc::new(reg)),
            ..Default::default()
        };
        let manifest = Some(AgentManifest {
            disallowed_tools: vec!["sleep".to_string()],
            ..Default::default()
        });
        let resolved = SubAgentRuntime::resolve_tools(&manifest, &config).unwrap();
        assert!(resolved.get("sleep").is_none());
        assert!(resolved.get("doctor").is_some());
    }

    #[test]
    fn test_resolve_tools_with_manifest_whitelist() {
        let reg = make_registry();
        let config = AgentLoopConfig {
            tools: Some(Arc::new(reg)),
            ..Default::default()
        };
        let manifest = Some(AgentManifest {
            tool_filter: vec!["sleep".to_string(), "doctor".to_string()],
            ..Default::default()
        });
        let resolved = SubAgentRuntime::resolve_tools(&manifest, &config).unwrap();
        assert!(resolved.get("sleep").is_some());
        assert!(resolved.get("doctor").is_some());
        assert!(resolved.get("notifier").is_none());
    }

    #[test]
    fn test_resolve_tools_no_manifest_uses_parent() {
        let reg = make_registry();
        let config = AgentLoopConfig {
            tools: Some(Arc::new(reg)),
            ..Default::default()
        };
        let resolved = SubAgentRuntime::resolve_tools(&None, &config).unwrap();
        assert_eq!(resolved.names().len(), 3);
    }

    #[tokio::test]
    async fn test_build_basic() {
        let reg = make_registry();
        let config = AgentLoopConfig {
            tools: Some(Arc::new(reg)),
            ..Default::default()
        };
        let mgr = Arc::new(SubAgentManager::new(4, 3));

        let rt = SubAgentRuntime::build(
            "test task".into(),
            None,
            &config,
            mgr.clone(),
            None,
            None,
        )
        .await
        .unwrap();

        assert!(rt.id.starts_with("sa-"));
        assert!(rt.agent_type.is_none());

        // Verify registered
        let agents = mgr.list().await;
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].status, SubAgentStatus::Running);
    }

    #[tokio::test]
    async fn test_build_with_manifest() {
        let reg = make_registry();
        let config = AgentLoopConfig {
            tools: Some(Arc::new(reg)),
            ..Default::default()
        };
        let mgr = Arc::new(SubAgentManager::new(4, 3));

        let manifest = AgentManifest {
            name: "test-agent".into(),
            system_prompt: Some("You are a test agent".into()),
            max_turns: Some(5),
            disallowed_tools: vec!["sleep".to_string()],
            ..Default::default()
        };

        let rt = SubAgentRuntime::build(
            "test task".into(),
            Some(manifest),
            &config,
            mgr.clone(),
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(rt.agent_type.as_deref(), Some("test-agent"));
        let config = rt.config.as_ref().unwrap();
        assert_eq!(config.max_iterations, 5);
        // Tools should exclude "sleep"
        let tools = config.tools.as_ref().unwrap();
        assert!(tools.get("sleep").is_none());
        assert!(tools.get("doctor").is_some());
    }

    #[tokio::test]
    async fn test_build_inherits_cancel_token() {
        let reg = make_registry();
        let config = AgentLoopConfig {
            tools: Some(Arc::new(reg)),
            ..Default::default()
        };
        let mgr = Arc::new(SubAgentManager::new(4, 3));

        let rt = SubAgentRuntime::build(
            "test".into(),
            None,
            &config,
            mgr,
            None,
            None,
        )
        .await
        .unwrap();

        // Child should have its own cancel token (not default)
        let child_config = rt.config.as_ref().unwrap();
        // Token should not be cancelled at build time
        assert!(!child_config.cancel_token.is_cancelled());
    }

    #[tokio::test]
    async fn test_build_with_permission_mode() {
        let reg = make_registry();
        let config = AgentLoopConfig {
            tools: Some(Arc::new(reg)),
            ..Default::default()
        };
        let mgr = Arc::new(SubAgentManager::new(4, 3));

        let manifest = AgentManifest {
            name: "auto-agent".into(),
            permission_mode: Some("auto".into()),
            ..Default::default()
        };

        let rt = SubAgentRuntime::build(
            "test".into(),
            Some(manifest),
            &config,
            mgr,
            None,
            None,
        )
        .await
        .unwrap();

        let child_config = rt.config.as_ref().unwrap();
        // Should have its own approval manager
        assert!(child_config.approval_manager.is_some());
    }

    #[tokio::test]
    async fn test_build_inherits_working_dir() {
        let reg = make_registry();
        let config = AgentLoopConfig {
            tools: Some(Arc::new(reg)),
            working_dir: Some(std::path::PathBuf::from("/test/worktree")),
            ..Default::default()
        };
        let mgr = Arc::new(SubAgentManager::new(4, 3));

        let rt = SubAgentRuntime::build(
            "test".into(),
            None,
            &config,
            mgr,
            None,
            None,
        )
        .await
        .unwrap();

        let child_config = rt.config.as_ref().unwrap();
        assert_eq!(
            child_config.working_dir.as_deref(),
            Some(std::path::Path::new("/test/worktree"))
        );
    }

    #[tokio::test]
    async fn test_build_depth_limit() {
        let config = AgentLoopConfig::default();
        // max_depth = 1 means no child can be spawned
        let mgr = Arc::new(SubAgentManager::new(4, 1));

        let result = SubAgentRuntime::build(
            "test".into(),
            None,
            &config,
            mgr,
            None,
            None,
        )
        .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("recursion depth limit"));
    }
}
