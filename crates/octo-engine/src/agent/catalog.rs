//! AgentCatalog - concurrent multi-index store for agent definitions

use std::sync::Arc;

use dashmap::DashMap;

use crate::agent::entry::{AgentEntry, AgentId, AgentManifest, AgentStatus};
use crate::agent::store::AgentStore;

pub struct AgentCatalog {
    by_id: DashMap<AgentId, AgentEntry>,
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

    pub fn with_store(mut self, store: Arc<AgentStore>) -> Self {
        self.store = Some(store);
        self
    }

    pub fn load_from_store(&self) -> anyhow::Result<usize> {
        if let Some(store) = &self.store {
            let entries = store.load_all()?;
            let count = entries.len();
            for entry in entries {
                let id = entry.id.clone();
                let name = entry.manifest.name.clone();
                let tags = entry.manifest.tags.clone();
                self.by_id.insert(id.clone(), entry);
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

    pub fn register(&self, manifest: AgentManifest) -> AgentId {
        let entry = AgentEntry::new(manifest);
        let id = entry.id.clone();
        let name = entry.manifest.name.clone();
        let tags = entry.manifest.tags.clone();
        self.by_id.insert(id.clone(), entry.clone());
        self.by_name.insert(name, id.clone());
        for tag in &tags {
            self.by_tag.entry(tag.clone()).or_default().push(id.clone());
        }
        if let Some(store) = &self.store {
            if let Err(e) = store.save(&entry) {
                tracing::warn!("AgentStore.save failed for {id}: {e}");
            }
        }
        id
    }

    pub fn get(&self, id: &AgentId) -> Option<AgentEntry> {
        self.by_id.get(id).map(|r| r.value().clone())
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
        self.by_id.iter().map(|r| r.value().clone()).collect()
    }

    pub fn unregister(&self, id: &AgentId) -> Option<AgentEntry> {
        let entry = {
            let slot = self.by_id.get(id)?;
            slot.value().clone()
        };
        self.by_name.remove(&entry.manifest.name);
        for tag in &entry.manifest.tags {
            if let Some(mut ids) = self.by_tag.get_mut(tag) {
                ids.retain(|i| i != id);
            }
        }
        let removed = self.by_id.remove(id).map(|(_, e)| e);
        if removed.is_some() {
            if let Some(store) = &self.store {
                if let Err(e) = store.delete(id) {
                    tracing::warn!("AgentStore.delete failed for {id}: {e}");
                }
            }
        }
        removed
    }

    /// Update agent state in memory and persist to store.
    /// Called by AgentRuntime on lifecycle transitions.
    pub fn update_state(&self, id: &AgentId, state: AgentStatus) {
        if let Some(mut slot) = self.by_id.get_mut(id) {
            slot.value_mut().state = state.clone();
        }
        if let Some(store) = &self.store {
            if let Err(e) = store.update_state(id, &state) {
                tracing::warn!("AgentStore.update_state failed for {id}: {e}");
            }
        }
    }

    pub fn state(&self, id: &AgentId) -> Option<AgentStatus> {
        self.by_id.get(id).map(|r| r.value().state.clone())
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
