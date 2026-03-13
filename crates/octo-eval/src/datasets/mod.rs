//! Dataset loaders for evaluation task sets (JSONL format).
pub mod loader;

pub use loader::{load_jsonl, load_jsonl_as_tasks, JsonlTask};
