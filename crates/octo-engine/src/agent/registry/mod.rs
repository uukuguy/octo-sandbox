//! Agent Registry - concurrent multi-index store
mod entry;
pub mod lifecycle;
mod store;

pub use entry::{AgentEntry, AgentId, AgentManifest, AgentStatus};
pub use lifecycle::AgentError;
pub use store::AgentStore;

use std::sync::Arc;

use dashmap::DashMap;

use crate::agent::CancellationToken;

pub(crate) struct AgentRuntimeHandle {
    pub cancel_token: CancellationToken,
}

pub struct AgentCatalog {
    pub(crate) by_id: DashMap<AgentId, (AgentEntry, Option<AgentRuntimeHandle>)>,
    by_name: DashMap<String, AgentId>,
    by_tag: DashMap<String, Vec<AgentId>>,
    store: Option<Arc<AgentStore>>,
}

impl AgentCatalog {
    pub fn new() -> Self {
        Self {
            by_id: DashMap::new(),
            by_name: DashMap::new(),
            by_tag: DashMap::new(),
            store: None,
        }
    }

    /// Attach a persistent store to this registry.
    pub fn with_store(mut self, store: Arc<AgentStore>) -> Self {
        self.store = Some(store);
        self
    }

    /// Load persisted entries from store into memory indexes.
    /// Called once at startup after with_store().
    pub fn load_from_store(&self) -> anyhow::Result<usize> {
        if let Some(store) = &self.store {
            let entries = store.load_all()?;
            let count = entries.len();
            for entry in entries {
                let id = entry.id.clone();
                let name = entry.manifest.name.clone();
                let tags = entry.manifest.tags.clone();
                // Insert by_id FIRST (same ordering as register())
                self.by_id.insert(id.clone(), (entry, None));
                self.by_name.insert(name, id.clone());
                for tag in &tags {
                    self.by_tag.entry(tag.clone()).or_default().push(id.clone());
                }
            }
            Ok(count)
        } else {
            Ok(0)
        }
    }

    /// Register a new agent. by_id is written FIRST so that any concurrent
    /// reader that observes the secondary index entries is guaranteed to find
    /// the corresponding by_id entry already present.
    pub fn register(&self, manifest: AgentManifest) -> AgentId {
        let entry = AgentEntry::new(manifest);
        let id = entry.id.clone();
        let name = entry.manifest.name.clone();
        let tags = entry.manifest.tags.clone();
        // Insert into by_id FIRST, then update secondary indexes.
        self.by_id.insert(id.clone(), (entry, None));
        self.by_name.insert(name, id.clone());
        for tag in &tags {
            self.by_tag.entry(tag.clone()).or_default().push(id.clone());
        }
        // Persist to store (fire-and-forget: store error does not fail the operation)
        if let Some(store) = &self.store {
            if let Some(slot) = self.by_id.get(&id) {
                if let Err(e) = store.save(&slot.value().0) {
                    tracing::warn!("AgentStore.save failed for {id}: {e}");
                }
            }
        }
        id
    }

    pub fn get(&self, id: &AgentId) -> Option<AgentEntry> {
        self.by_id.get(id).map(|r| r.value().0.clone())
    }

    pub fn get_by_name(&self, name: &str) -> Option<AgentEntry> {
        self.by_name.get(name).and_then(|id| self.get(id.value()))
    }

    pub fn get_by_tag(&self, tag: &str) -> Vec<AgentEntry> {
        self.by_tag
            .get(tag)
            .map(|ids| ids.value().iter().filter_map(|id| self.get(id)).collect())
            .unwrap_or_default()
    }

    pub fn list_all(&self) -> Vec<AgentEntry> {
        self.by_id.iter().map(|r| r.value().0.clone()).collect()
    }

    /// Unregister an agent. Secondary indexes are removed FIRST so that no
    /// window exists where a secondary index entry points to an already-removed
    /// by_id entry.
    pub fn unregister(&self, id: &AgentId) -> Option<AgentEntry> {
        // Read the name and tags we need to clean up without holding a write lock.
        let (name, tags) = {
            let slot = self.by_id.get(id)?;
            let entry = &slot.value().0;
            (entry.manifest.name.clone(), entry.manifest.tags.clone())
        };
        // Remove secondary indexes first so no dangling references exist.
        self.by_name.remove(&name);
        for tag in &tags {
            if let Some(mut ids) = self.by_tag.get_mut(tag) {
                ids.retain(|i| i != id);
            }
        }
        // Finally remove from by_id and cancel any live handle.
        let result = self.by_id.remove(id).map(|(_, (entry, handle))| {
            if let Some(h) = handle {
                h.cancel_token.cancel();
            }
            entry
        });
        // Persist deletion to store (fire-and-forget)
        if result.is_some() {
            if let Some(store) = &self.store {
                if let Err(e) = store.delete(id) {
                    tracing::warn!("AgentStore.delete failed for {id}: {e}");
                }
            }
        }
        result
    }

    pub fn state(&self, id: &AgentId) -> Option<AgentStatus> {
        self.by_id.get(id).map(|r| r.value().0.state.clone())
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
}

impl Default for AgentCatalog {
    fn default() -> Self {
        Self::new()
    }
}
