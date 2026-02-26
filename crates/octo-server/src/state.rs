use std::sync::Arc;

use octo_engine::{
    MemoryStore, Provider, SessionStore, SkillRegistry, ToolExecutionRecorder, ToolRegistry,
    WorkingMemory,
};

pub struct AppState {
    pub provider: Arc<dyn Provider>,
    pub tools: Arc<ToolRegistry>,
    pub memory: Arc<dyn WorkingMemory>,
    pub sessions: Arc<dyn SessionStore>,
    pub memory_store: Arc<dyn MemoryStore>,
    pub model: Option<String>,
    pub recorder: Option<Arc<ToolExecutionRecorder>>,
    pub skill_registry: Arc<SkillRegistry>,
}

impl AppState {
    pub fn new(
        provider: Arc<dyn Provider>,
        tools: Arc<ToolRegistry>,
        memory: Arc<dyn WorkingMemory>,
        sessions: Arc<dyn SessionStore>,
        memory_store: Arc<dyn MemoryStore>,
        model: Option<String>,
        recorder: Option<Arc<ToolExecutionRecorder>>,
        skill_registry: Arc<SkillRegistry>,
    ) -> Self {
        Self {
            provider,
            tools,
            memory,
            sessions,
            memory_store,
            model,
            recorder,
            skill_registry,
        }
    }
}
