pub mod prompts;
pub mod approval;
pub mod bash;
pub mod bash_classifier;
pub mod cast_params;
pub mod plan_mode;
pub mod file_edit;
pub mod file_read;
pub mod file_write;
pub mod find;
pub mod glob;
pub mod grep;
pub mod interceptor;
pub mod knowledge_graph;
pub mod mcp_manage;
pub mod path_safety;
pub mod memory_compress;
pub mod memory_edit;
pub mod memory_forget;
pub mod memory_recall;
pub mod memory_search;
pub mod memory_store;
pub mod memory_timeline;
pub mod memory_update;
pub mod rate_limiter;
pub mod recorder;
pub mod scheduler;
pub mod sleep;
pub mod subagent;
pub mod task;
pub mod team;
pub mod traits;
pub mod truncation;
pub mod web_fetch;
pub mod web_search;

use std::collections::HashMap;
use std::sync::Arc;

pub use interceptor::ToolCallInterceptor;
pub use traits::Tool;

use self::bash::BashTool;
use self::file_edit::FileEditTool;
use self::file_read::FileReadTool;
use self::file_write::FileWriteTool;
use self::find::FindTool;
use self::glob::GlobTool;
use self::grep::GrepTool;
use self::memory_compress::MemoryCompressTool;
use self::memory_forget::MemoryForgetTool;
use self::memory_recall::MemoryRecallTool;
use self::memory_search::MemorySearchTool;
use self::memory_edit::MemoryEditTool;
use self::memory_store::MemoryStoreTool;
use self::memory_timeline::MemoryTimelineTool;
use self::memory_update::MemoryUpdateTool;
use self::sleep::SleepTool;
use self::web_fetch::WebFetchTool;
use self::web_search::WebSearchTool;
use tokio::sync::RwLock;

use crate::memory::graph::KnowledgeGraph;
use crate::memory::hybrid_query::HybridQueryEngine;
use crate::memory::store_traits::MemoryStore;
use crate::providers::Provider;
use crate::scheduler::SchedulerStorage;
use octo_types::ToolSpec;

pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, tool: impl Tool + 'static) {
        let name = tool.name().to_string();
        self.tools.insert(name, Arc::new(tool));
    }

    /// Register a tool that is already wrapped in an Arc.
    pub fn register_arc(&mut self, name: String, tool: Arc<dyn Tool>) {
        self.tools.insert(name, tool);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn specs(&self) -> Vec<ToolSpec> {
        self.tools.values().map(|t| t.spec()).collect()
    }

    pub fn names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    /// Iterate over all (name, tool) pairs in the registry.
    pub fn iter(&self) -> impl Iterator<Item = (&String, Arc<dyn Tool>)> {
        self.tools.iter().map(|(k, v)| (k, v.clone()))
    }

    /// Create a snapshot (clone) of the current registry.
    /// This replaces the manual clone pattern used in multiple locations.
    pub fn snapshot(&self) -> ToolRegistry {
        let mut registry = ToolRegistry::new();
        for (name, tool) in self.tools.iter() {
            registry.tools.insert(name.clone(), tool.clone());
        }
        registry
    }

    /// Create a filtered snapshot containing only the named tools.
    pub fn snapshot_filtered(&self, filter: &[String]) -> ToolRegistry {
        let mut registry = ToolRegistry::new();
        for name in filter {
            if let Some(tool) = self.tools.get(name) {
                registry.tools.insert(name.clone(), tool.clone());
            }
        }
        registry
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub fn default_tools() -> ToolRegistry {
    default_tools_with_search_priority(&[])
}

/// Create default tool registry with custom web search engine priority.
/// Empty slice means use default priority (Jina → Tavily → DDG).
pub fn default_tools_with_search_priority(search_priority: &[String]) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(BashTool::new());
    registry.register(FileReadTool::new());
    registry.register(FileWriteTool::new());
    registry.register(FileEditTool::new());
    registry.register(GrepTool::new());
    registry.register(GlobTool::new());
    registry.register(FindTool::new());
    registry.register(WebFetchTool::new());
    let search_tool = if search_priority.is_empty() {
        WebSearchTool::new()
    } else {
        WebSearchTool::new().with_priority_strings(search_priority)
    };
    registry.register(search_tool);
    registry.register(SleepTool);
    registry
}

pub fn register_memory_tools(
    registry: &mut ToolRegistry,
    store: Arc<dyn MemoryStore>,
    provider: Arc<dyn Provider>,
) {
    register_memory_tools_with_hybrid(registry, store, provider, None);
}

/// Register memory tools with an optional HybridQueryEngine for enhanced search.
pub fn register_memory_tools_with_hybrid(
    registry: &mut ToolRegistry,
    store: Arc<dyn MemoryStore>,
    provider: Arc<dyn Provider>,
    hybrid_engine: Option<Arc<HybridQueryEngine>>,
) {
    registry.register(MemoryStoreTool::new(store.clone(), provider.clone()));
    let search_tool = match hybrid_engine {
        Some(engine) => MemorySearchTool::with_hybrid_engine(store.clone(), provider.clone(), engine),
        None => MemorySearchTool::new(store.clone(), provider.clone()),
    };
    registry.register(search_tool);
    registry.register(MemoryUpdateTool::new(store.clone()));
    registry.register(MemoryRecallTool::new(store.clone(), provider.clone()));
    registry.register(MemoryForgetTool::new(store.clone()));
    registry.register(MemoryCompressTool::new(store.clone(), provider));
    registry.register(MemoryTimelineTool::new(store));
}

/// Register working memory management tools (memory_edit).
/// Call after register_memory_tools when working memory is available.
pub fn register_working_memory_tools(
    registry: &mut ToolRegistry,
    memory: Arc<dyn crate::memory::WorkingMemory>,
) {
    registry.register(MemoryEditTool::new(memory));
}

pub fn register_kg_tools(
    registry: &mut ToolRegistry,
    kg: Arc<RwLock<KnowledgeGraph>>,
) {
    knowledge_graph::register_kg_tools(registry, kg);
}

pub fn register_scheduler_tools(
    registry: &mut ToolRegistry,
    storage: Arc<dyn SchedulerStorage>,
) {
    registry.register(scheduler::ScheduleTaskTool::new(storage));
}
