//! Resilience evaluation suite — tests retry, emergency stop, canary detection,
//! error recovery, and text-tool recovery behaviors.

use std::path::Path;

use anyhow::Result;

use crate::datasets::loader::{load_jsonl, load_jsonl_as_tasks, JsonlTask};
use crate::task::EvalTask;

/// Resilience evaluation suite
pub struct ResilienceSuite;

impl ResilienceSuite {
    const DEFAULT_DATASET: &'static str = "datasets/octo_resilience.jsonl";

    pub fn load() -> Result<Vec<Box<dyn EvalTask>>> {
        let path = Path::new(Self::DEFAULT_DATASET);
        if path.exists() {
            return load_jsonl_as_tasks(path);
        }
        let crate_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(Self::DEFAULT_DATASET);
        if crate_path.exists() {
            return load_jsonl_as_tasks(&crate_path);
        }
        anyhow::bail!(
            "Resilience dataset not found at {} or {}",
            path.display(),
            crate_path.display()
        )
    }

    pub fn load_from(path: &Path) -> Result<Vec<Box<dyn EvalTask>>> {
        load_jsonl_as_tasks(path)
    }

    pub fn load_raw() -> Result<Vec<JsonlTask>> {
        let crate_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(Self::DEFAULT_DATASET);
        load_jsonl(&crate_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_resilience_suite() {
        let tasks = ResilienceSuite::load().unwrap();
        assert_eq!(
            tasks.len(),
            20,
            "Expected 20 resilience tasks, got {}",
            tasks.len()
        );
        assert_eq!(tasks[0].id(), "res-RT-01");
    }

    #[test]
    fn test_load_raw_resilience() {
        let tasks = ResilienceSuite::load_raw().unwrap();
        assert_eq!(tasks.len(), 20);
        assert_eq!(tasks[0].category, "resilience");
    }
}
