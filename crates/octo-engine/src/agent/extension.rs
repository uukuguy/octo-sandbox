//! Agent Extension System - Event-driven hooks for agent loop
//!
//! Provides extension points for monitoring and intercepting agent behavior

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Extension event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExtensionEvent {
    /// Agent turn started
    TurnStart {
        round: u32,
        max_rounds: u32,
    },
    /// Agent turn ended
    TurnEnd {
        round: u32,
        stop_reason: String,
    },
    /// Tool call started
    ToolCallStart {
        tool_name: String,
        input: serde_json::Value,
    },
    /// Tool call ended
    ToolCallEnd {
        tool_name: String,
        success: bool,
        duration_ms: u64,
    },
    /// Error occurred
    Error {
        error: String,
        round: u32,
    },
}

/// Agent extension trait for handling events
#[async_trait]
pub trait AgentExtension: Send + Sync {
    /// Extension name
    fn name(&self) -> &str;

    /// Handle an extension event
    async fn on_event(&self, event: ExtensionEvent);
}

/// Extension registry for managing extensions
#[derive(Default)]
pub struct ExtensionRegistry {
    extensions: Vec<Arc<dyn AgentExtension>>,
}

impl ExtensionRegistry {
    /// Create a new registry
    pub fn new() -> Self {
        Self {
            extensions: Vec::new(),
        }
    }

    /// Register an extension
    pub fn register(&mut self, ext: Arc<dyn AgentExtension>) {
        self.extensions.push(ext);
    }

    /// Emit an event to all extensions
    pub async fn emit(&self, event: ExtensionEvent) {
        for ext in &self.extensions {
            ext.on_event(event.clone()).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TestExtension {
        name: String,
        event_count: AtomicUsize,
    }

    #[async_trait]
    impl AgentExtension for TestExtension {
        fn name(&self) -> &str {
            &self.name
        }

        async fn on_event(&self, _event: ExtensionEvent) {
            self.event_count.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn test_extension_registry() {
        let ext1 = Arc::new(TestExtension {
            name: "ext1".to_string(),
            event_count: AtomicUsize::new(0),
        });

        let ext2 = Arc::new(TestExtension {
            name: "ext2".to_string(),
            event_count: AtomicUsize::new(0),
        });

        let mut registry = ExtensionRegistry::new();
        registry.register(ext1.clone());
        registry.register(ext2.clone());

        registry
            .emit(ExtensionEvent::TurnStart {
                round: 1,
                max_rounds: 50,
            })
            .await;

        assert_eq!(ext1.event_count.load(Ordering::SeqCst), 1);
        assert_eq!(ext2.event_count.load(Ordering::SeqCst), 1);
    }
}
