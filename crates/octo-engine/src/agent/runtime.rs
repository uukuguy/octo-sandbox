use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::{broadcast, mpsc};
use tracing::info;

use octo_types::{ChatMessage, SandboxId, SessionId, UserId};

use crate::agent::{
    AgentCatalog, AgentConfig, AgentError, AgentEvent, AgentId, AgentManifest,
    AgentMessage, AgentExecutor, AgentExecutorHandle, AgentStatus, CancellationToken,
};
use crate::event::EventBus;
use crate::memory::store_traits::MemoryStore;
use crate::memory::WorkingMemory;
use crate::providers::{Provider, ProviderChain};
use crate::session::SessionStore;
use crate::skills::{SkillRegistry, SkillTool};
use crate::tools::recorder::ToolExecutionRecorder;
use crate::tools::ToolRegistry;

const MPSC_CAPACITY: usize = 32;
const BROADCAST_CAPACITY: usize = 256;

/// Session → AgentExecutorHandle 的注册表，同时持有所有共享运行时依赖
pub struct AgentRuntime {
    handles: DashMap<SessionId, AgentExecutorHandle>,
    /// AgentId → CancellationToken，用于 stop/pause 时取消正在运行的 AgentExecutor
    agent_handles: DashMap<AgentId, CancellationToken>,
    // 定义层
    catalog: Arc<AgentCatalog>,
    // 共享依赖（构造时注入一次）
    provider: Arc<dyn Provider>,
    tools: Arc<ToolRegistry>,
    skill_registry: Option<Arc<SkillRegistry>>,
    memory: Arc<dyn WorkingMemory>,
    memory_store: Option<Arc<dyn MemoryStore>>,
    session_store: Option<Arc<dyn SessionStore>>,
    default_model: String,
    // TODO: forward to AgentExecutor once observability wiring is added
    event_bus: Option<Arc<EventBus>>,
    recorder: Option<Arc<ToolExecutionRecorder>>,
    provider_chain: Option<Arc<ProviderChain>>,
}

impl AgentRuntime {
    pub fn new(
        catalog: Arc<AgentCatalog>,
        provider: Arc<dyn Provider>,
        tools: Arc<ToolRegistry>,
        memory: Arc<dyn WorkingMemory>,
        default_model: String,
    ) -> Self {
        Self {
            handles: DashMap::new(),
            agent_handles: DashMap::new(),
            catalog,
            provider,
            tools,
            skill_registry: None,
            memory,
            memory_store: None,
            session_store: None,
            default_model,
            event_bus: None,
            recorder: None,
            provider_chain: None,
        }
    }

    pub fn with_skill_registry(mut self, skills: Arc<SkillRegistry>) -> Self {
        self.skill_registry = Some(skills);
        self
    }

    pub fn with_memory_store(mut self, store: Arc<dyn MemoryStore>) -> Self {
        self.memory_store = Some(store);
        self
    }

    pub fn with_session_store(mut self, store: Arc<dyn SessionStore>) -> Self {
        self.session_store = Some(store);
        self
    }

    pub fn with_event_bus(mut self, bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    pub fn with_recorder(mut self, recorder: Arc<ToolExecutionRecorder>) -> Self {
        self.recorder = Some(recorder);
        self
    }

    pub fn with_provider_chain(mut self, chain: Arc<ProviderChain>) -> Self {
        self.provider_chain = Some(chain);
        self
    }

    // ── Getter 方法（供 server API 层只读访问） ──────────────────────────────

    pub fn catalog(&self) -> &Arc<AgentCatalog> {
        &self.catalog
    }

    pub fn tools(&self) -> &Arc<ToolRegistry> {
        &self.tools
    }

    pub fn memory(&self) -> &Arc<dyn WorkingMemory> {
        &self.memory
    }

    pub fn memory_store(&self) -> Option<&Arc<dyn MemoryStore>> {
        self.memory_store.as_ref()
    }

    pub fn session_store(&self) -> Option<&Arc<dyn SessionStore>> {
        self.session_store.as_ref()
    }

    pub fn recorder(&self) -> Option<&Arc<ToolExecutionRecorder>> {
        self.recorder.as_ref()
    }

    pub fn provider_chain(&self) -> Option<&Arc<ProviderChain>> {
        self.provider_chain.as_ref()
    }

    /// 获取已有的 AgentExecutorHandle（如果存在）
    pub fn get(&self, session_id: &SessionId) -> Option<AgentExecutorHandle> {
        self.handles.get(session_id).map(|h| h.clone())
    }

    /// 获取或 spawn 与 session 绑定的 AgentExecutor。
    /// agent_id: 可选，指定要绑定的 AgentCatalog 中的 agent 定义（携带 manifest）。
    pub fn get_or_spawn(
        &self,
        session_id: SessionId,
        user_id: UserId,
        sandbox_id: SandboxId,
        initial_history: Vec<ChatMessage>,
        agent_id: Option<&AgentId>,
    ) -> AgentExecutorHandle {
        // 已有 handle 则直接复用
        if let Some(handle) = self.get(&session_id) {
            return handle;
        }

        // 从 manifest 解析运行时配置
        let (tools, system_prompt, model, config) = self.resolve_runtime_config(agent_id);

        let (tx, rx) = mpsc::channel::<AgentMessage>(MPSC_CAPACITY);
        let (broadcast_tx, _) = broadcast::channel::<AgentEvent>(BROADCAST_CAPACITY);

        let handle = AgentExecutorHandle {
            tx,
            broadcast_tx: broadcast_tx.clone(),
            session_id: session_id.clone(),
        };

        let runtime = AgentExecutor::new(
            session_id.clone(),
            user_id,
            sandbox_id,
            initial_history,
            rx,
            broadcast_tx,
            self.provider.clone(),
            tools,
            self.memory.clone(),
            self.memory_store.clone(),
            Some(model),
            self.session_store.clone(),
            system_prompt,
            config,
        );

        // Spawn 持久化主循环
        tokio::spawn(async move {
            runtime.run().await;
        });

        if let Some(id) = agent_id {
            let cancel_token = CancellationToken::new();
            self.agent_handles.insert(id.clone(), cancel_token);
            self.catalog.update_state(id, AgentStatus::Running);
        }

        info!(session_id = %session_id.as_str(), "AgentExecutor spawned");

        self.handles.insert(session_id, handle.clone());
        handle
    }

    /// 启动主 Runtime 并返回其 Handle。
    /// 由 main.rs 在 server 启动时调用一次。
    /// channels（ws.rs 等）通过持有返回的 Handle 与 Agent 通信，
    /// 无需持有 AgentRuntime 引用（解耦）。
    pub fn start_primary(
        &self,
        session_id: SessionId,
        user_id: UserId,
        sandbox_id: SandboxId,
        initial_history: Vec<ChatMessage>,
        agent_id: Option<&AgentId>,
    ) -> AgentExecutorHandle {
        self.get_or_spawn(session_id, user_id, sandbox_id, initial_history, agent_id)
    }

    /// 移除 session 对应的 handle（当 session 销毁时调用）
    pub fn remove(&self, session_id: &SessionId) {
        self.handles.remove(session_id);
        info!(session_id = %session_id.as_str(), "AgentExecutor handle removed");
    }

    pub fn len(&self) -> usize {
        self.handles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.handles.is_empty()
    }

    /// 按 tool_filter 构建 ToolRegistry（含 SkillRegistry 热重载 overlay）
    fn build_tool_registry(&self, tool_filter: &[String]) -> Arc<ToolRegistry> {
        // 快速路径：无动态 skills 且无 filter
        if self.skill_registry.is_none() && tool_filter.is_empty() {
            return self.tools.clone();
        }

        // 从全局工具快照构建
        let mut registry = ToolRegistry::new();
        for (name, tool) in self.tools.iter() {
            registry.register_arc(name.clone(), tool);
        }

        // 覆盖当前热重载的 skill tools
        if let Some(ref skills) = self.skill_registry {
            for skill in skills.invocable_skills() {
                let name = skill.name.clone();
                registry.register_arc(name, Arc::new(SkillTool::new(skill)));
            }
        }

        // 应用 per-agent tool filter
        if tool_filter.is_empty() {
            return Arc::new(registry);
        }
        let mut filtered = ToolRegistry::new();
        for name in tool_filter {
            if let Some(tool) = registry.get(name) {
                filtered.register_arc(name.clone(), tool);
            }
        }
        Arc::new(filtered)
    }

    /// 从 AgentManifest 构建 system prompt
    fn build_system_prompt(manifest: &AgentManifest) -> Option<String> {
        if let Some(ref prompt) = manifest.system_prompt {
            return Some(prompt.clone());
        }
        if manifest.role.is_some() || manifest.goal.is_some() || manifest.backstory.is_some() {
            let mut parts: Vec<String> = Vec::new();
            if let Some(ref role) = manifest.role {
                parts.push(format!("## Role\n{role}"));
            }
            if let Some(ref goal) = manifest.goal {
                parts.push(format!("## Goal\n{goal}"));
            }
            if let Some(ref backstory) = manifest.backstory {
                parts.push(format!("## Backstory\n{backstory}"));
            }
            return Some(parts.join("\n\n"));
        }
        None  // 返回 None 表示使用 AgentLoop 默认（SOUL.md）
    }

    /// 启动 agent：从 catalog 读取 manifest，spawn AgentExecutor，更新状态机。
    /// session_id：为该 agent 创建或复用的会话标识。
    pub fn start(
        &self,
        agent_id: &AgentId,
        session_id: SessionId,
        user_id: UserId,
        sandbox_id: SandboxId,
        initial_history: Vec<ChatMessage>,
    ) -> Result<AgentExecutorHandle, AgentError> {
        // 验证 agent 存在
        self.catalog
            .get(agent_id)
            .ok_or_else(|| AgentError::NotFound(agent_id.clone()))?;

        // spawn Runtime（内部会调用 catalog.mark_running）
        let handle = self.get_or_spawn(
            session_id,
            user_id,
            sandbox_id,
            initial_history,
            Some(agent_id),
        );

        Ok(handle)
    }

    /// 停止 agent：发送 Cancel，移除 handle，更新 catalog 状态。
    pub async fn stop(&self, agent_id: &AgentId, session_id: &SessionId) -> Result<(), AgentError> {
        let entry = self.catalog
            .get(agent_id)
            .ok_or_else(|| AgentError::NotFound(agent_id.clone()))?;
        if entry.state == AgentStatus::Stopped {
            return Err(AgentError::InvalidTransition {
                from: AgentStatus::Stopped,
                action: "stop",
            });
        }
        if let Some((_, token)) = self.agent_handles.remove(agent_id) {
            token.cancel();
        }
        if let Some(handle) = self.get(session_id) {
            if let Err(e) = handle.send(AgentMessage::Cancel).await {
                tracing::warn!(session_id = %session_id.as_str(), "cancel send failed on stop: {e}");
            }
        }
        self.remove(session_id);
        self.catalog.update_state(agent_id, AgentStatus::Stopped);
        Ok(())
    }

    /// 暂停 agent：发送 Cancel（中断当前 round），更新 catalog 状态。
    pub async fn pause(&self, agent_id: &AgentId, session_id: &SessionId) -> Result<(), AgentError> {
        let entry = self.catalog
            .get(agent_id)
            .ok_or_else(|| AgentError::NotFound(agent_id.clone()))?;
        if entry.state != AgentStatus::Running {
            return Err(AgentError::InvalidTransition {
                from: entry.state.clone(),
                action: "pause",
            });
        }
        if let Some((_, token)) = self.agent_handles.remove(agent_id) {
            token.cancel();
        }
        if let Some(handle) = self.get(session_id) {
            if let Err(e) = handle.send(AgentMessage::Cancel).await {
                tracing::warn!(session_id = %session_id.as_str(), "cancel send failed on pause: {e}");
            }
        }
        self.catalog.update_state(agent_id, AgentStatus::Paused);
        Ok(())
    }

    /// 恢复 agent：更新 catalog 状态（Runtime 仍在运行，cancel_flag 已重置）。
    pub fn resume(&self, agent_id: &AgentId) -> Result<(), AgentError> {
        let entry = self.catalog
            .get(agent_id)
            .ok_or_else(|| AgentError::NotFound(agent_id.clone()))?;
        if entry.state != AgentStatus::Paused {
            return Err(AgentError::InvalidTransition {
                from: entry.state.clone(),
                action: "resume",
            });
        }
        let cancel_token = CancellationToken::new();
        self.agent_handles.insert(agent_id.clone(), cancel_token);
        self.catalog.update_state(agent_id, AgentStatus::Running);
        Ok(())
    }

    /// 按 agent_id 解析运行时配置（从 catalog 读取 manifest）
    fn resolve_runtime_config(
        &self,
        agent_id: Option<&AgentId>,
    ) -> (Arc<ToolRegistry>, Option<String>, String, AgentConfig) {
        if let Some(id) = agent_id {
            if let Some(entry) = self.catalog.get(id) {
                let manifest = &entry.manifest;
                let tools = self.build_tool_registry(&manifest.tool_filter);
                let system_prompt = Self::build_system_prompt(manifest);
                let model = manifest.model.clone().unwrap_or_else(|| self.default_model.clone());
                let config = manifest.config.clone();
                return (tools, system_prompt, model, config);
            }
        }
        // 无 agent_id 或 agent 不存在：使用全局默认
        (
            self.tools.clone(),
            None,
            self.default_model.clone(),
            AgentConfig::default(),
        )
    }
}
