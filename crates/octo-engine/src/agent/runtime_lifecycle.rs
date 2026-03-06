//! Agent lifecycle management methods for AgentRuntime (start/stop/pause/resume).

use octo_types::{ChatMessage, SandboxId, SessionId, UserId};

use super::runtime::AgentRuntime;
use super::{AgentError, AgentExecutorHandle, AgentId, AgentMessage, AgentStatus, CancellationToken};

impl AgentRuntime {
    /// 启动 agent：从 catalog 读取 manifest，启动 primary Executor，更新状态机。
    /// session_id：为该 agent 创建或复用的会话标识。
    pub async fn start(
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

        // 启动 primary Runtime
        let handle = self
            .start_primary(
                session_id,
                user_id,
                sandbox_id,
                initial_history,
                Some(agent_id),
            )
            .await;

        Ok(handle)
    }

    /// 停止 agent：发送 Cancel，更新 catalog 状态。
    pub async fn stop(&self, agent_id: &AgentId) -> Result<(), AgentError> {
        let entry = self
            .catalog
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
        let handle = {
            let mut guard = self.primary_handle.lock().await;
            guard.take()
        };
        if let Some(ref h) = handle {
            if let Err(e) = h.send(AgentMessage::Cancel).await {
                tracing::warn!("cancel send failed on stop: {e}");
            }
        }
        self.catalog.update_state(agent_id, AgentStatus::Stopped);
        Ok(())
    }

    /// 暂停 agent：发送 Cancel（中断当前 round），更新 catalog 状态。
    pub async fn pause(&self, agent_id: &AgentId) -> Result<(), AgentError> {
        let entry = self
            .catalog
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
        let handle = {
            let guard = self.primary_handle.lock().await;
            guard.clone()
        };
        if let Some(ref h) = handle {
            if let Err(e) = h.send(AgentMessage::Cancel).await {
                tracing::warn!("cancel send failed on pause: {e}");
            }
        }
        self.catalog.update_state(agent_id, AgentStatus::Paused);
        Ok(())
    }

    /// 恢复 agent：更新 catalog 状态（Runtime 仍在运行，cancel_flag 已重置）。
    pub fn resume(&self, agent_id: &AgentId) -> Result<(), AgentError> {
        let entry = self
            .catalog
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
}
