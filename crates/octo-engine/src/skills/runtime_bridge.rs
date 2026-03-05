use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tokio::fs;

use crate::skill_runtime::{PythonRuntime, RuntimeType, SkillContext, SkillRuntime};

/// Bridge between SkillLoader and SkillRuntime.
/// Handles runtime selection based on file extension and script execution.
pub struct SkillRuntimeBridge {
    /// Python runtime instance
    python_runtime: Option<PythonRuntime>,
    /// Additional runtimes (can be extended)
    runtimes: HashMap<RuntimeType, Box<dyn SkillRuntime>>,
}

impl SkillRuntimeBridge {
    /// Create a new bridge with the given Python venv base directory.
    pub fn new(venv_base: PathBuf) -> Self {
        let python_runtime = PythonRuntime::new(venv_base);
        Self {
            python_runtime: Some(python_runtime),
            runtimes: HashMap::new(),
        }
    }

    /// Create a bridge with no runtimes (for testing).
    #[cfg(test)]
    pub fn new_mock() -> Self {
        Self {
            python_runtime: None,
            runtimes: HashMap::new(),
        }
    }

    /// Add a runtime to the bridge.
    #[allow(dead_code)]
    pub fn add_runtime(&mut self, runtime: Box<dyn SkillRuntime>) {
        let rt_type = runtime.runtime_type();
        self.runtimes.insert(rt_type, runtime);
    }

    /// Private helper to map file extension to RuntimeType.
    /// Returns None for unknown extensions.
    fn runtime_type_from_ext(ext: &str) -> Option<RuntimeType> {
        match ext.to_lowercase().as_str() {
            "py" | "python" => Some(RuntimeType::Python),
            "js" | "mjs" | "node" => Some(RuntimeType::NodeJS),
            "wasm" => Some(RuntimeType::WASM),
            "rs" | "builtin" => Some(RuntimeType::Builtin),
            _ => None,
        }
    }

    /// Get runtime for a given file extension.
    /// Returns None if the runtime is not available.
    pub fn get_runtime_for_extension(&self, ext: &str) -> Option<&dyn SkillRuntime> {
        match Self::runtime_type_from_ext(ext) {
            Some(RuntimeType::Python) => {
                self.python_runtime.as_ref().map(|r| r as &dyn SkillRuntime)
            }
            Some(rt) => self.runtimes.get(&rt).map(|r| r.as_ref()),
            None => None,
        }
    }

    /// Get runtime for a given RuntimeType.
    #[allow(dead_code)]
    pub fn get_runtime(&self, runtime_type: RuntimeType) -> Option<&dyn SkillRuntime> {
        match runtime_type {
            RuntimeType::Python => self.python_runtime.as_ref().map(|r| r as &dyn SkillRuntime),
            rt => self.runtimes.get(&rt).map(|r| r.as_ref()),
        }
    }

    /// Detect runtime type from file extension.
    pub fn detect_runtime_type(ext: &str) -> Option<RuntimeType> {
        Self::runtime_type_from_ext(ext)
    }

    /// Execute a script file with the given arguments and context.
    pub async fn execute_script_file(
        &self,
        script_path: &Path,
        args: serde_json::Value,
        context: &SkillContext,
    ) -> Result<serde_json::Value> {
        // Detect runtime from extension
        let ext = script_path
            .extension()
            .and_then(|e| e.to_str())
            .context("Script has no file extension")?;

        let runtime = self
            .get_runtime_for_extension(ext)
            .with_context(|| format!("No runtime available for extension: {}", ext))?;

        // Check environment first
        runtime.check_environment().await?;

        // Read script content
        let script = fs::read_to_string(script_path)
            .await
            .with_context(|| format!("Failed to read script: {}", script_path.display()))?;

        // Execute with the runtime
        runtime.execute(&script, args, context).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_detect_runtime_type() {
        assert_eq!(
            SkillRuntimeBridge::detect_runtime_type("py"),
            Some(RuntimeType::Python)
        );
        assert_eq!(
            SkillRuntimeBridge::detect_runtime_type("PY"),
            Some(RuntimeType::Python)
        );
        assert_eq!(
            SkillRuntimeBridge::detect_runtime_type("js"),
            Some(RuntimeType::NodeJS)
        );
        assert_eq!(
            SkillRuntimeBridge::detect_runtime_type("wasm"),
            Some(RuntimeType::WASM)
        );
        assert_eq!(SkillRuntimeBridge::detect_runtime_type("unknown"), None);
    }

    #[test]
    fn test_bridge_new_mock() {
        let bridge = SkillRuntimeBridge::new_mock();
        // No runtimes available in mock
        assert!(bridge.get_runtime_for_extension("py").is_none());
    }

    #[test]
    fn test_bridge_with_python_runtime() {
        let temp_dir = tempdir().unwrap();
        let bridge = SkillRuntimeBridge::new(temp_dir.path().to_path_buf());

        // Python runtime should be available
        assert!(bridge.get_runtime(RuntimeType::Python).is_some());
        assert!(bridge.get_runtime_for_extension("py").is_some());
    }
}
