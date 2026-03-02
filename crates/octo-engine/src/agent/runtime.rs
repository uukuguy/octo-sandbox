use tokio::sync::{broadcast, mpsc};

use octo_types::SessionId;

use crate::agent::AgentEvent;

/// Channel → AgentRuntime 的消息
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

/// AgentRuntime 的对外句柄（可 clone，廉价）
#[derive(Clone)]
pub struct AgentRuntimeHandle {
    /// 向 AgentRuntime 发送消息
    pub tx: mpsc::Sender<AgentMessage>,
    /// 订阅 AgentRuntime 的广播事件
    pub broadcast_tx: broadcast::Sender<AgentEvent>,
    /// 关联的 session_id
    pub session_id: SessionId,
}

impl AgentRuntimeHandle {
    /// 创建一个新的广播订阅者
    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent> {
        self.broadcast_tx.subscribe()
    }

    /// 发送消息到 AgentRuntime
    pub async fn send(&self, msg: AgentMessage) -> Result<(), mpsc::error::SendError<AgentMessage>> {
        self.tx.send(msg).await
    }
}
