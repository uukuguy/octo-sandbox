use std::sync::Arc;

use octo_engine::{AgentLoop, Provider, ToolRegistry, WorkingMemory};

use crate::session::SessionStore;

pub struct AppState {
    pub provider: Arc<dyn Provider>,
    pub tools: Arc<ToolRegistry>,
    pub memory: Arc<dyn WorkingMemory>,
    pub sessions: Arc<dyn SessionStore>,
    pub agent_loop: Arc<AgentLoop>,
}

impl AppState {
    pub fn new(
        provider: Arc<dyn Provider>,
        tools: Arc<ToolRegistry>,
        memory: Arc<dyn WorkingMemory>,
        sessions: Arc<dyn SessionStore>,
        model: Option<String>,
    ) -> Self {
        let mut loop_ = AgentLoop::new(
            provider.clone(),
            tools.clone(),
            memory.clone(),
        );
        if let Some(m) = model {
            loop_ = loop_.with_model(m);
        }
        let agent_loop = Arc::new(loop_);

        Self {
            provider,
            tools,
            memory,
            sessions,
            agent_loop,
        }
    }
}
