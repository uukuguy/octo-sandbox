use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

/// octo-engine 内部事件（参考 ARCHITECTURE_DESIGN.md §Phase 2.4）
#[derive(Debug, Clone)]
pub enum OctoEvent {
    /// Agent Loop 开始新一轮
    LoopTurnStarted { session_id: String, turn: u32 },
    /// 工具调用开始
    ToolCallStarted { session_id: String, tool_name: String },
    /// 工具调用完成
    ToolCallCompleted { session_id: String, tool_name: String, duration_ms: u64 },
    /// 上下文降级触发
    ContextDegraded { session_id: String, level: String },
    /// Loop Guard 触发
    LoopGuardTriggered { session_id: String, reason: String },
    /// Token 预算快照
    TokenBudgetUpdated { session_id: String, used: u64, total: u64, ratio: f64 },
}

/// 内部事件广播总线
///
/// 设计：broadcast::Sender（1000 容量）+ 环形缓冲区历史（最近 1000 条）
/// 参考：OpenFang openfang-kernel/src/event/bus.rs
pub struct EventBus {
    sender: broadcast::Sender<OctoEvent>,
    history: Arc<RwLock<VecDeque<OctoEvent>>>,
    history_capacity: usize,
}

impl EventBus {
    pub fn new(channel_capacity: usize, history_capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(channel_capacity);
        Self {
            sender,
            history: Arc::new(RwLock::new(VecDeque::with_capacity(history_capacity))),
            history_capacity,
        }
    }

    /// 发布事件（fire-and-forget，不阻塞发送方）
    pub async fn publish(&self, event: OctoEvent) {
        // 存入历史环形缓冲区
        {
            let mut history = self.history.write().await;
            if history.len() >= self.history_capacity {
                history.pop_front();
            }
            history.push_back(event.clone());
        }
        // 广播给订阅者（忽略无订阅者的错误）
        let _ = self.sender.send(event);
    }

    /// 订阅事件流（每个订阅者独立接收）
    pub fn subscribe(&self) -> broadcast::Receiver<OctoEvent> {
        self.sender.subscribe()
    }

    /// 获取最近 N 条历史事件
    pub async fn recent_events(&self, n: usize) -> Vec<OctoEvent> {
        let history = self.history.read().await;
        let collected: Vec<_> = history.iter().rev().take(n).cloned().collect();
        collected.into_iter().rev().collect()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(1000, 1000)
    }
}
