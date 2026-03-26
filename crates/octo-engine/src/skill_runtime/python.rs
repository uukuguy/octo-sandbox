use std::path::PathBuf;
use std::process::Stdio;

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value;
use tokio::fs;
use tokio::process::Command;

use super::traits::{RuntimeType, SkillRuntime};
use super::SkillContext;

/// Python runtime for executing Python scripts in isolated virtual environments.
pub struct PythonRuntime {
    /// Base directory for Python virtual environments.
    venv_base: PathBuf,
    /// Cache of created virtual environments (skill_name -> venv_path).
    venv_cache: tokio::sync::RwLock<std::collections::HashMap<String, PathBuf>>,
    /// Mutex to prevent concurrent venv creation for the same skill (race condition fix).
    venv_creation_lock: tokio::sync::Mutex<()>,
}

impl PythonRuntime {
    /// Create a new Python runtime with the given base directory for venvs.
    pub fn new(venv_base: PathBuf) -> Self {
        Self {
            venv_base,
            venv_cache: tokio::sync::RwLock::new(std::collections::HashMap::new()),
            venv_creation_lock: tokio::sync::Mutex::new(()),
        }
    }

    /// Create a new Python runtime with default temporary directory.
    #[cfg(test)]
    pub async fn new_for_test() -> Result<Self> {
        let temp_dir = std::env::temp_dir()
            .join("octo-python-runtime")
            .join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(&temp_dir).await?;
        Ok(Self::new(temp_dir))
    }

    /// Get or create a virtual environment for the given skill.
    async fn get_venv(&self, skill_name: &str) -> Result<PathBuf> {
        // Check cache first (read-only, no lock needed)
        {
            let cache = self.venv_cache.read().await;
            if let Some(venv_path) = cache.get(skill_name) {
                if venv_path.exists() {
                    return Ok(venv_path.clone());
                }
            }
        }

        // Acquire lock to prevent race condition in venv creation
        let _lock = self.venv_creation_lock.lock().await;

        // Double-check cache after acquiring lock (another task may have created it)
        {
            let cache = self.venv_cache.read().await;
            if let Some(venv_path) = cache.get(skill_name) {
                if venv_path.exists() {
                    return Ok(venv_path.clone());
                }
            }
        }

        // Create new venv
        let venv_path = self.venv_base.join(skill_name);

        // Check if venv already exists
        if !venv_path.exists() {
            // Create parent directories
            if let Some(parent) = venv_path.parent() {
                fs::create_dir_all(parent).await?;
            }

            // Find Python executable (cross-platform: prefer `py` on Windows, `python3` on Unix)
            let python_cmd = Self::find_python_command();

            // Create virtual environment
            let output = Command::new(&python_cmd)
                .args(["-m", "venv", venv_path.to_string_lossy().as_ref()])
                .output()
                .await
                .with_context(|| {
                    format!(
                        "Failed to create Python virtual environment using {}",
                        python_cmd
                    )
                })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("Failed to create venv: {}", stderr);
            }

            // Only cache after successful creation (fixes issue #4)
            let mut cache = self.venv_cache.write().await;
            cache.insert(skill_name.to_string(), venv_path.clone());
        } else {
            // venv already existed, cache it
            let mut cache = self.venv_cache.write().await;
            cache.insert(skill_name.to_string(), venv_path.clone());
        }

        Ok(venv_path)
    }

    /// Find the appropriate Python command for the current platform.
    fn find_python_command() -> String {
        #[cfg(windows)]
        {
            // On Windows, prefer `py` launcher, fall back to `python`
            if std::process::Command::new("py")
                .arg("--version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
            {
                return "py".to_string();
            }
            if std::process::Command::new("python")
                .arg("--version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
            {
                return "python".to_string();
            }
            // Fall back to python3 (some Windows setups have it)
            "python3".to_string()
        }
        #[cfg(not(windows))]
        {
            // On Unix, prefer python3
            "python3".to_string()
        }
    }

    /// Get the Python executable path in the virtual environment.
    fn get_python_exe(venv_path: &std::path::Path) -> PathBuf {
        #[cfg(windows)]
        {
            venv_path.join("Scripts").join("python.exe")
        }
        #[cfg(not(windows))]
        {
            venv_path.join("bin").join("python")
        }
    }

    /// Write the skill script to a temporary file and get its path.
    async fn write_script(&self, script: &str, context: &SkillContext) -> Result<PathBuf> {
        let script_name = format!("{}.py", context.skill_name);
        let script_path = context.working_dir.join(&script_name);

        // Write the script to the working directory
        fs::write(&script_path, script)
            .await
            .context("Failed to write skill script")?;

        Ok(script_path)
    }

    /// Execute a Python script with the given arguments.
    async fn execute_script(
        &self,
        script_path: &PathBuf,
        args: Value,
        context: &SkillContext,
    ) -> Result<Value> {
        let venv_path = self.get_venv(&context.skill_name).await?;
        let python_exe = Self::get_python_exe(&venv_path);

        // Serialize args to JSON string for command line
        let args_json = serde_json::to_string(&args)?;

        // Execute the Python script
        // We pass args via stdin to avoid shell escaping issues
        let mut cmd = Command::new(&python_exe);
        cmd.arg(script_path)
            .arg("--args")
            .arg(args_json)
            .current_dir(&context.working_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = cmd
            .output()
            .await
            .with_context(|| format!("Failed to execute Python script: {:?}", script_path))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Return the error from the script, don't mask it by parsing stdout
            anyhow::bail!("Python script failed: {}", stderr);
        }

        // Parse stdout as JSON result
        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim().is_empty() {
            return Ok(Value::Null);
        }

        // Try to parse as JSON
        serde_json::from_str(&stdout).context("Failed to parse Python script output as JSON")
    }
}

#[async_trait]
impl SkillRuntime for PythonRuntime {
    fn runtime_type(&self) -> RuntimeType {
        RuntimeType::Python
    }

    async fn execute(&self, script: &str, args: Value, context: &SkillContext) -> Result<Value> {
        // First check Python environment
        self.check_environment().await?;

        // Write the script to a temporary file
        let script_path = self.write_script(script, context).await?;

        // Execute the script
        let result = self.execute_script(&script_path, args, context).await;

        // Clean up the script file
        let _ = fs::remove_file(&script_path).await;

        result
    }

    async fn check_environment(&self) -> Result<()> {
        // Find Python command (cross-platform)
        let python_cmd = Self::find_python_command();

        // Check if Python is available
        let output = Command::new(&python_cmd)
            .arg("--version")
            .output()
            .await
            .with_context(|| format!("Python not found (tried: {})", python_cmd))?;

        if !output.status.success() {
            anyhow::bail!("Python not found (tried: {})", python_cmd);
        }

        let version = String::from_utf8_lossy(&output.stdout);
        tracing::info!(
            "Python runtime check passed: {} (using {})",
            version.trim(),
            python_cmd
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skill_runtime::ToolInfo;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_python_runtime_check_environment() {
        let temp_dir = tempdir().unwrap();
        let runtime = PythonRuntime::new(temp_dir.path().to_path_buf());

        // This should pass if Python 3 is installed
        let result = runtime.check_environment().await;
        if result.is_err() {
            // Skip test if Python is not installed
            println!("Python not installed, skipping test");
            return;
        }
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_python_execution_basic() {
        let temp_dir = tempdir().unwrap();
        let runtime = PythonRuntime::new(temp_dir.path().to_path_buf());

        // Check if Python is available
        if runtime.check_environment().await.is_err() {
            println!("Python not installed, skipping test");
            return;
        }

        // Create a simple script that prints JSON
        let script = r#"
import sys
import json

# Read args from command line
args_json = sys.argv[2] if len(sys.argv) > 2 else '{}'
args = json.loads(args_json)

# Return a simple result
result = {"status": "success", "message": "Hello from Python", "input": args}
print(json.dumps(result))
"#;

        let context = SkillContext::new("test_skill".to_string(), temp_dir.path().to_path_buf());
        let args = serde_json::json!({"name": "test"});

        let result = runtime.execute(script, args, &context).await;
        assert!(result.is_ok());

        let result_value = result.unwrap();
        assert_eq!(result_value["status"], "success");
        assert_eq!(result_value["message"], "Hello from Python");
    }

    #[tokio::test]
    async fn test_python_execution_with_tools() {
        let temp_dir = tempdir().unwrap();
        let runtime = PythonRuntime::new(temp_dir.path().to_path_buf());

        // Check if Python is available
        if runtime.check_environment().await.is_err() {
            println!("Python not installed, skipping test");
            return;
        }

        let script = r#"
import sys
import json

args_json = sys.argv[2] if len(sys.argv) > 2 else '{}'
args = json.loads(args_json)

result = {"echo": args.get("message", "no message")}
print(json.dumps(result))
"#;

        let tools = vec![ToolInfo {
            name: "echo".to_string(),
            description: "Echo back the message".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "message": {"type": "string"}
                }
            }),
        }];

        let context = SkillContext::new("echo_skill".to_string(), temp_dir.path().to_path_buf())
            .with_tools(tools);
        let args = serde_json::json!({"message": "hello world"});

        let result = runtime.execute(script, args, &context).await;
        assert!(result.is_ok());

        let result_value = result.unwrap();
        assert_eq!(result_value["echo"], "hello world");
    }

    #[tokio::test]
    async fn test_python_execution_error_handling() {
        let temp_dir = tempdir().unwrap();
        let runtime = PythonRuntime::new(temp_dir.path().to_path_buf());

        // Check if Python is available
        if runtime.check_environment().await.is_err() {
            println!("Python not installed, skipping test");
            return;
        }

        // Script that raises an error
        let script = r#"
import sys
import json

raise ValueError("Test error")
"#;

        let context = SkillContext::new("error_skill".to_string(), temp_dir.path().to_path_buf());
        let args = Value::Null;

        let result = runtime.execute(script, args, &context).await;
        // Should fail because of the error in the script
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_python_runtime_type() {
        let temp_dir = tempdir().unwrap();
        let runtime = PythonRuntime::new(temp_dir.path().to_path_buf());
        assert_eq!(runtime.runtime_type(), RuntimeType::Python);
    }
}
