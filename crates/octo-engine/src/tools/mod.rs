pub mod bash;
pub mod file_edit;
pub mod file_read;
pub mod file_write;
pub mod find;
pub mod glob;
pub mod grep;
pub mod memory_forget;
pub mod memory_recall;
pub mod memory_search;
pub mod memory_store;
pub mod memory_update;
pub mod recorder;
pub mod traits;
pub mod web_fetch;
pub mod web_search;

use std::collections::HashMap;
use std::sync::Arc;

pub use traits::Tool;

use self::bash::BashTool;
use self::file_edit::FileEditTool;
use self::file_read::FileReadTool;
use self::file_write::FileWriteTool;
use self::find::FindTool;
use self::glob::GlobTool;
use self::grep::GrepTool;
use self::memory_forget::MemoryForgetTool;
use self::memory_recall::MemoryRecallTool;
use self::memory_search::MemorySearchTool;
use self::memory_store::MemoryStoreTool;
use self::memory_update::MemoryUpdateTool;
use self::web_fetch::WebFetchTool;
use self::web_search::WebSearchTool;
use crate::memory::store_traits::MemoryStore;
use crate::providers::Provider;
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
    let mut registry = ToolRegistry::new();
    registry.register(BashTool::new());
    registry.register(FileReadTool::new());
    registry.register(FileWriteTool::new());
    registry.register(FileEditTool::new());
    registry.register(GrepTool::new());
    registry.register(GlobTool::new());
    registry.register(FindTool::new());
    registry.register(WebFetchTool::new());
    registry.register(WebSearchTool::new());
    registry
}

pub fn register_memory_tools(
    registry: &mut ToolRegistry,
    store: Arc<dyn MemoryStore>,
    provider: Arc<dyn Provider>,
) {
    registry.register(MemoryStoreTool::new(store.clone(), provider.clone()));
    registry.register(MemorySearchTool::new(store.clone(), provider.clone()));
    registry.register(MemoryUpdateTool::new(store.clone()));
    registry.register(MemoryRecallTool::new(store.clone(), provider));
    registry.register(MemoryForgetTool::new(store));
}
