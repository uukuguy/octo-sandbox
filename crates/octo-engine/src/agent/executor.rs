use std::path::PathBuf;
use std::sync::Arc;

use futures_util::StreamExt;
use tokio::sync::{broadcast, mpsc};
use tracing::info;

use octo_types::{ChatMessage, PathValidator, SandboxId, SessionId, ToolContext, UserId};

use crate::agent::{AgentConfig, AgentEvent, AgentLoopConfig};
use crate::context::{ContextBudgetManager, ContextPruner};
use crate::agent::subagent::SubAgentManager;
use crate::memory::store_traits::MemoryStore;
use crate::memory::{EventExtractor, ProceduralExtractor, SessionSummarizer, SessionSummaryStore, WorkingMemory};
use crate::providers::Provider;
use crate::sandbox::{
    DockerAdapter, OctoRunMode, SandboxProfile, SandboxRouter,
    SessionSandboxManager,
};
use crate::skills::{ExecuteSkillTool, SkillRegistry, SubAgentContext};
use crate::tools::bash::BashTool;
use crate::tools::ToolRegistry;

use super::entry::AgentManifest;
use super::harness::{run_agent_loop, rewind_messages};
use super::CancellationToken;

/// Channel → AgentExecutor 的消息
#[derive(Debug, Clone)]
pub enum AgentMessage {
    /// 用户发来的文本消息
    UserMessage {
        content: String,
        /// 消息来源 channel 标识（用于广播给其他 channel）
        channel_id: String,
    },
    /// 外部请求取消当前正在运行的 round
    Cancel,
    /// 清空对话历史（/clear 命令）
    ClearHistory,
    /// Pause autonomous mode (AQ-T4).
    Pause,
    /// Resume autonomous mode after pause (AQ-T4).
    Resume,
    /// Update user presence state for autonomous mode (AQ-T6).
    UserPresence(bool),
    /// AR-T4: Rewind conversation to a specific turn index.
    Rewind { to_turn: usize },
    /// AR-T4: Fork conversation at a turn into a new session.
    Fork {
        at_turn: usize,
        new_session_id: SessionId,
    },
}

/// AgentExecutor 的对外句柄（可 clone，廉价）
#[derive(Clone)]
pub struct AgentExecutorHandle {
    /// 向 AgentExecutor 发送消息
    pub tx: mpsc::Sender<AgentMessage>,
    /// 订阅 AgentExecutor 的广播事件
    pub broadcast_tx: broadcast::Sender<AgentEvent>,
    /// 关联的 session_id
    pub session_id: SessionId,
}

impl AgentExecutorHandle {
    /// 创建一个新的广播订阅者
    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent> {
        self.broadcast_tx.subscribe()
    }

    /// 发送消息到 AgentExecutor
    pub async fn send(
        &self,
        msg: AgentMessage,
    ) -> Result<(), mpsc::error::SendError<AgentMessage>> {
        self.tx.send(msg).await
    }

    /// Pause autonomous mode (AQ-T5).
    pub async fn pause(&self) -> Result<(), mpsc::error::SendError<AgentMessage>> {
        self.tx.send(AgentMessage::Pause).await
    }

    /// Resume autonomous mode after pause (AQ-T5).
    pub async fn resume(&self) -> Result<(), mpsc::error::SendError<AgentMessage>> {
        self.tx.send(AgentMessage::Resume).await
    }

    /// Update user presence state for autonomous mode (AQ-T6).
    pub async fn set_user_presence(&self, online: bool) -> Result<(), mpsc::error::SendError<AgentMessage>> {
        self.tx.send(AgentMessage::UserPresence(online)).await
    }

    /// AR-T4: Rewind conversation to a specific turn.
    pub async fn rewind(&self, to_turn: usize) -> Result<(), mpsc::error::SendError<AgentMessage>> {
        self.tx.send(AgentMessage::Rewind { to_turn }).await
    }

    /// AR-T4: Fork conversation at a turn into a new session.
    pub async fn fork(&self, at_turn: usize, new_session_id: SessionId) -> Result<(), mpsc::error::SendError<AgentMessage>> {
        self.tx.send(AgentMessage::Fork { at_turn, new_session_id }).await
    }
}

/// 持久化运行的 Agent 自主智能体本体
pub struct AgentExecutor {
    // 身份
    session_id: SessionId,
    user_id: UserId,
    sandbox_id: SandboxId,
    // P1-6: TurnGate prevents concurrent turns on same session
    turn_gate: super::turn_gate::TurnGate,

    // 通道
    rx: mpsc::Receiver<AgentMessage>,
    broadcast_tx: broadcast::Sender<AgentEvent>,

    // Harness 核心（所有字段跨 round 持久化）
    history: Vec<ChatMessage>,
    provider: Arc<dyn Provider>,
    tools: Arc<std::sync::Mutex<ToolRegistry>>,
    memory: Arc<dyn WorkingMemory>,
    memory_store: Option<Arc<dyn MemoryStore>>,
    model: Option<String>,
    session_store: Option<Arc<dyn crate::session::SessionStore>>,

    // manifest 配置（来自 AgentCatalog）
    system_prompt: Option<String>,
    config: AgentConfig,

    // 生命周期
    cancel_token: CancellationToken,

    // 工作目录
    working_dir: PathBuf,
    // 事件总线
    event_bus: Option<Arc<crate::event::TelemetryBus>>,
    // 路径安全验证器
    path_validator: Option<Arc<dyn PathValidator>>,
    // Hook 系统
    hook_registry: Option<Arc<crate::hooks::HookRegistry>>,
    // Safety pipeline with canary guard (T1)
    safety_pipeline: Option<Arc<crate::security::SafetyPipeline>>,
    // Canary token for system prompt injection (T1)
    canary_token: Option<String>,
    // Shared approval gate for interactive tool approval (T7)
    approval_gate: Option<crate::tools::approval::ApprovalGate>,
    // Skill registry for SubAgent-backed playbook skill execution
    skill_registry: Option<Arc<SkillRegistry>>,
    // Tool execution recorder for observability
    recorder: Option<Arc<crate::tools::recorder::ToolExecutionRecorder>>,
    // Session-scoped sandbox container manager (AC-T7)
    session_sandbox: Option<Arc<SessionSandboxManager>>,
    // Session summary store for episodic memory (Phase AG)
    session_summary_store: Option<Arc<SessionSummaryStore>>,
}

impl AgentExecutor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session_id: SessionId,
        user_id: UserId,
        sandbox_id: SandboxId,
        initial_history: Vec<ChatMessage>,
        rx: mpsc::Receiver<AgentMessage>,
        broadcast_tx: broadcast::Sender<AgentEvent>,
        provider: Arc<dyn Provider>,
        tools: Arc<std::sync::Mutex<ToolRegistry>>,
        memory: Arc<dyn WorkingMemory>,
        memory_store: Option<Arc<dyn MemoryStore>>,
        model: Option<String>,
        session_store: Option<Arc<dyn crate::session::SessionStore>>,
        system_prompt: Option<String>,
        config: AgentConfig,
        working_dir: PathBuf,
        event_bus: Option<Arc<crate::event::TelemetryBus>>,
        path_validator: Option<Arc<dyn PathValidator>>,
        hook_registry: Option<Arc<crate::hooks::HookRegistry>>,
        safety_pipeline: Option<Arc<crate::security::SafetyPipeline>>,
        canary_token: Option<String>,
        approval_gate: Option<crate::tools::approval::ApprovalGate>,
        skill_registry: Option<Arc<SkillRegistry>>,
        recorder: Option<Arc<crate::tools::recorder::ToolExecutionRecorder>>,
        session_sandbox: Option<Arc<SessionSandboxManager>>,
        session_summary_store: Option<Arc<SessionSummaryStore>>,
    ) -> Self {
        Self {
            session_id,
            user_id,
            sandbox_id,
            rx,
            broadcast_tx,
            history: initial_history,
            provider,
            tools,
            memory,
            memory_store,
            model,
            session_store,
            system_prompt,
            config,
            cancel_token: CancellationToken::new(),
            working_dir,
            event_bus,
            path_validator,
            hook_registry,
            safety_pipeline,
            canary_token,
            approval_gate,
            turn_gate: super::turn_gate::TurnGate::new(),
            skill_registry,
            recorder,
            session_sandbox,
            session_summary_store,
        }
    }

    /// Agent 主循环入口 — 持续等待消息，处理，广播结果
    pub async fn run(mut self) {
        info!(session_id = %self.session_id.as_str(), "AgentExecutor started");

        while let Some(msg) = self.rx.recv().await {
            match msg {
                AgentMessage::UserMessage { content, .. } => {
                    // P1-6: Acquire TurnGate to prevent concurrent turns
                    let _turn_guard = self.turn_gate.acquire().await;

                    // Reset cancellation token for the new turn
                    self.cancel_token = CancellationToken::new();

                    // 追加用户消息到持久化历史
                    self.history.push(ChatMessage::user(content));

                    let model = self
                        .model
                        .clone()
                        .unwrap_or_else(|| "claude-sonnet-4-6".to_string());

                    // 从共享 ToolRegistry 生成快照（每 round 新建，实现 MCP 热插拔）
                    // 同时为 execute_skill 工具注入 SubAgent 执行上下文
                    // AF-D1: 如果有 SSM，替换 BashTool 为带 session sandbox 的版本
                    let tools_snapshot = {
                        let guard = self.tools.lock().unwrap_or_else(|e| e.into_inner());
                        let mut registry = ToolRegistry::new();
                        for (name, tool) in guard.iter() {
                            // Skip default BashTool — we'll replace it with SSM-wired version below
                            if name == "bash" && self.session_sandbox.is_some() {
                                continue;
                            }
                            registry.register_arc(name.clone(), tool);
                        }
                        // Wire SSM into BashTool for per-session container execution
                        if let Some(ref ssm) = self.session_sandbox {
                            let run_mode = OctoRunMode::detect();
                            let profile = SandboxProfile::resolve(false, None, None);
                            let docker = DockerAdapter::new(crate::sandbox::DEFAULT_SANDBOX_IMAGE);
                            let mut router = SandboxRouter::with_policy(profile.policy());
                            router.register_adapter(crate::sandbox::AdapterEnum::Docker(docker));
                            let bash = BashTool::with_session_sandbox(
                                run_mode,
                                profile,
                                router,
                                ssm.clone(),
                                self.session_id.as_str().to_string(),
                            );
                            registry.register(bash);
                        }
                        // Replace execute_skill with SubAgent-wired version
                        if let Some(ref skill_reg) = self.skill_registry {
                            if registry.get("execute_skill").is_some() {
                                let subagent_ctx = SubAgentContext {
                                    manager: Arc::new(SubAgentManager::new(4, 3)),
                                    provider: self.provider.clone(),
                                    tools: Arc::new({
                                        // Snapshot parent tools for SubAgent
                                        let mut parent = ToolRegistry::new();
                                        for (n, t) in guard.iter() {
                                            parent.register_arc(n.clone(), t);
                                        }
                                        parent
                                    }),
                                    model: model.clone(),
                                    user_id: self.user_id.clone(),
                                    sandbox_id: self.sandbox_id.clone(),
                                    tool_ctx: Some(ToolContext {
                                        sandbox_id: self.sandbox_id.clone(),
                                        working_dir: self.working_dir.clone(),
                                        path_validator: self.path_validator.clone(),
                                    }),
                                    event_sender: Some(self.broadcast_tx.clone()),
                                };
                                registry.register(
                                    ExecuteSkillTool::new(skill_reg.clone())
                                        .with_subagent_ctx(subagent_ctx),
                                );
                            }
                        }
                        Arc::new(registry)
                    };

                    // Resolve manifest from system_prompt if set
                    let manifest = self.system_prompt.as_ref().map(|prompt| AgentManifest {
                        name: String::new(),
                        tags: Vec::new(),
                        role: None,
                        goal: None,
                        backstory: None,
                        system_prompt: Some(prompt.clone()),
                        model: None,
                        tool_filter: Vec::new(),
                        config: AgentConfig::default(),
                        max_concurrent_tasks: 0,
                        priority: None,
                    });

                    let tool_ctx = ToolContext {
                        sandbox_id: self.sandbox_id.clone(),
                        working_dir: self.working_dir.clone(),
                        path_validator: self.path_validator.clone(),
                    };
                    let _ = tokio::fs::create_dir_all(&tool_ctx.working_dir).await;

                    // --- AQ-T1: Create shared InteractionGate for this turn ---
                    let interaction_gate = Arc::new(
                        crate::tools::interaction::InteractionGate::new(),
                    );

                    // --- AQ-T3: Create BlobStore from working dir ---
                    // Uses <working_dir>/.octo/blobs/ as the blob storage location.
                    let blob_store = {
                        let blobs_dir = self.working_dir.join(".octo").join("blobs");
                        Arc::new(crate::storage::BlobStore::new(blobs_dir))
                    };

                    // --- AR-T2: Create TranscriptWriter for session audit trail ---
                    let transcript_writer = {
                        let transcripts_dir = self.working_dir.join(".octo").join("transcripts");
                        Arc::new(crate::session::TranscriptWriter::new(
                            transcripts_dir,
                            self.session_id.as_str(),
                        ))
                    };

                    // --- Phase AS: Collect git context for system prompt ---
                    let git_context = collect_git_context(&self.working_dir);

                    // Build AgentLoopConfig directly (D5 Stage 3)
                    let loop_config = AgentLoopConfig {
                        max_iterations: if self.config.max_rounds == 0 {
                            u32::MAX
                        } else {
                            self.config.max_rounds
                        },
                        model,
                        provider: Some(self.provider.clone()),
                        tools: Some(tools_snapshot),
                        memory: Some(self.memory.clone()),
                        memory_store: self.memory_store.clone(),
                        event_bus: self.event_bus.clone(),
                        hook_registry: self.hook_registry.clone(),
                        manifest,
                        session_id: self.session_id.clone(),
                        user_id: self.user_id.clone(),
                        sandbox_id: self.sandbox_id.clone(),
                        tool_ctx: Some(tool_ctx),
                        cancel_token: self.cancel_token.clone(),
                        agent_config: self.config.clone(),
                        safety_pipeline: self.safety_pipeline.clone(),
                        canary_token: self.canary_token.clone(),
                        approval_gate: self.approval_gate.clone(),
                        recorder: self.recorder.clone(),
                        budget: Some(ContextBudgetManager::default()),
                        pruner: Some(ContextPruner::new()),
                        loop_guard: Some(super::loop_guard::LoopGuard::new()),
                        session_summary_store: self.session_summary_store.clone(),
                        interaction_gate: Some(interaction_gate),
                        blob_store: Some(blob_store),
                        transcript_writer: Some(transcript_writer.clone()),
                        working_dir: Some(self.working_dir.clone()),
                        git_context,
                        ..AgentLoopConfig::default()
                    };

                    // Call the harness directly and consume the event stream
                    let history_len_before = self.history.len();
                    let mut stream = run_agent_loop(loop_config, self.history.clone());

                    while let Some(event) = stream.next().await {
                        // Capture final_messages from the Completed event
                        if let AgentEvent::Completed(ref result) = event {
                            if !result.final_messages.is_empty() {
                                self.history = result.final_messages.clone();
                            }
                        }

                        let is_done = matches!(event, AgentEvent::Done);
                        let _ = self.broadcast_tx.send(event);

                        if is_done {
                            break;
                        }
                    }

                    // AR-T2: Write new messages to transcript
                    Self::write_transcript(
                        &transcript_writer,
                        &self.history,
                        history_len_before,
                    );

                    // 持久化 history 到 SessionStore
                    if let Some(ref store) = self.session_store {
                        store
                            .set_messages(&self.session_id, self.history.clone())
                            .await;
                    }
                }
                AgentMessage::Cancel => {
                    self.cancel_token.cancel();
                    info!(session_id = %self.session_id.as_str(), "AgentExecutor: cancel requested");
                }
                AgentMessage::ClearHistory => {
                    self.history.clear();
                    // Persist empty history to SessionStore
                    if let Some(ref store) = self.session_store {
                        store.set_messages(&self.session_id, vec![]).await;
                    }
                    info!(session_id = %self.session_id.as_str(), "AgentExecutor: history cleared");
                }
                AgentMessage::Pause => {
                    info!(session_id = %self.session_id.as_str(), "AgentExecutor: pause requested (autonomous)");
                    let _ = self.broadcast_tx.send(AgentEvent::AutonomousPaused);
                }
                AgentMessage::Resume => {
                    info!(session_id = %self.session_id.as_str(), "AgentExecutor: resume requested (autonomous)");
                    let _ = self.broadcast_tx.send(AgentEvent::AutonomousResumed);
                }
                AgentMessage::UserPresence(online) => {
                    info!(session_id = %self.session_id.as_str(), online, "AgentExecutor: user presence updated");
                }
                AgentMessage::Rewind { to_turn } => {
                    let before = self.history.len();
                    rewind_messages(&mut self.history, to_turn);
                    info!(
                        session_id = %self.session_id.as_str(),
                        to_turn,
                        before,
                        after = self.history.len(),
                        "AgentExecutor: conversation rewound"
                    );
                    if let Some(ref store) = self.session_store {
                        store.set_messages(&self.session_id, self.history.clone()).await;
                    }
                }
                AgentMessage::Fork { at_turn, new_session_id } => {
                    let mut forked = self.history.clone();
                    rewind_messages(&mut forked, at_turn);
                    if let Some(ref store) = self.session_store {
                        let _ = store.create_session_with_user(&self.user_id).await;
                        store.set_messages(&new_session_id, forked).await;
                    }
                    info!(
                        session_id = %self.session_id.as_str(),
                        new_session_id = %new_session_id.as_str(),
                        at_turn,
                        "AgentExecutor: conversation forked"
                    );
                }
            }
        }

        // Session-end memory extraction (Phase AG)
        self.run_session_end_hooks().await;

        // Release session sandbox container before stopping (AC-T7)
        self.shutdown_sandbox().await;

        info!(session_id = %self.session_id.as_str(), "AgentExecutor stopped (channel closed)");
    }

    /// Run session-end memory extraction hooks.
    ///
    /// Three-step async pipeline (each step independent — failure in one
    /// does not block others):
    ///   1. Rule-based extraction (SessionEndMemoryHook) → L2 semantic memories
    ///   2. LLM event extraction (EventExtractor) → L2 episodic memories
    ///   3. LLM session summary (SessionSummarizer) → session_summaries table
    async fn run_session_end_hooks(&self) {
        if self.history.is_empty() {
            return;
        }
        let Some(ref store) = self.memory_store else {
            return;
        };

        let model = self
            .model
            .clone()
            .unwrap_or_else(|| "claude-sonnet-4-6".to_string());

        // Step 1: Rule-based extraction (existing)
        let hook = crate::memory::SessionEndMemoryHook::with_defaults();
        let count = hook
            .on_session_end(&self.history, store.as_ref(), self.user_id.as_str())
            .await;
        if count > 0 {
            info!(
                session_id = %self.session_id.as_str(),
                extracted = count,
                "Step 1: Rule-based memory extraction complete"
            );
        }

        // Step 2: LLM event extraction → episodic memories
        match EventExtractor::extract_events(self.provider.as_ref(), &self.history, &model).await {
            Ok(events) if !events.is_empty() => {
                let mut event_stored = 0;
                for event in &events {
                    let entry = octo_types::MemoryEntry::new_episodic(
                        self.user_id.as_str(),
                        event,
                        self.session_id.as_str(),
                    );
                    if store.store(entry).await.is_ok() {
                        event_stored += 1;
                    }
                }
                info!(
                    session_id = %self.session_id.as_str(),
                    events = event_stored,
                    "Step 2: Event extraction complete"
                );
            }
            Ok(_) => {
                tracing::debug!(
                    session_id = %self.session_id.as_str(),
                    "Step 2: No events extracted"
                );
            }
            Err(e) => {
                tracing::warn!(
                    session_id = %self.session_id.as_str(),
                    error = %e,
                    "Step 2: Event extraction failed"
                );
            }
        }

        // Step 3: Session summary → session_summaries table
        if let Some(ref summary_store) = self.session_summary_store {
            match SessionSummarizer::summarize(self.provider.as_ref(), &self.history, &model).await {
                Ok(summary) if !summary.text.is_empty() => {
                    if let Err(e) = summary_store
                        .save(
                            self.session_id.as_str(),
                            &summary.text,
                            summary.event_count,
                            &summary.key_topics,
                            count, // memory_count from step 1
                        )
                        .await
                    {
                        tracing::warn!(
                            session_id = %self.session_id.as_str(),
                            error = %e,
                            "Step 3: Failed to save session summary"
                        );
                    } else {
                        info!(
                            session_id = %self.session_id.as_str(),
                            topics = ?summary.key_topics,
                            "Step 3: Session summary saved"
                        );
                    }
                }
                Ok(_) => {
                    tracing::debug!(
                        session_id = %self.session_id.as_str(),
                        "Step 3: Empty summary, skipped"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        session_id = %self.session_id.as_str(),
                        error = %e,
                        "Step 3: Session summarization failed"
                    );
                }
            }
        }

        // Step 4: Procedural pattern extraction → L2 procedural memories
        match ProceduralExtractor::extract_patterns(self.provider.as_ref(), &self.history, &model)
            .await
        {
            Ok(patterns) if !patterns.is_empty() => {
                let mut proc_stored = 0;
                for pattern in &patterns {
                    let entry = octo_types::MemoryEntry::new_procedural(
                        self.user_id.as_str(),
                        &pattern.description,
                        &pattern.tool_sequence,
                        &pattern.task_type,
                        self.session_id.as_str(),
                    );
                    if store.store(entry).await.is_ok() {
                        proc_stored += 1;
                    }
                }
                info!(
                    session_id = %self.session_id.as_str(),
                    patterns = proc_stored,
                    "Step 4: Procedural pattern extraction complete"
                );
            }
            Ok(_) => {
                tracing::debug!(
                    session_id = %self.session_id.as_str(),
                    "Step 4: No procedural patterns extracted"
                );
            }
            Err(e) => {
                tracing::warn!(
                    session_id = %self.session_id.as_str(),
                    error = %e,
                    "Step 4: Procedural pattern extraction failed"
                );
            }
        }
    }

    /// Release the session sandbox container, if any.
    async fn shutdown_sandbox(&self) {
        if let Some(ref ssm) = self.session_sandbox {
            if let Err(e) = ssm.release(self.session_id.as_str()).await {
                tracing::warn!(
                    session_id = %self.session_id.as_str(),
                    error = %e,
                    "Failed to release session sandbox"
                );
            }
        }
    }

    /// Expose session sandbox manager for BashTool wiring.
    pub fn session_sandbox(&self) -> Option<&Arc<SessionSandboxManager>> {
        self.session_sandbox.as_ref()
    }

    /// 返回当前对话历史（用于 session 持久化）
    pub fn history(&self) -> &[ChatMessage] {
        &self.history
    }

    /// AR-T2: Write new messages (since `from_idx`) to the transcript.
    fn write_transcript(
        writer: &Arc<crate::session::TranscriptWriter>,
        history: &[ChatMessage],
        from_idx: usize,
    ) {
        use crate::session::transcript::{make_preview, TranscriptEntry};
        use chrono::Utc;

        for msg in history.iter().skip(from_idx) {
            let text = msg
                .content
                .iter()
                .filter_map(|b| match b {
                    octo_types::ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            if text.is_empty() {
                continue;
            }
            let entry = TranscriptEntry {
                timestamp: Utc::now(),
                role: format!("{:?}", msg.role).to_lowercase(),
                content_preview: make_preview(&text),
                blob_ref: None,
                tool_name: None,
                input_tokens: None,
                output_tokens: None,
            };
            if let Err(e) = writer.append(&entry) {
                tracing::debug!(error = %e, "TranscriptWriter: failed to append entry");
            }
        }
    }
}

/// Collect git context (branch, status, recent commits) from the working directory.
/// Returns None if not a git repo or git is unavailable.
fn collect_git_context(working_dir: &std::path::Path) -> Option<super::loop_config::GitContext> {
    use std::process::Command;

    // Quick check: is this a git repo?
    let branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(working_dir)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())?;

    let status = Command::new("git")
        .args(["status", "--short"])
        .current_dir(working_dir)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            let s = String::from_utf8_lossy(&o.stdout);
            // Limit to first 20 lines to avoid bloating system prompt
            s.lines().take(20).collect::<Vec<_>>().join("\n")
        })
        .unwrap_or_default();

    let recent_commits = Command::new("git")
        .args(["log", "--oneline", "-5"])
        .current_dir(working_dir)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    Some(super::loop_config::GitContext {
        branch,
        status,
        recent_commits,
    })
}
