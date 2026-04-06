//! Runtime pool — manages available L1 runtime instances for selection.

use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};

/// A registered runtime instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeEntry {
    pub id: String,
    pub name: String,
    pub endpoint: String,
    pub tier: String,
    pub healthy: bool,
}

/// Pool of available runtime instances.
pub struct RuntimePool {
    entries: Arc<RwLock<Vec<RuntimeEntry>>>,
}

impl RuntimePool {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Register or update a runtime entry.
    pub fn register(&self, entry: RuntimeEntry) {
        let mut entries = self.entries.write().unwrap();
        entries.retain(|e| e.id != entry.id);
        entries.push(entry);
    }

    /// List all registered runtimes.
    pub fn list(&self) -> Vec<RuntimeEntry> {
        self.entries.read().unwrap().clone()
    }

    /// List only healthy runtimes.
    pub fn healthy(&self) -> Vec<RuntimeEntry> {
        self.entries
            .read()
            .unwrap()
            .iter()
            .filter(|e| e.healthy)
            .cloned()
            .collect()
    }

    /// Get a specific runtime by ID.
    pub fn get(&self, id: &str) -> Option<RuntimeEntry> {
        self.entries
            .read()
            .unwrap()
            .iter()
            .find(|e| e.id == id)
            .cloned()
    }

    /// Remove a runtime by ID.
    pub fn remove(&self, id: &str) {
        let mut entries = self.entries.write().unwrap();
        entries.retain(|e| e.id != id);
    }

    /// Mark a runtime as unhealthy.
    pub fn mark_unhealthy(&self, id: &str) {
        let mut entries = self.entries.write().unwrap();
        if let Some(entry) = entries.iter_mut().find(|e| e.id == id) {
            entry.healthy = false;
        }
    }
}

impl Default for RuntimePool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_register_and_list() {
        let pool = RuntimePool::new();
        pool.register(RuntimeEntry {
            id: "grid-harness".into(),
            name: "Grid".into(),
            endpoint: "http://localhost:50051".into(),
            tier: "harness".into(),
            healthy: true,
        });
        pool.register(RuntimeEntry {
            id: "claude-code".into(),
            name: "Claude Code".into(),
            endpoint: "http://localhost:50052".into(),
            tier: "harness".into(),
            healthy: true,
        });
        let all = pool.list();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn pool_healthy_only() {
        let pool = RuntimePool::new();
        pool.register(RuntimeEntry {
            id: "a".into(),
            name: "A".into(),
            endpoint: "x".into(),
            tier: "harness".into(),
            healthy: true,
        });
        pool.register(RuntimeEntry {
            id: "b".into(),
            name: "B".into(),
            endpoint: "y".into(),
            tier: "harness".into(),
            healthy: false,
        });
        let healthy = pool.healthy();
        assert_eq!(healthy.len(), 1);
        assert_eq!(healthy[0].id, "a");
    }

    #[test]
    fn pool_remove_and_mark_unhealthy() {
        let pool = RuntimePool::new();
        pool.register(RuntimeEntry {
            id: "a".into(),
            name: "A".into(),
            endpoint: "x".into(),
            tier: "harness".into(),
            healthy: true,
        });
        pool.register(RuntimeEntry {
            id: "b".into(),
            name: "B".into(),
            endpoint: "y".into(),
            tier: "harness".into(),
            healthy: true,
        });

        pool.mark_unhealthy("a");
        assert!(!pool.get("a").unwrap().healthy);

        pool.remove("b");
        assert_eq!(pool.list().len(), 1);
    }

    #[test]
    fn pool_register_replaces_existing() {
        let pool = RuntimePool::new();
        pool.register(RuntimeEntry {
            id: "a".into(),
            name: "A-old".into(),
            endpoint: "x".into(),
            tier: "harness".into(),
            healthy: true,
        });
        pool.register(RuntimeEntry {
            id: "a".into(),
            name: "A-new".into(),
            endpoint: "y".into(),
            tier: "harness".into(),
            healthy: true,
        });
        let all = pool.list();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "A-new");
    }
}
