//! Platform security evaluation suite — tests SecurityPolicy, AutonomyLevel,
//! path traversal defense, and rate limiting behaviors.

use std::path::Path;

use anyhow::Result;

use crate::datasets::loader::{load_jsonl, load_jsonl_as_tasks, JsonlTask};
use crate::task::EvalTask;

/// Platform security evaluation suite
pub struct PlatformSecuritySuite;

impl PlatformSecuritySuite {
    /// Default dataset path (relative to crate root)
    const DEFAULT_DATASET: &'static str = "datasets/octo_platform_security.jsonl";

    /// Load tasks from the default dataset
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
            "Platform security dataset not found at {} or {}",
            path.display(),
            crate_path.display()
        )
    }

    /// Load tasks from a custom path
    pub fn load_from(path: &Path) -> Result<Vec<Box<dyn EvalTask>>> {
        load_jsonl_as_tasks(path)
    }

    /// Load raw JsonlTask structs (useful for inspection)
    pub fn load_raw() -> Result<Vec<JsonlTask>> {
        let crate_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(Self::DEFAULT_DATASET);
        load_jsonl(&crate_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_platform_security_suite() {
        let tasks = PlatformSecuritySuite::load().unwrap();
        assert!(
            tasks.len() >= 15,
            "Expected at least 15 platform security tasks, got {}",
            tasks.len()
        );
        assert_eq!(tasks[0].id(), "psec-01");
    }

    #[test]
    fn test_load_raw() {
        let tasks = PlatformSecuritySuite::load_raw().unwrap();
        assert!(tasks.len() >= 15);
        assert_eq!(tasks[0].category, "platform_security");
    }
}
