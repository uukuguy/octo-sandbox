use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::warn;

use super::store::EventStore;
use crate::metrics::MetricsRegistry;

/// octo-engine 内部事件（参考 ARCHITECTURE_DESIGN.md §Phase 2.4）
#[derive(Debug, Clone, serde::Serialize)]
pub enum TelemetryEvent {
    /// Agent Loop 开始新一轮
    LoopTurnStarted { session_id: String, turn: u32 },
    /// 工具调用开始
    ToolCallStarted {
        session_id: String,
        tool_name: String,
    },
    /// 工具调用完成
    ToolCallCompleted {
        session_id: String,
        tool_name: String,
        duration_ms: u64,
    },
    /// 上下文降级触发
    ContextDegraded { session_id: String, level: String },
    /// Loop Guard 触发
    LoopGuardTriggered { session_id: String, reason: String },
    /// Token 预算快照
    TokenBudgetUpdated {
        session_id: String,
        used: u64,
        total: u64,
        ratio: f64,
    },
}

impl TelemetryEvent {
    /// Extract the session_id from any variant.
    pub fn session_id(&self) -> &str {
        match self {
            TelemetryEvent::LoopTurnStarted { session_id, .. }
            | TelemetryEvent::ToolCallStarted { session_id, .. }
            | TelemetryEvent::ToolCallCompleted { session_id, .. }
            | TelemetryEvent::ContextDegraded { session_id, .. }
            | TelemetryEvent::LoopGuardTriggered { session_id, .. }
            | TelemetryEvent::TokenBudgetUpdated { session_id, .. } => session_id,
        }
    }
}

/// 内部事件广播总线
///
/// 设计：broadcast::Sender（1000 容量）+ 环形缓冲区历史（最近 1000 条）
/// 参考：OpenFang openfang-kernel/src/event/bus.rs
pub struct TelemetryBus {
    sender: broadcast::Sender<TelemetryEvent>,
    history: Arc<RwLock<VecDeque<TelemetryEvent>>>,
    history_capacity: usize,
    metrics: Arc<MetricsRegistry>,
    /// Optional persistent event store. When set, every published event
    /// is also appended to the store for replay and projection support.
    event_store: Option<Arc<EventStore>>,
}

impl TelemetryBus {
    pub fn new(
        channel_capacity: usize,
        history_capacity: usize,
        metrics: Arc<MetricsRegistry>,
    ) -> Self {
        let (sender, _) = broadcast::channel(channel_capacity);
        Self {
            sender,
            history: Arc::new(RwLock::new(VecDeque::with_capacity(history_capacity))),
            history_capacity,
            metrics,
            event_store: None,
        }
    }

    /// Attach a persistent EventStore (opt-in).
    ///
    /// When set, every `publish()` call also appends the event to the store.
    /// This is backward compatible -- callers that never set a store are
    /// unaffected.
    pub fn with_event_store(mut self, store: Arc<EventStore>) -> Self {
        self.event_store = Some(store);
        self
    }

    /// Set the event store after construction.
    pub fn set_event_store(&mut self, store: Arc<EventStore>) {
        self.event_store = Some(store);
    }

    /// 发布事件（fire-and-forget，不阻塞发送方）
    pub async fn publish(&self, event: TelemetryEvent) {
        // 记录指标
        self.record_metrics(&event);

        // Persist to event store if configured
        if let Some(store) = &self.event_store {
            let (event_type, session_id) = event_metadata(&event);
            let payload = serde_json::to_value(&event).unwrap_or(serde_json::Value::Null);
            if let Err(e) = store
                .append(
                    &event_type,
                    payload,
                    session_id.as_deref(),
                    session_id.as_deref(),
                    None,
                )
                .await
            {
                warn!(error = %e, "Failed to persist event to EventStore");
            }
        }

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

    /// 根据事件类型记录指标
    fn record_metrics(&self, event: &TelemetryEvent) {
        match event {
            TelemetryEvent::ToolCallCompleted {
                tool_name: _,
                duration_ms,
                ..
            } => {
                self.metrics.counter("octo.tools.executions.total").inc();
                self.metrics
                    .histogram(
                        "octo.tools.executions.duration_ms",
                        vec![
                            10.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0, 10000.0,
                        ],
                    )
                    .observe(*duration_ms as f64);
            }
            TelemetryEvent::LoopTurnStarted { turn, .. } => {
                self.metrics.counter("octo.sessions.turns.total").inc();
                self.metrics
                    .histogram(
                        "octo.sessions.turns.number",
                        vec![1.0, 5.0, 10.0, 20.0, 50.0, 100.0],
                    )
                    .observe(*turn as f64);
            }
            TelemetryEvent::ToolCallStarted { tool_name: _, .. } => {
                self.metrics.counter("octo.tools.calls.started.total").inc();
            }
            TelemetryEvent::ContextDegraded { level: _, .. } => {
                self.metrics
                    .counter("octo.context.degradations.total")
                    .inc();
            }
            TelemetryEvent::LoopGuardTriggered { reason: _, .. } => {
                self.metrics
                    .counter("octo.sessions.guards.triggered.total")
                    .inc();
            }
            TelemetryEvent::TokenBudgetUpdated {
                used, total, ratio, ..
            } => {
                self.metrics
                    .gauge("octo.context.tokens.used")
                    .set(*used as i64);
                self.metrics
                    .gauge("octo.context.tokens.total")
                    .set(*total as i64);
                // ratio is f64, but gauge only supports i64, so we store it as basis points (x10000)
                self.metrics
                    .gauge("octo.context.tokens.ratio")
                    .set((ratio * 10000.0) as i64);
            }
        }
    }

    /// 订阅事件流（每个订阅者独立接收）
    pub fn subscribe(&self) -> broadcast::Receiver<TelemetryEvent> {
        self.sender.subscribe()
    }

    /// 获取最近 N 条历史事件
    pub async fn recent_events(&self, n: usize) -> Vec<TelemetryEvent> {
        let history = self.history.read().await;
        let collected: Vec<_> = history.iter().rev().take(n).cloned().collect();
        collected.into_iter().rev().collect()
    }
}

impl Default for TelemetryBus {
    fn default() -> Self {
        Self::new(1000, 1000, Arc::new(MetricsRegistry::new()))
    }
}

/// Extract event type name and session_id from an TelemetryEvent.
fn event_metadata(event: &TelemetryEvent) -> (String, Option<String>) {
    match event {
        TelemetryEvent::LoopTurnStarted { session_id, .. } => {
            ("LoopTurnStarted".to_string(), Some(session_id.clone()))
        }
        TelemetryEvent::ToolCallStarted { session_id, .. } => {
            ("ToolCallStarted".to_string(), Some(session_id.clone()))
        }
        TelemetryEvent::ToolCallCompleted { session_id, .. } => {
            ("ToolCallCompleted".to_string(), Some(session_id.clone()))
        }
        TelemetryEvent::ContextDegraded { session_id, .. } => {
            ("ContextDegraded".to_string(), Some(session_id.clone()))
        }
        TelemetryEvent::LoopGuardTriggered { session_id, .. } => {
            ("LoopGuardTriggered".to_string(), Some(session_id.clone()))
        }
        TelemetryEvent::TokenBudgetUpdated { session_id, .. } => {
            ("TokenBudgetUpdated".to_string(), Some(session_id.clone()))
        }
    }
}
