//! RuntimeSelector — Mock L3 runtime selection strategy.
//!
//! In production, the L3 governance layer selects which runtime(s) to use.
//! This mock provides three strategies for certifier testing.

use crate::runtime_pool::{RuntimeEntry, RuntimePool};

/// Runtime selection strategy.
pub enum SelectionStrategy {
    /// User explicitly chose a runtime.
    UserPreference(String),
    /// Blindbox: pick 2 random runtimes for comparison.
    Blindbox,
    /// Default: first healthy runtime (cheapest-first ordering is future work BF-D8).
    Default,
}

/// Mock L3 runtime selector.
pub struct RuntimeSelector;

impl RuntimeSelector {
    /// Select runtime(s) based on strategy.
    pub fn select(pool: &RuntimePool, strategy: &SelectionStrategy) -> Vec<RuntimeEntry> {
        let healthy = pool.healthy();
        if healthy.is_empty() {
            return vec![];
        }

        match strategy {
            SelectionStrategy::UserPreference(id) => {
                healthy.into_iter().filter(|e| e.id == *id).collect()
            }
            SelectionStrategy::Blindbox => {
                // Pick up to 2 distinct healthy runtimes
                if healthy.len() >= 2 {
                    vec![healthy[0].clone(), healthy[1].clone()]
                } else {
                    healthy
                }
            }
            SelectionStrategy::Default => {
                // First healthy runtime
                vec![healthy[0].clone()]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_pool::RuntimeEntry;

    fn make_pool() -> RuntimePool {
        let pool = RuntimePool::new();
        pool.register(RuntimeEntry {
            id: "grid".into(),
            name: "Grid".into(),
            endpoint: "http://localhost:50051".into(),
            tier: "harness".into(),
            healthy: true,
        });
        pool.register(RuntimeEntry {
            id: "cc".into(),
            name: "Claude Code".into(),
            endpoint: "http://localhost:50052".into(),
            tier: "harness".into(),
            healthy: true,
        });
        pool
    }

    #[test]
    fn select_user_preference() {
        let pool = make_pool();
        let selected =
            RuntimeSelector::select(&pool, &SelectionStrategy::UserPreference("cc".into()));
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].id, "cc");
    }

    #[test]
    fn select_user_preference_not_found() {
        let pool = make_pool();
        let selected = RuntimeSelector::select(
            &pool,
            &SelectionStrategy::UserPreference("nonexistent".into()),
        );
        assert!(selected.is_empty());
    }

    #[test]
    fn select_blindbox_two() {
        let pool = make_pool();
        let selected = RuntimeSelector::select(&pool, &SelectionStrategy::Blindbox);
        assert_eq!(selected.len(), 2);
    }

    #[test]
    fn select_blindbox_single_runtime() {
        let pool = RuntimePool::new();
        pool.register(RuntimeEntry {
            id: "only".into(),
            name: "Only".into(),
            endpoint: "x".into(),
            tier: "harness".into(),
            healthy: true,
        });
        let selected = RuntimeSelector::select(&pool, &SelectionStrategy::Blindbox);
        assert_eq!(selected.len(), 1);
    }

    #[test]
    fn select_default_first_healthy() {
        let pool = make_pool();
        let selected = RuntimeSelector::select(&pool, &SelectionStrategy::Default);
        assert_eq!(selected.len(), 1);
    }

    #[test]
    fn select_empty_pool() {
        let pool = RuntimePool::new();
        let selected = RuntimeSelector::select(&pool, &SelectionStrategy::Default);
        assert!(selected.is_empty());
    }
}
