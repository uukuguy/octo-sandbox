use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::SkillContext;

/// Runtime type for skill execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeType {
    /// Python runtime
    Python,
    /// WebAssembly runtime
    WASM,
    /// Node.js runtime
    NodeJS,
    /// Built-in runtime (native Rust)
    Builtin,
}

impl std::fmt::Display for RuntimeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeType::Python => write!(f, "Python"),
            RuntimeType::WASM => write!(f, "WASM"),
            RuntimeType::NodeJS => write!(f, "NodeJS"),
            RuntimeType::Builtin => write!(f, "Builtin"),
        }
    }
}

/// Runtime trait for skill execution.
///
/// This trait defines the interface for executing skills written in
/// different programming languages (Python, WASM, Node.js, etc.).
#[async_trait]
pub trait SkillRuntime: Send + Sync {
    /// Returns the type of runtime.
    fn runtime_type(&self) -> RuntimeType;

    /// Execute a skill script with the given arguments and context.
    async fn execute(
        &self,
        script: &str,
        args: serde_json::Value,
        context: &SkillContext,
    ) -> Result<serde_json::Value>;

    /// Check if the runtime environment is properly configured.
    async fn check_environment(&self) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A mock runtime for testing purposes.
    #[derive(Debug)]
    pub struct MockRuntime {
        runtime_type: RuntimeType,
    }

    impl MockRuntime {
        pub fn new(runtime_type: RuntimeType) -> Self {
            Self { runtime_type }
        }
    }

    #[async_trait]
    impl SkillRuntime for MockRuntime {
        fn runtime_type(&self) -> RuntimeType {
            self.runtime_type
        }

        async fn execute(
            &self,
            _script: &str,
            args: serde_json::Value,
            _context: &SkillContext,
        ) -> Result<serde_json::Value> {
            Ok(args)
        }

        async fn check_environment(&self) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_runtime_type_display() {
        assert_eq!(format!("{}", RuntimeType::Python), "Python");
        assert_eq!(format!("{}", RuntimeType::WASM), "WASM");
        assert_eq!(format!("{}", RuntimeType::NodeJS), "NodeJS");
        assert_eq!(format!("{}", RuntimeType::Builtin), "Builtin");
    }

    #[tokio::test]
    async fn test_mock_runtime_execute() {
        let runtime = MockRuntime::new(RuntimeType::Python);
        assert_eq!(runtime.runtime_type(), RuntimeType::Python);

        let args = serde_json::json!({"key": "value"});
        let context = SkillContext::new("test".to_string(), PathBuf::from("/tmp"));
        let result = runtime
            .execute("print('hello')", args, &context)
            .await
            .unwrap();
        assert_eq!(result, serde_json::json!({"key": "value"}));
    }

    #[tokio::test]
    async fn test_mock_runtime_check_environment() {
        let runtime = MockRuntime::new(RuntimeType::WASM);
        runtime
            .check_environment()
            .await
            .expect("check should pass");
    }
}
