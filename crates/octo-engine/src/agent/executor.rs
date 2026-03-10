use std::path::PathBuf;
use std::sync::Arc;

use futures_util::StreamExt;
use tokio::sync::{broadcast, mpsc};
use tracing::info;

use octo_types::{ChatMessage, PathValidator, SandboxId, SessionId, ToolContext, UserId};

use crate::agent::{AgentConfig, AgentEvent, AgentLoopConfig};
use crate::memory::store_traits::MemoryStore;
use crate::memory::WorkingMemory;
use crate::providers::Provider;
use crate::tools::ToolRegistry;

use super::entry::AgentManifest;
use super::harness::run_agent_loop;
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
            turn_gate: super::turn_gate::TurnGate::new(),
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

                    // 从共享 ToolRegistry 生成快照（每 round 新建，实现 MCP 热插拔）
                    let tools_snapshot = {
                        let guard = self.tools.lock().unwrap_or_else(|e| e.into_inner());
                        let mut registry = ToolRegistry::new();
                        for (name, tool) in guard.iter() {
                            registry.register_arc(name.clone(), tool);
                        }
                        Arc::new(registry)
                    };

                    let model = self
                        .model
                        .clone()
                        .unwrap_or_else(|| "claude-sonnet-4-6".to_string());

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
                        ..AgentLoopConfig::default()
                    };

                    // Call the harness directly and consume the event stream
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
            }
        }

        info!(session_id = %self.session_id.as_str(), "AgentExecutor stopped (channel closed)");
    }

    /// 返回当前对话历史（用于 session 持久化）
    pub fn history(&self) -> &[ChatMessage] {
        &self.history
    }
}
