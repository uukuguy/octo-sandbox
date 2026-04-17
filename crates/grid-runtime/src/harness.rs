//! GridHarness — Tier 1 Harness implementation of RuntimeContract.
//!
//! Bridges grid-engine's AgentRuntime/AgentExecutor to the 16-method
//! EAASP v2.0 RuntimeContract. This is a zero-adapter implementation:
//! all calls are direct Rust function calls with no serialization.
//!
//! ## v2 SessionPayload mapping
//!
//! The v2 payload is a 5-block priority stack (P1 PolicyContext → P5
//! UserPreferences). GridHarness unpacks it as follows:
//! - `payload.user_id` or `payload.user_preferences.user_id` → engine `UserId`
//! - `payload.policy_context` (P1) → managed hooks (wired in a later phase)
//! - `payload.skill_instructions` (P4) → directly `load_skill` into engine
//! - `payload.memory_refs` (P3) → Phase 1 L2 Memory Engine projection

use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use tokio_stream::{Stream, StreamExt};
use tracing::{error, info, warn};

use eaasp_skill_registry::skill_parser::{substitute_hook_vars, HookVars};
use grid_engine::{
    AgentEvent, AgentMessage, AgentRuntime,
    McpServerConfigV2, McpTransport as EngineMcpTransport,
};
use grid_types::id::{SandboxId, SessionId, UserId};
use grid_types::{ChatMessage, ContentBlock, MessageRole};

use crate::contract::*;
use crate::telemetry::TelemetryCollector;

/// Grid Tier 1 Harness — native RuntimeContract implementation.
///
/// Wraps an `AgentRuntime` and exposes it through the 16-method contract.
/// Hooks, MCP, and skills are handled natively by grid-engine internals;
/// `on_tool_call`, `on_tool_result`, and `on_stop` are no-ops for Grid.
pub struct GridHarness {
    runtime: Arc<AgentRuntime>,
    runtime_id: String,
    telemetry: TelemetryCollector,
    /// LLM provider name (e.g. "anthropic", "openai") — read from RuntimeConfig.
    provider: String,
    /// LLM model name (e.g. "claude-sonnet-4-20250514") — read from RuntimeConfig.
    model: String,
}

impl GridHarness {
    /// Create a GridHarness wrapping an existing AgentRuntime.
    pub fn new(runtime: Arc<AgentRuntime>) -> Self {
        let runtime_id = "grid-harness".to_string();
        Self {
            runtime,
            telemetry: TelemetryCollector::new(&runtime_id),
            runtime_id,
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
        }
    }

    /// Create a GridHarness with a custom runtime ID.
    pub fn with_runtime_id(mut self, id: impl Into<String>) -> Self {
        self.runtime_id = id.into();
        self.telemetry = TelemetryCollector::new(&self.runtime_id);
        self
    }

    /// Set the LLM provider and model (from RuntimeConfig).
    pub fn with_provider(mut self, provider: impl Into<String>, model: impl Into<String>) -> Self {
        self.provider = provider.into();
        self.model = model.into();
        self
    }

    /// Access the underlying AgentRuntime.
    pub fn runtime(&self) -> &Arc<AgentRuntime> {
        &self.runtime
    }

    /// Convert an AgentEvent broadcast receiver into a ResponseChunk stream.
    ///
    /// The stream terminates after emitting a `Done` or `Completed` chunk,
    /// so the gRPC server-stream (and downstream SSE) closes cleanly.
    fn map_events_to_chunks(
        rx: tokio::sync::broadcast::Receiver<AgentEvent>,
    ) -> Pin<Box<dyn Stream<Item = ResponseChunk> + Send>> {
        // Use Arc<AtomicBool> to signal stream termination after "done" chunk.
        let terminated = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let terminated_clone = terminated.clone();
        let stream = tokio_stream::wrappers::BroadcastStream::new(rx)
            .filter_map(|result| match result {
                Ok(event) => Self::event_to_chunk(event),
                // Lag-fallback: if the gRPC consumer fell behind and the
                // broadcast channel dropped events, `AgentEvent::Done`
                // may have been among the dropped items — which would leave
                // the CLI hanging forever (4d9e0c6 precedent). We surface a
                // synthetic "error" chunk marking the lag; the take_while
                // below will emit it and then terminate on the next synthetic
                // "done" chunk. This is defense-in-depth — the primary fix
                // is a larger BROADCAST_CAPACITY in grid-engine.
                Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
                    tracing::warn!(
                        skipped = n,
                        "gRPC consumer lagged on AgentEvent broadcast — emitting synthetic done to terminate stream cleanly"
                    );
                    Some(ResponseChunk {
                        chunk_type: "done".into(),
                        content: format!(
                            "[grid-runtime] stream terminated early: consumer lag, {n} events dropped"
                        ),
                        tool_name: None,
                        tool_id: None,
                        is_error: true,
                    })
                }
            })
            .take_while(move |chunk| {
                if terminated_clone.load(std::sync::atomic::Ordering::Relaxed) {
                    return false;
                }
                if chunk.chunk_type == "done" {
                    // Emit this chunk, then terminate on the next poll.
                    terminated_clone.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                true
            });
        Box::pin(stream)
    }

    /// Build a system-prompt preamble from P3 memory_refs.
    ///
    /// Format:
    /// ```text
    /// ## Prior memories from previous sessions
    ///
    /// - [{memory_type}] {content}
    /// - [{memory_type}] {content}
    /// ```
    ///
    /// Returns an empty string when `memory_refs` is empty. Exposed at
    /// `pub(crate)` so D2 behavior can be covered by unit tests without
    /// spinning up a full AgentRuntime.
    pub(crate) fn build_memory_preamble(memory_refs: &[MemoryRef]) -> String {
        if memory_refs.is_empty() {
            return String::new();
        }
        let mut s = String::from("## Prior memories from previous sessions\n\n");
        for m in memory_refs {
            s.push_str(&format!("- [{}] {}\n", m.memory_type, m.content));
        }
        s
    }

    /// Parse `mcp:*` dependencies into McpServerConfig list.
    ///
    /// Convention: `mcp:<name>` maps to a stdio MCP server with
    /// `command = "uv run <name>"` (overridable via metadata keys
    /// `mcp.<name>.command` and `mcp.<name>.args`).
    /// Non-mcp dependencies are silently filtered out.
    fn resolve_mcp_dependencies(
        dependencies: &[String],
        metadata: &std::collections::HashMap<String, String>,
    ) -> Vec<McpServerConfig> {
        dependencies
            .iter()
            .filter(|d| d.starts_with("mcp:"))
            .map(|d| {
                let name = d.strip_prefix("mcp:").unwrap();
                // Resolve MCP server command from (in priority order):
                // 1. Skill metadata: mcp.<name>.command
                // 2. Default: <name> (bare executable name)
                let command_key = format!("mcp.{}.command", name);
                let command = metadata
                    .get(&command_key)
                    .cloned()
                    .unwrap_or_else(|| name.to_string());

                // Check for explicit args override: mcp.<name>.args
                let args_key = format!("mcp.{}.args", name);
                let args: Vec<String> = metadata
                    .get(&args_key)
                    .map(|a| a.split_whitespace().map(String::from).collect())
                    .unwrap_or_default();

                McpServerConfig {
                    name: name.to_string(),
                    transport: "stdio".to_string(),
                    command: if command.is_empty() { None } else { Some(command) },
                    args,
                    env: std::collections::HashMap::new(),
                    url: None,
                }
            })
            .collect()
    }

    /// Register skill-frontmatter scoped hooks into the engine's HookRegistry.
    ///
    /// Each `ScopedHook` from P4 SkillInstructions is wrapped in a
    /// `ScopedHookHandler` and registered at the corresponding `HookPoint`.
    ///
    /// S3.T5 (G1): `hook_vars` resolves `${SKILL_DIR}` / `${SESSION_DIR}` /
    /// `${RUNTIME_DIR}` in each hook's `action` body before registration.
    /// Per ADR-V2-006 §7 (fail-open), substitution failures for a single hook
    /// are logged at ERROR and that hook is skipped — other hooks still
    /// register normally, and Initialize never fails on hook substitution
    /// errors (hook errors must never block a session).
    async fn register_scoped_hooks(
        &self,
        session_id: &SessionId,
        hooks: &[ScopedHook],
        hook_vars: HookVars,
    ) {
        use crate::scoped_hook_handler::{ScopedHookHandler, ScopedStopHookBridge};
        use grid_engine::agent::StopHook;
        use grid_engine::hooks::HookPoint;

        let registry = self.runtime.hook_registry();

        // S3.T5 (G7): accumulate bridges for Stop-scope hooks; they must be
        // forwarded to AgentRuntime via `register_session_stop_hooks` so
        // `build_and_spawn_executor_filtered` picks them up when the
        // executor spawns. Stop hooks cannot ride the general-purpose
        // `HookRegistry` (typed StopHookDecision has no HookAction match);
        // this Vec carries them out of the loop instead.
        let mut stop_bridges: Vec<Arc<dyn StopHook>> = Vec::new();

        for hook in hooks {
            // Proto mapping: condition = scope (PreToolUse/PostToolUse/Stop),
            //                hook_type = execution type (command/prompt).
            // Determine HookPoint from condition (scope), not hook_type.
            let scope_str = if !hook.condition.is_empty() {
                hook.condition.as_str()
            } else {
                hook.hook_type.as_str()
            };
            let hook_point = match scope_str {
                "pre_tool_call" | "PreToolUse" => HookPoint::PreToolUse,
                "post_tool_result" | "PostToolUse" => HookPoint::PostToolUse,
                "stop" | "Stop" => HookPoint::Stop,
                other => {
                    warn!(scope = %other, "Unknown scoped hook scope, skipping");
                    continue;
                }
            };

            // S3.T5 (G1): resolve ${SKILL_DIR} / ${SESSION_DIR} / ${RUNTIME_DIR}
            // before the command reaches the subprocess. Substitution errors
            // (Unbound / Unknown / Malformed) are per-hook failures: log loudly
            // and skip this hook so other hooks still fire. Per ADR-V2-006 §7,
            // a hook substitution error NEVER aborts the session.
            let resolved_command = match substitute_hook_vars(&hook.action, &hook_vars) {
                Ok(cmd) => cmd,
                Err(e) => {
                    error!(
                        hook_id = %hook.hook_id,
                        error_kind = ?std::mem::discriminant(&e),
                        error_msg = %e,
                        "Scoped hook substitution failed, skipping this hook"
                    );
                    continue;
                }
            };

            // S3.T5 (G7): Stop-scope hooks take a separate path.
            // `HookRegistry` dispatches via `HookAction`, which cannot
            // express `StopHookDecision::InjectAndContinue`. Wrap in the
            // typed `StopHook` trait instead and stage for forwarding
            // into the executor via `AgentRuntime::register_session_stop_hooks`.
            if matches!(hook_point, HookPoint::Stop) {
                let bridge = Arc::new(ScopedStopHookBridge::new(
                    hook.hook_id.clone(),
                    resolved_command.clone(),
                ));
                stop_bridges.push(bridge as Arc<dyn StopHook>);
                info!(
                    hook_id = %hook.hook_id,
                    hook_type = %hook.hook_type,
                    "Scoped Stop hook bridged to StopHook trait"
                );
                continue;
            }

            let handler = ScopedHookHandler::new(
                hook.hook_id.clone(),
                resolved_command,
                hook.condition.clone(),
                hook.precedence,
            );

            registry
                .register(hook_point, Arc::new(handler))
                .await;
            info!(
                hook_id = %hook.hook_id,
                hook_type = %hook.hook_type,
                condition = %hook.condition,
                "Scoped hook registered"
            );
        }

        // S3.T5 (G7): hand the accumulated bridges to AgentRuntime so
        // they land in the AgentLoopConfig when the executor is built.
        // Must happen BEFORE `start_session_*` (the executor spawn drains
        // the stash). Empty stash is a no-op.
        if !stop_bridges.is_empty() {
            let count = stop_bridges.len();
            self.runtime
                .register_session_stop_hooks(session_id, stop_bridges);
            info!(
                session_id = %session_id,
                count = count,
                "Staged scoped Stop-hook bridges for session"
            );
        }
    }

    /// Build `HookVars` for a session at Initialize time.
    ///
    /// S3.T5 (G1) — variable resolution for scoped-hook command bodies.
    ///
    /// `skill_dir` resolution priority:
    /// 1. If `payload.skill_instructions.content` is non-empty AND there is
    ///    at least one scoped hook declared, materialize SKILL.md under
    ///    `{runtime_workspace}/grid-session-{session_id}/skill/` and use
    ///    that directory.
    /// 2. Else if `EAASP_SKILL_CACHE_DIR` env var is set AND
    ///    `{cache}/{skill_id}` exists on disk, use that.
    /// 3. Else `None`. `substitute_hook_vars` will raise `Unbound` on any
    ///    hook containing `${SKILL_DIR}` and that hook is skipped at
    ///    registration (fail-open per hook, never per session).
    ///
    /// `session_dir` is always created (mirrors claude-code-runtime's
    /// `{workspace}/{session_id}` convention) so hooks can always resolve
    /// `${SESSION_DIR}`. `runtime_dir` is optional (platform-level mount
    /// point supplied via `EAASP_RUNTIME_DIR`).
    ///
    /// Returns an `anyhow::Result` because I/O failures at Initialize
    /// (e.g. unwritable workspace) SHOULD fail the session — that is a
    /// distinct failure mode from hook-substitution errors.
    fn build_hook_vars(
        &self,
        session_id: &str,
        skill_instructions: Option<&SkillInstructions>,
    ) -> anyhow::Result<HookVars> {
        use std::path::PathBuf;

        // SESSION_DIR — real directory under runtime workspace. Mirrors
        // claude-code-runtime's per-session layout so cross-runtime hook
        // authors see the same path shape.
        let session_dir = {
            let base = std::env::var("EAASP_RUNTIME_WORKSPACE")
                .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().into_owned());
            let path = PathBuf::from(base).join(format!("grid-session-{}", session_id));
            std::fs::create_dir_all(&path)?;
            path.to_string_lossy().into_owned()
        };

        // RUNTIME_DIR — optional, sourced from env. Empty string treated as
        // unset so ops can "unset" without removing the var.
        let runtime_dir = std::env::var("EAASP_RUNTIME_DIR")
            .ok()
            .filter(|s| !s.is_empty());

        // SKILL_DIR resolution — try inline materialization first, then the
        // on-disk cache fallback, else leave it unset.
        let skill_dir = match skill_instructions {
            Some(si) if !si.content.is_empty() && !si.frontmatter_hooks.is_empty() => {
                // Materialize inline skill content to {session_dir}/skill/
                // so hooks referencing ${SKILL_DIR}/hooks/*.sh resolve to
                // real executable files.
                //
                // Write SKILL.md from inline content, then copy the hooks/
                // directory from the on-disk skill source (the authoring
                // repository). Priority:
                //   1. EAASP_SKILL_SOURCE_DIR env var (e.g. examples/skills)
                //   2. {CARGO_WORKSPACE_ROOT}/examples/skills
                // If neither is found, the skill runs without materialized
                // hooks and ${SKILL_DIR}/hooks/*.sh paths will fail at exec
                // time — which is better than silently ignoring them (the
                // previous D118 behavior that leaked into Phase 2.5 S4.T2).
                let dir = PathBuf::from(&session_dir).join("skill");
                std::fs::create_dir_all(&dir)?;
                let skill_md = dir.join("SKILL.md");
                std::fs::write(&skill_md, &si.content)?;

                // Locate source hooks dir.
                let source_root = std::env::var("EAASP_SKILL_SOURCE_DIR")
                    .ok()
                    .filter(|s| !s.is_empty())
                    .map(PathBuf::from)
                    .or_else(|| {
                        // Fallback to CWD/examples/skills; `make dev-eaasp`
                        // launches grid-runtime from the repo root so this
                        // is a safe bet for dev.
                        let cwd = std::env::current_dir().ok()?;
                        let p = cwd.join("examples").join("skills");
                        if p.exists() { Some(p) } else { None }
                    });

                if let Some(root) = source_root {
                    let src_hooks = root.join(&si.skill_id).join("hooks");
                    if src_hooks.is_dir() {
                        let dst_hooks = dir.join("hooks");
                        std::fs::create_dir_all(&dst_hooks)?;
                        for entry in std::fs::read_dir(&src_hooks)? {
                            let entry = entry?;
                            let src = entry.path();
                            if src.is_file() {
                                let dst = dst_hooks.join(entry.file_name());
                                std::fs::copy(&src, &dst)?;
                                // Preserve exec bit so bash can run them.
                                #[cfg(unix)]
                                {
                                    use std::os::unix::fs::PermissionsExt;
                                    let mut perms = std::fs::metadata(&dst)?.permissions();
                                    perms.set_mode(0o755);
                                    std::fs::set_permissions(&dst, perms)?;
                                }
                            }
                        }
                        info!(
                            skill_id = %si.skill_id,
                            src = %src_hooks.display(),
                            dst = %dst_hooks.display(),
                            "Materialized skill hooks/ directory"
                        );
                    } else {
                        warn!(
                            skill_id = %si.skill_id,
                            expected = %src_hooks.display(),
                            "Skill declares scoped_hooks but source hooks/ dir not found — hook scripts will fail at exec"
                        );
                    }
                } else {
                    warn!(
                        skill_id = %si.skill_id,
                        "EAASP_SKILL_SOURCE_DIR unset and ./examples/skills not found — skill hook scripts unavailable"
                    );
                }

                Some(dir.to_string_lossy().into_owned())
            }
            Some(si) if !si.skill_id.is_empty() => {
                // Cache fallback — only adopt if the directory actually
                // exists on disk (prevents dangling unresolved paths).
                std::env::var("EAASP_SKILL_CACHE_DIR")
                    .ok()
                    .filter(|s| !s.is_empty())
                    .and_then(|root| {
                        let p = PathBuf::from(root).join(&si.skill_id);
                        if p.exists() {
                            Some(p.to_string_lossy().into_owned())
                        } else {
                            None
                        }
                    })
            }
            _ => None,
        };

        Ok(HookVars {
            skill_dir,
            session_dir: Some(session_dir),
            runtime_dir,
        })
    }

    /// Map a single AgentEvent to an optional ResponseChunk.
    fn event_to_chunk(event: AgentEvent) -> Option<ResponseChunk> {
        match event {
            AgentEvent::TextDelta { text } => Some(ResponseChunk {
                chunk_type: "text_delta".into(),
                content: text,
                tool_name: None,
                tool_id: None,
                is_error: false,
            }),
            AgentEvent::TextComplete { text } => Some(ResponseChunk {
                chunk_type: "text_delta".into(),
                content: text,
                tool_name: None,
                tool_id: None,
                is_error: false,
            }),
            AgentEvent::ThinkingDelta { text } => Some(ResponseChunk {
                chunk_type: "thinking".into(),
                content: text,
                tool_name: None,
                tool_id: None,
                is_error: false,
            }),
            AgentEvent::ToolStart { tool_id, tool_name, input } => Some(ResponseChunk {
                chunk_type: "tool_start".into(),
                content: serde_json::to_string(&input).unwrap_or_default(),
                tool_name: Some(tool_name),
                tool_id: Some(tool_id),
                is_error: false,
            }),
            AgentEvent::ToolResult { tool_id, tool_name, output, success } => Some(ResponseChunk {
                chunk_type: "tool_result".into(),
                content: output,
                // D83 (S1.T4): populate tool_name so POST_TOOL_USE hooks /
                // CLI telemetry can tag results without a side-channel lookup.
                tool_name: Some(tool_name),
                tool_id: Some(tool_id),
                is_error: !success,
            }),
            AgentEvent::Error { message } => Some(ResponseChunk {
                chunk_type: "error".into(),
                content: message,
                tool_name: None,
                tool_id: None,
                is_error: true,
            }),
            // D85 (S1.T5): the `Completed(AgentLoopResult)` event carries
            // the full final_messages history; extract the last assistant
            // message's concatenated text and surface it in the "done" chunk
            // so downstream L4 STOP consumers (and tests/telemetry) can read
            // the final response without re-walking the event stream.
            // `Done` has no payload — treat it as an empty-body done chunk.
            AgentEvent::Done => Some(ResponseChunk {
                chunk_type: "done".into(),
                content: String::new(),
                tool_name: None,
                tool_id: None,
                is_error: false,
            }),
            AgentEvent::Completed(result) => {
                // Walk final_messages in reverse to find the last assistant
                // turn; concatenate every Text block (tool-use/tool-result
                // blocks are skipped by `ChatMessage::text_content`). Empty
                // string is the correct default when no assistant text is
                // present (e.g. a tool-only turn that never terminated with
                // plain text). Never panic.
                let content = result
                    .final_messages
                    .iter()
                    .rev()
                    .find(|m| m.role == MessageRole::Assistant)
                    .map(|m| m.text_content())
                    .unwrap_or_default();
                Some(ResponseChunk {
                    chunk_type: "done".into(),
                    content,
                    tool_name: None,
                    tool_id: None,
                    is_error: false,
                })
            }
            // D87: surface workflow-continuation injections so they are
            // ingestable as `WORKFLOW_CONTINUATION` events by L3/L4. Without
            // this, the fix silently fires but downstream tooling can't
            // observe it, making E2E debugging painful.
            AgentEvent::WorkflowContinuation { attempt, max_attempts } => Some(ResponseChunk {
                chunk_type: "workflow_continuation".into(),
                content: format!("attempt {attempt}/{max_attempts}"),
                tool_name: None,
                tool_id: None,
                is_error: false,
            }),
            // Other events are internal — not exposed through contract
            _ => None,
        }
    }
}

#[async_trait]
impl RuntimeContract for GridHarness {
    async fn initialize(&self, payload: SessionPayload) -> anyhow::Result<SessionHandle> {
        let session_id = SessionId::new();

        // Resolve user_id from v2 priority blocks, preferring the
        // top-level session metadata, then P5 UserPreferences.
        let user_id_str = if !payload.user_id.is_empty() {
            payload.user_id.clone()
        } else {
            payload
                .user_preferences
                .as_ref()
                .map(|u| u.user_id.clone())
                .unwrap_or_else(|| "anonymous".into())
        };
        let user_id = UserId::from_string(&user_id_str);
        let sandbox_id = SandboxId::from_string("default");

        // D1 — read P1 PolicyContext metadata (best-effort, read-only).
        // Hook installation/execution is deferred to Phase 2 (D50/D53).
        let org_unit = payload
            .policy_context
            .as_ref()
            .map(|p| p.org_unit.clone())
            .unwrap_or_default();
        let policy_version = payload
            .policy_context
            .as_ref()
            .map(|p| p.policy_version.clone())
            .unwrap_or_default();
        let hooks_count = payload
            .policy_context
            .as_ref()
            .map(|p| p.hooks.len())
            .unwrap_or(0);

        info!(
            session_id = %session_id,
            org_unit = %org_unit,
            policy_version = %policy_version,
            hooks_count = hooks_count,
            "GridHarness: policy_context metadata (D1)"
        );

        // Build initial_history with System messages:
        // 1. P4 Skill prose (workflow instructions for the agent)
        // 2. P3 Memory refs preamble (prior session context)
        let mut initial_history: Vec<ChatMessage> = Vec::new();

        // P4 — inject skill prose as system prompt so agent knows its task.
        let skill_prose = payload
            .skill_instructions
            .as_ref()
            .map(|s| s.content.clone())
            .unwrap_or_default();
        if !skill_prose.is_empty() {
            initial_history.push(ChatMessage {
                role: MessageRole::System,
                content: vec![ContentBlock::Text { text: skill_prose }],
            });
        }

        // D2 — P3 memory_refs preamble.
        if !payload.memory_refs.is_empty() {
            let preamble = Self::build_memory_preamble(&payload.memory_refs);
            initial_history.push(ChatMessage {
                role: MessageRole::System,
                content: vec![ContentBlock::Text { text: preamble }],
            });
        }
        let memory_refs_count = payload.memory_refs.len();

        // Extract MCP dependencies and scoped hooks from P4 BEFORE start_session.
        // start_session snapshots the ToolRegistry — MCP tools must be registered first.
        let skill_mcp_deps: Vec<McpServerConfig> = payload
            .skill_instructions
            .as_ref()
            .map(|skill| Self::resolve_mcp_dependencies(&skill.dependencies, &skill.metadata))
            .unwrap_or_default();

        let scoped_hooks: Vec<ScopedHook> = payload
            .skill_instructions
            .as_ref()
            .map(|s| s.frontmatter_hooks.clone())
            .unwrap_or_default();

        // Connect MCP servers BEFORE start_session so tools are in the snapshot.
        let handle_placeholder = SessionHandle {
            session_id: session_id.as_str().to_string(),
        };
        let mcp_to_connect: Vec<_> = skill_mcp_deps
            .into_iter()
            .filter(|s| s.command.as_ref().map_or(false, |c| !c.is_empty()))
            .collect();
        if !mcp_to_connect.is_empty() {
            info!(count = mcp_to_connect.len(), "Connecting MCP servers from skill dependencies");
            if let Err(e) = self.connect_mcp(&handle_placeholder, mcp_to_connect).await {
                warn!(error = %e, "Failed to connect MCP servers from skill dependencies");
            }
        }

        // S3.T5 (G1): resolve ${SKILL_DIR} / ${SESSION_DIR} / ${RUNTIME_DIR}
        // so scoped hook bodies can reference absolute paths instead of
        // literal placeholders. Materialize inline skill content to per-session
        // workspace when present; fall back to EAASP_SKILL_CACHE_DIR/{skill_id}
        // when set; otherwise skill_dir=None and substitute_hook_vars will
        // refuse to expand ${SKILL_DIR} — the affected hook is skipped at
        // registration with a loud ERROR log (other hooks still register).
        let hook_vars = self.build_hook_vars(
            session_id.as_str(),
            payload.skill_instructions.as_ref(),
        )?;

        info!(
            session_id = %session_id,
            skill_dir = ?hook_vars.skill_dir,
            session_dir = ?hook_vars.session_dir,
            runtime_dir = ?hook_vars.runtime_dir,
            "GridHarness: hook_vars resolved (S3.T5 G1)"
        );

        // Register scoped hooks into global HookRegistry (visible to all sessions).
        // S3.T5 (G7): pass `session_id` so Stop-scope hooks can be staged
        // in `AgentRuntime.session_stop_hooks` and drained into the executor
        // when it spawns — Stop hooks require a typed dispatch path distinct
        // from the generic HookRegistry (see `register_scoped_hooks`).
        if !scoped_hooks.is_empty() {
            self.register_scoped_hooks(&session_id, &scoped_hooks, hook_vars).await;
        }

        // Tool filter (restored from commit 055badf design):
        // When EAASP_TOOL_FILTER=on, restrict the agent's ToolRegistry to
        // only the MCP tools provided by the current skill's MCP
        // dependencies. This prevents the LLM from choosing grid-engine
        // built-ins (memory_recall/timeline/store/edit, bash, graph_*, ...)
        // that are not part of the skill's declared workflow.
        //
        // Deferred D145/D146: grid-engine vs L2 MCP tool coexistence is an
        // EAASP architecture issue — to be re-designed in Phase 3.
        let tool_filter: Option<Vec<String>> = {
            let filter_enabled = std::env::var("EAASP_TOOL_FILTER")
                .map(|v| v == "on" || v == "true" || v == "1")
                .unwrap_or(false);
            if filter_enabled {
                let mcp_guard = self.runtime.mcp_manager().lock().await;
                let mcp_tools = mcp_guard.tool_names();
                if mcp_tools.is_empty() {
                    None
                } else {
                    Some(mcp_tools)
                }
            } else {
                None
            }
        };

        info!(
            session_id = %session_id,
            tool_filter = ?tool_filter,
            "GridHarness: tool_filter derived from EAASP_TOOL_FILTER + MCP deps"
        );

        // NOW start session — ToolRegistry snapshot will include MCP tools.
        let _handle = self
            .runtime
            .start_session_with_tool_filter(
                session_id.clone(),
                user_id,
                sandbox_id,
                initial_history,
                None, // no agent_id override
                tool_filter.as_deref(),
            )
            .await
            .map_err(|e| anyhow::anyhow!("Failed to start session: {}", e))?;

        // Register PostToolUse memory write hook (fire-and-forget, FailOpen).
        let l2_mem_client = crate::l2_memory_client::L2MemoryClient::from_env();
        let memory_hook = crate::memory_write_hook::MemoryWriteHook::new(
            l2_mem_client,
            session_id.as_str().to_string(),
        );
        self.runtime
            .hook_registry()
            .register(
                grid_engine::hooks::HookPoint::PostToolUse,
                std::sync::Arc::new(memory_hook),
            )
            .await;

        info!(
            session_id = %session_id,
            user = %user_id_str,
            org_unit = %org_unit,
            memory_refs_count = memory_refs_count,
            "GridHarness: session initialized (v2)"
        );

        let handle = SessionHandle {
            session_id: session_id.as_str().to_string(),
        };

        // If the payload arrived with an inline P4 SkillInstructions block,
        // hand it off to load_skill for metadata logging.
        if let Some(skill) = payload.skill_instructions {
            let content = SkillContent {
                skill_id: skill.skill_id,
                name: skill.name,
                frontmatter_yaml: serde_json::to_string(&skill.frontmatter_hooks)
                    .unwrap_or_default(),
                prose: skill.content,
                required_tools: skill.required_tools,
            };
            if let Err(e) = self.load_skill(&handle, content).await {
                warn!(error = %e, "Failed to load inline P4 skill instructions");
            }
        }

        Ok(handle)
    }

    async fn send(
        &self,
        handle: &SessionHandle,
        message: UserMessage,
    ) -> anyhow::Result<Pin<Box<dyn Stream<Item = ResponseChunk> + Send>>> {
        let session_id = SessionId::from_string(&handle.session_id);

        let executor_handle = self
            .runtime
            .get_session_handle(&session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", handle.session_id))?;

        // Subscribe to events before sending to avoid missing early events
        let rx = executor_handle.subscribe();

        // Send user message
        executor_handle
            .send(AgentMessage::UserMessage {
                content: message.content,
                channel_id: "eaasp".to_string(),
            })
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send message: {}", e))?;

        Ok(Self::map_events_to_chunks(rx))
    }

    async fn load_skill(
        &self,
        handle: &SessionHandle,
        content: SkillContent,
    ) -> anyhow::Result<()> {
        // Skill prose is already injected as System message in initialize().
        // This method handles additional skill metadata registration.
        info!(
            session_id = %handle.session_id,
            skill = %content.name,
            prose_len = content.prose.len(),
            required_tools_count = content.required_tools.len(),
            "GridHarness: load_skill — prose injected via initial_history"
        );

        // D87 L1 metadata: forward required_tools into the running executor
        // so the harness can drive `tool_choice=Specific(next_required)`
        // on workflow-continuation triggers. Empty list = no L1 constraint.
        if !content.required_tools.is_empty() {
            let session_id = SessionId::from_string(&handle.session_id);
            if let Some(executor_handle) = self.runtime.get_session_handle(&session_id) {
                if let Err(e) = executor_handle
                    .set_required_tools(content.required_tools.clone())
                    .await
                {
                    warn!(
                        error = %e,
                        skill = %content.name,
                        "Failed to forward required_tools to executor"
                    );
                }
            }
        }

        Ok(())
    }

    async fn on_tool_call(
        &self,
        _handle: &SessionHandle,
        _call: ToolCall,
    ) -> anyhow::Result<HookDecision> {
        // No-op for Grid: PreToolUse hooks fire natively inside AgentLoop.
        Ok(HookDecision::Allow)
    }

    async fn on_tool_result(
        &self,
        _handle: &SessionHandle,
        _result: ToolResult,
    ) -> anyhow::Result<HookDecision> {
        // No-op for Grid: PostToolUse hooks fire natively inside AgentLoop.
        Ok(HookDecision::Allow)
    }

    async fn on_stop(&self, _handle: &SessionHandle) -> anyhow::Result<StopDecision> {
        // No-op for Grid: Stop hooks fire natively inside AgentLoop.
        Ok(StopDecision::Complete)
    }

    async fn get_state(&self, handle: &SessionHandle) -> anyhow::Result<SessionState> {
        let session_id = SessionId::from_string(&handle.session_id);

        // Retrieve messages from session store
        let messages = self
            .runtime
            .session_store()
            .get_messages(&session_id)
            .await;

        // Serialize messages as state
        let state_data = match messages {
            Some(msgs) => serde_json::to_vec(&msgs)
                .map_err(|e| anyhow::anyhow!("Failed to serialize session state: {}", e))?,
            None => Vec::new(),
        };

        Ok(SessionState {
            session_id: handle.session_id.clone(),
            runtime_id: self.runtime_id.clone(),
            state_data,
            created_at: chrono::Utc::now(),
            state_format: "rust-serde-v2".into(),
        })
    }

    async fn restore_state(&self, state: SessionState) -> anyhow::Result<SessionHandle> {
        if state.state_data.is_empty() {
            return Err(anyhow::anyhow!("Empty state data"));
        }

        let messages: Vec<grid_types::ChatMessage> = serde_json::from_slice(&state.state_data)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize session state: {}", e))?;

        let session_id = SessionId::from_string(&state.session_id);
        let user_id = UserId::from_string("restored");
        let sandbox_id = SandboxId::from_string("default");

        let _handle = self
            .runtime
            .start_session(
                session_id.clone(),
                user_id,
                sandbox_id,
                messages,
                None,
            )
            .await
            .map_err(|e| anyhow::anyhow!("Failed to restore session: {}", e))?;

        info!(session_id = %state.session_id, "GridHarness: session restored");

        Ok(SessionHandle {
            session_id: state.session_id,
        })
    }

    async fn connect_mcp(
        &self,
        handle: &SessionHandle,
        servers: Vec<McpServerConfig>,
    ) -> anyhow::Result<()> {
        for server in &servers {
            let transport = match server.transport.as_str() {
                "stdio" => EngineMcpTransport::Stdio,
                "sse" | "http" | "streamable-http" => EngineMcpTransport::Sse,
                other => {
                    warn!(server = %server.name, transport = %other, "Unknown MCP transport, defaulting to stdio");
                    EngineMcpTransport::Stdio
                }
            };

            let config = McpServerConfigV2 {
                id: format!("eaasp-{}", server.name),
                name: server.name.clone(),
                source: "eaasp-platform".to_string(),
                command: server.command.clone().unwrap_or_default(),
                args: server.args.clone(),
                env: server.env.clone(),
                enabled: true,
                transport,
                url: server.url.clone(),
                oauth: None,
            };

            // Use AgentRuntime.add_mcp_server — it connects the MCP server AND
            // registers tools into the global ToolRegistry. Previously we called
            // McpManager.add_server which only connects but doesn't register tools.
            match self.runtime.add_mcp_server(config.into()).await {
                Ok(tools) => {
                    info!(
                        server = %server.name,
                        tool_count = tools.len(),
                        "MCP server connected and tools registered"
                    );
                }
                Err(e) => {
                    warn!(server = %server.name, error = %e, "Failed to add MCP server");
                }
            }
        }

        info!(
            session_id = %handle.session_id,
            count = servers.len(),
            "GridHarness: MCP servers connected"
        );
        Ok(())
    }

    async fn emit_telemetry(
        &self,
        handle: &SessionHandle,
    ) -> anyhow::Result<Vec<TelemetryEvent>> {
        Ok(self.telemetry.collect(&handle.session_id, &self.runtime).await)
    }

    fn get_capabilities(&self) -> CapabilityManifest {
        let tool_names: Vec<String> = {
            let tools = self.runtime.tools().lock().unwrap();
            tools.names()
        };

        CapabilityManifest {
            runtime_id: self.runtime_id.clone(),
            runtime_name: "Grid".into(),
            tier: RuntimeTier::Harness,
            model: self.model.clone(),
            context_window: 200_000,
            supported_tools: tool_names,
            native_hooks: true,
            native_mcp: true,
            native_skills: true,
            cost: Some(CostEstimate {
                input_cost_per_1k: 0.003,
                output_cost_per_1k: 0.015,
            }),
            metadata: Default::default(),
            requires_hook_bridge: false,
            deployment_mode: DeploymentMode::Shared,
        }
    }

    async fn terminate(&self, handle: &SessionHandle) -> anyhow::Result<()> {
        let session_id = SessionId::from_string(&handle.session_id);
        self.runtime.stop_session(&session_id).await;

        info!(session_id = %handle.session_id, "GridHarness: session terminated");
        Ok(())
    }

    async fn disconnect_mcp(
        &self,
        handle: &SessionHandle,
        server_name: &str,
    ) -> anyhow::Result<()> {
        let mcp_manager = self.runtime.mcp_manager();
        let mut mcp_guard = mcp_manager.lock().await;
        let _ = mcp_guard.remove_server(server_name).await;
        info!(
            session_id = %handle.session_id,
            server = %server_name,
            "GridHarness: MCP server disconnected"
        );
        Ok(())
    }

    async fn pause_session(&self, handle: &SessionHandle) -> anyhow::Result<()> {
        let session_id = SessionId::from_string(&handle.session_id);
        self.runtime.stop_session(&session_id).await;
        info!(session_id = %handle.session_id, "GridHarness: session paused");
        Ok(())
    }

    async fn resume_session(&self, session_id: &str) -> anyhow::Result<SessionHandle> {
        warn!(session_id = %session_id, "GridHarness: resume_session stub — use restore_state with persisted state");
        Err(anyhow::anyhow!("resume_session requires state from L4 session store; use restore_state instead"))
    }

    async fn health(&self) -> anyhow::Result<HealthStatus> {
        let mut checks = std::collections::HashMap::new();

        // Check MCP manager
        {
            let mcp_guard = self.runtime.mcp_manager().lock().await;
            let server_count = mcp_guard.server_count();
            checks.insert("mcp".into(), format!("ok ({} servers)", server_count));
        }

        // Check session count
        let session_count = self.runtime.active_session_count();
        checks.insert("sessions".into(), format!("ok ({} active)", session_count));

        // Check event bus
        let has_event_bus = self.runtime.event_bus().is_some();
        checks.insert(
            "telemetry".into(),
            if has_event_bus { "ok" } else { "disabled" }.into(),
        );

        Ok(HealthStatus {
            healthy: true,
            runtime_id: self.runtime_id.clone(),
            checks,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_to_chunk_text_delta() {
        let event = AgentEvent::TextDelta { text: "hello".into() };
        let chunk = GridHarness::event_to_chunk(event).unwrap();
        assert_eq!(chunk.chunk_type, "text_delta");
        assert_eq!(chunk.content, "hello");
        assert!(!chunk.is_error);
    }

    #[test]
    fn event_to_chunk_tool_start() {
        let event = AgentEvent::ToolStart {
            tool_id: "t1".into(),
            tool_name: "bash".into(),
            input: serde_json::json!({"command": "ls"}),
        };
        let chunk = GridHarness::event_to_chunk(event).unwrap();
        assert_eq!(chunk.chunk_type, "tool_start");
        assert_eq!(chunk.tool_name.as_deref(), Some("bash"));
        assert_eq!(chunk.tool_id.as_deref(), Some("t1"));
    }

    #[test]
    fn event_to_chunk_tool_result() {
        let event = AgentEvent::ToolResult {
            tool_id: "t1".into(),
            tool_name: "bash".into(),
            output: "file1.rs".into(),
            success: true,
        };
        let chunk = GridHarness::event_to_chunk(event).unwrap();
        assert_eq!(chunk.chunk_type, "tool_result");
        assert!(!chunk.is_error);
        // D83 (S1.T4): tool_name MUST propagate from AgentEvent into the
        // wire-level ResponseChunk so downstream POST_TOOL_USE hooks can
        // correlate results to tool calls without a side-channel lookup.
        assert_eq!(chunk.tool_name.as_deref(), Some("bash"));
        assert_eq!(chunk.tool_id.as_deref(), Some("t1"));
    }

    #[test]
    fn event_to_chunk_error() {
        let event = AgentEvent::Error { message: "boom".into() };
        let chunk = GridHarness::event_to_chunk(event).unwrap();
        assert_eq!(chunk.chunk_type, "error");
        assert!(chunk.is_error);
    }

    #[test]
    fn event_to_chunk_done() {
        let chunk = GridHarness::event_to_chunk(AgentEvent::Done).unwrap();
        assert_eq!(chunk.chunk_type, "done");
        // D85 — `Done` has no payload, so content stays empty.
        assert_eq!(chunk.content, "");
    }

    #[test]
    fn event_to_chunk_completed_populates_final_assistant_text() {
        // D85 (S1.T5) — `AgentEvent::Completed(result)` carries the full
        // conversation history. The "done" ResponseChunk must surface the
        // LAST assistant message's text so downstream L4 STOP consumers
        // (and telemetry) can read the final reply off the chunk stream.
        use grid_engine::AgentLoopResult;

        let result = AgentLoopResult {
            final_messages: vec![
                ChatMessage {
                    role: MessageRole::User,
                    content: vec![ContentBlock::Text {
                        text: "what is 2+2?".into(),
                    }],
                },
                ChatMessage {
                    role: MessageRole::Assistant,
                    // Two Text blocks must concatenate cleanly (no separator).
                    content: vec![
                        ContentBlock::Text { text: "hello".into() },
                        ContentBlock::Text { text: " world".into() },
                    ],
                },
            ],
            ..Default::default()
        };

        let chunk = GridHarness::event_to_chunk(AgentEvent::Completed(result)).unwrap();
        assert_eq!(chunk.chunk_type, "done");
        assert_eq!(chunk.content, "hello world");
        assert!(!chunk.is_error);
        assert!(chunk.tool_name.is_none());
        assert!(chunk.tool_id.is_none());
    }

    #[test]
    fn event_to_chunk_completed_no_assistant_message_returns_empty_content() {
        // Defensive path: if final_messages has no Assistant message (e.g.
        // the loop terminated before any assistant reply was produced),
        // the chunk must still fire but carry an empty content string —
        // not panic, not error.
        use grid_engine::AgentLoopResult;

        let result = AgentLoopResult {
            final_messages: vec![ChatMessage {
                role: MessageRole::User,
                content: vec![ContentBlock::Text { text: "hi".into() }],
            }],
            ..Default::default()
        };

        let chunk = GridHarness::event_to_chunk(AgentEvent::Completed(result)).unwrap();
        assert_eq!(chunk.chunk_type, "done");
        assert_eq!(chunk.content, "");
        assert!(!chunk.is_error);
    }

    #[test]
    fn event_to_chunk_completed_skips_non_text_blocks() {
        // Tool-use blocks in the final assistant message must NOT leak into
        // the response_text. `ChatMessage::text_content` filters for Text
        // variants only, so a mid-message ToolUse is silently dropped.
        use grid_engine::AgentLoopResult;

        let result = AgentLoopResult {
            final_messages: vec![ChatMessage {
                role: MessageRole::Assistant,
                content: vec![
                    ContentBlock::Text { text: "final answer: ".into() },
                    ContentBlock::ToolUse {
                        id: "ignored-t1".into(),
                        name: "ignored-tool".into(),
                        input: serde_json::json!({}),
                    },
                    ContentBlock::Text { text: "42".into() },
                ],
            }],
            ..Default::default()
        };

        let chunk = GridHarness::event_to_chunk(AgentEvent::Completed(result)).unwrap();
        assert_eq!(chunk.chunk_type, "done");
        assert_eq!(chunk.content, "final answer: 42");
    }

    #[test]
    fn event_to_chunk_thinking() {
        let event = AgentEvent::ThinkingDelta { text: "analyzing...".into() };
        let chunk = GridHarness::event_to_chunk(event).unwrap();
        assert_eq!(chunk.chunk_type, "thinking");
    }

    #[test]
    fn build_memory_preamble_empty_returns_empty_string() {
        // D2 — empty memory_refs must produce an empty preamble so the
        // harness can unconditionally skip adding a system message.
        let out = GridHarness::build_memory_preamble(&[]);
        assert_eq!(out, "");
    }

    #[test]
    fn build_memory_preamble_formats_entries() {
        // D2 — preamble must contain the exact header plus one bullet
        // per memory, each prefixed with the memory_type in brackets.
        let refs = vec![
            MemoryRef {
                memory_id: "mem-1".into(),
                memory_type: "fact".into(),
                relevance_score: 0.95,
                content: "Device XYZ temperature threshold is 75C".into(),
                source_session_id: "s-prev".into(),
                created_at: "2026-04-10T00:00:00Z".into(),
                tags: Default::default(),
            },
            MemoryRef {
                memory_id: "mem-2".into(),
                memory_type: "preference".into(),
                relevance_score: 0.80,
                content: "User prefers conservative thresholds".into(),
                source_session_id: "s-prev".into(),
                created_at: "2026-04-10T00:00:00Z".into(),
                tags: Default::default(),
            },
        ];
        let out = GridHarness::build_memory_preamble(&refs);
        assert!(out.starts_with("## Prior memories from previous sessions\n"));
        assert!(out.contains("- [fact] Device XYZ temperature threshold is 75C"));
        assert!(out.contains("- [preference] User prefers conservative thresholds"));
    }

    #[test]
    fn resolve_mcp_dependencies_basic() {
        let deps = vec!["mcp:mock-scada".to_string(), "mcp:eaasp-l2-memory".to_string()];
        let metadata = std::collections::HashMap::new();
        let configs = GridHarness::resolve_mcp_dependencies(&deps, &metadata);
        assert_eq!(configs.len(), 2);
        assert_eq!(configs[0].name, "mock-scada");
        assert_eq!(configs[0].transport, "stdio");
        assert_eq!(configs[0].command.as_deref(), Some("mock-scada"));
        assert!(configs[0].args.is_empty());
        assert_eq!(configs[1].name, "eaasp-l2-memory");
        assert_eq!(configs[1].command.as_deref(), Some("eaasp-l2-memory"));
    }

    #[test]
    fn resolve_mcp_dependencies_with_metadata_override() {
        let deps = vec!["mcp:bar".to_string()];
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("mcp.bar.command".to_string(), "python -m bar".to_string());
        metadata.insert("mcp.bar.args".to_string(), "--verbose --port 8080".to_string());
        let configs = GridHarness::resolve_mcp_dependencies(&deps, &metadata);
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].name, "bar");
        assert_eq!(configs[0].command.as_deref(), Some("python -m bar"));
        assert_eq!(configs[0].args, vec!["--verbose", "--port", "8080"]);
    }

    #[test]
    fn resolve_mcp_dependencies_filters_non_mcp() {
        let deps = vec![
            "mcp:foo".to_string(),
            "pip:numpy".to_string(),
            "npm:lodash".to_string(),
        ];
        let metadata = std::collections::HashMap::new();
        let configs = GridHarness::resolve_mcp_dependencies(&deps, &metadata);
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].name, "foo");
    }

    #[test]
    fn resolve_mcp_dependencies_empty_input() {
        let deps: Vec<String> = vec![];
        let metadata = std::collections::HashMap::new();
        let configs = GridHarness::resolve_mcp_dependencies(&deps, &metadata);
        assert!(configs.is_empty());
    }

    #[test]
    fn event_to_chunk_internal_event_returns_none() {
        let event = AgentEvent::IterationStart { round: 1 };
        assert!(GridHarness::event_to_chunk(event).is_none());

        let event = AgentEvent::TokenBudgetUpdate {
            budget: grid_types::TokenBudgetSnapshot {
                total: 200_000,
                system_prompt: 5000,
                dynamic_context: 1000,
                history: 10000,
                free: 184000,
                usage_percent: 8.0,
                degradation_level: 0,
            },
        };
        assert!(GridHarness::event_to_chunk(event).is_none());
    }
}
