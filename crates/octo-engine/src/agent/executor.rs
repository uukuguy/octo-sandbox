use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::{broadcast, mpsc};
use tracing::{info, warn};

use octo_types::{ChatMessage, SandboxId, SessionId, ToolContext, UserId};

use crate::agent::{AgentConfig, AgentEvent, AgentLoop};
use crate::memory::store_traits::MemoryStore;
use crate::memory::WorkingMemory;
use crate::providers::Provider;
use crate::tools::ToolRegistry;

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
    cancel_flag: Arc<AtomicBool>,

    // 工作目录
    working_dir: PathBuf,
    // 事件总线
    event_bus: Option<Arc<crate::event::EventBus>>,
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
        event_bus: Option<Arc<crate::event::EventBus>>,
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
            cancel_flag: Arc::new(AtomicBool::new(false)),
            working_dir,
            event_bus,
        }
    }

    /// Agent 主循环入口 — 持续等待消息，处理，广播结果
    pub async fn run(mut self) {
        info!(session_id = %self.session_id.as_str(), "AgentExecutor started");

        while let Some(msg) = self.rx.recv().await {
            match msg {
                AgentMessage::UserMessage { content, .. } => {
                    // 重置取消标志
                    self.cancel_flag.store(false, Ordering::Relaxed);

                    // 追加用户消息到持久化历史
                    self.history.push(ChatMessage::user(content));

                    // 从共享 ToolRegistry 生成快照（每 round 新建，实现 MCP 热插拔）
                    let tools_snapshot = {
                        let guard = self.tools.lock().unwrap();
                        let mut registry = ToolRegistry::new();
                        for (name, tool) in guard.iter() {
                            registry.register_arc(name.clone(), tool);
                        }
                        Arc::new(registry)
                    };

                    // 构建 AgentLoop（每 round 新建，但 history 由 AgentExecutor 持有）
                    let mut agent_loop =
                        AgentLoop::new(self.provider.clone(), tools_snapshot, self.memory.clone());
                    if let Some(ref ms) = self.memory_store {
                        agent_loop = agent_loop.with_memory_store(ms.clone());
                    }
                    // AgentLoop::run() 中有 assert!(!self.model.is_empty())，必须设置 model
                    let model = self
                        .model
                        .clone()
                        .unwrap_or_else(|| "claude-sonnet-4-6".to_string());
                    agent_loop = agent_loop.with_model(model);

                    // 注入 manifest 配置
                    if let Some(ref prompt) = self.system_prompt {
                        agent_loop = agent_loop.with_system_prompt(prompt.clone());
                    }
                    agent_loop = agent_loop.with_config(self.config.clone());

                    // 注入事件总线
                    if let Some(ref bus) = self.event_bus {
                        agent_loop = agent_loop.with_event_bus(bus.clone());
                    }

                    let tool_ctx = ToolContext {
                        sandbox_id: self.sandbox_id.clone(),
                        working_dir: self.working_dir.clone(),
                    };
                    let _ = tokio::fs::create_dir_all(&tool_ctx.working_dir).await;

                    if let Err(e) = agent_loop
                        .run(
                            &self.session_id,
                            &self.user_id,
                            &self.sandbox_id,
                            &mut self.history,
                            self.broadcast_tx.clone(),
                            tool_ctx,
                            Some(self.cancel_flag.clone()),
                        )
                        .await
                    {
                        warn!("AgentExecutor round error: {e}");
                        let _ = self.broadcast_tx.send(AgentEvent::Error {
                            message: e.to_string(),
                        });
                    }

                    // 持久化 history 到 SessionStore
                    if let Some(ref store) = self.session_store {
                        store
                            .set_messages(&self.session_id, self.history.clone())
                            .await;
                    }
                }
                AgentMessage::Cancel => {
                    self.cancel_flag.store(true, Ordering::Relaxed);
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
