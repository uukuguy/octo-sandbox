use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::{broadcast, mpsc};
use tracing::info;

use octo_types::{ChatMessage, SandboxId, SessionId, UserId};

use crate::agent::{AgentEvent, AgentMessage, AgentRuntime, AgentRuntimeHandle};
use crate::memory::store_traits::MemoryStore;
use crate::memory::WorkingMemory;
use crate::providers::Provider;
use crate::tools::ToolRegistry;

const MPSC_CAPACITY: usize = 32;
const BROADCAST_CAPACITY: usize = 256;

/// Session → AgentRuntimeHandle 的注册表
pub struct AgentSupervisor {
    handles: DashMap<SessionId, AgentRuntimeHandle>,
}

impl AgentSupervisor {
    pub fn new() -> Self {
        Self {
            handles: DashMap::new(),
        }
    }

    /// 获取已有的 AgentRuntimeHandle（如果存在）
    pub fn get(&self, session_id: &SessionId) -> Option<AgentRuntimeHandle> {
        self.handles.get(session_id).map(|h| h.clone())
    }

    /// Spawn 一个新的 AgentRuntime 并注册其 Handle。
    /// 如果该 session 已有运行中的 runtime，直接返回已有 handle。
    #[allow(clippy::too_many_arguments)]
    pub fn get_or_spawn(
        &self,
        session_id: SessionId,
        user_id: UserId,
        sandbox_id: SandboxId,
        initial_history: Vec<ChatMessage>,
        provider: Arc<dyn Provider>,
        tools: Arc<ToolRegistry>,
        memory: Arc<dyn WorkingMemory>,
        memory_store: Option<Arc<dyn MemoryStore>>,
        model: Option<String>,
        session_store: Option<Arc<dyn crate::session::SessionStore>>,
    ) -> AgentRuntimeHandle {
        // 已有 handle 则直接复用
        if let Some(handle) = self.get(&session_id) {
            return handle;
        }

        let (tx, rx) = mpsc::channel::<AgentMessage>(MPSC_CAPACITY);
        let (broadcast_tx, _) = broadcast::channel::<AgentEvent>(BROADCAST_CAPACITY);

        let handle = AgentRuntimeHandle {
            tx,
            broadcast_tx: broadcast_tx.clone(),
            session_id: session_id.clone(),
        };

        let runtime = AgentRuntime::new(
            session_id.clone(),
            user_id,
            sandbox_id,
            initial_history,
            rx,
            broadcast_tx,
            provider,
            tools,
            memory,
            memory_store,
            model,
            session_store,
        );

        // Spawn 持久化主循环
        tokio::spawn(async move {
            runtime.run().await;
        });

        info!(session_id = %session_id.as_str(), "AgentRuntime spawned");

        self.handles.insert(session_id, handle.clone());
        handle
    }

    /// 移除 session 对应的 handle（当 session 销毁时调用）
    pub fn remove(&self, session_id: &SessionId) {
        self.handles.remove(session_id);
        info!(session_id = %session_id.as_str(), "AgentRuntime handle removed");
    }

    pub fn len(&self) -> usize {
        self.handles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.handles.is_empty()
    }
}

impl Default for AgentSupervisor {
    fn default() -> Self {
        Self::new()
    }
}
