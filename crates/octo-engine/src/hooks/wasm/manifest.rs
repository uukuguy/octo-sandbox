//! Plugin manifest parsing.
//!
//! Each WASM hook plugin is packaged as a directory containing a `manifest.yaml`
//! and a `.wasm` component binary. The manifest declares metadata, capabilities,
//! hook points, and failure mode.

use serde::Deserialize;

/// Plugin manifest loaded from `manifest.yaml`.
#[derive(Debug, Clone, Deserialize)]
pub struct PluginManifest {
    /// Unique plugin name (e.g., "my-security-hook").
    pub name: String,
    /// Semantic version string.
    pub version: String,
    /// Human-readable description.
    #[serde(default)]
    pub description: Option<String>,
    /// Author identifier (email or name).
    #[serde(default)]
    pub author: Option<String>,
    /// Relative path to the `.wasm` component file.
    pub wasm: String,
    /// Hook points this plugin handles (e.g., ["PreToolUse", "PostToolUse"]).
    pub hook_points: Vec<String>,
    /// Tool matcher regex or "*" for all tools.
    #[serde(default)]
    pub matcher: Option<String>,
    /// Failure mode: "fail_open" (default) or "fail_closed".
    #[serde(default = "default_failure_mode")]
    pub failure_mode: String,
    /// Requested host capabilities (e.g., ["log", "get-context", "get-secret"]).
    #[serde(default)]
    pub capabilities: Vec<String>,
}

fn default_failure_mode() -> String {
    "fail_open".to_string()
}

impl PluginManifest {
    /// Parse a manifest from YAML string.
    pub fn from_yaml(yaml: &str) -> anyhow::Result<Self> {
        let manifest: Self = serde_yaml::from_str(yaml)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validate manifest fields.
    fn validate(&self) -> anyhow::Result<()> {
        if self.name.is_empty() {
            anyhow::bail!("Plugin name cannot be empty");
        }
        if self.wasm.is_empty() {
            anyhow::bail!("Plugin wasm path cannot be empty");
        }
        if self.hook_points.is_empty() {
            anyhow::bail!("Plugin must declare at least one hook_point");
        }
        if self.failure_mode != "fail_open" && self.failure_mode != "fail_closed" {
            anyhow::bail!(
                "Invalid failure_mode '{}': must be 'fail_open' or 'fail_closed'",
                self.failure_mode
            );
        }
        Ok(())
    }

    /// Check if a capability is requested.
    pub fn has_capability(&self, cap: &str) -> bool {
        self.capabilities.iter().any(|c| c == cap)
    }

    /// Convert failure_mode string to HookFailureMode enum.
    pub fn hook_failure_mode(&self) -> crate::hooks::HookFailureMode {
        if self.failure_mode == "fail_closed" {
            crate::hooks::HookFailureMode::FailClosed
        } else {
            crate::hooks::HookFailureMode::FailOpen
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_manifest() {
        let yaml = r#"
name: test-hook
version: 0.1.0
description: A test hook
wasm: hook.wasm
hook_points:
  - PreToolUse
matcher: "bash"
failure_mode: fail_closed
capabilities:
  - log
  - get-context
"#;
        let manifest = PluginManifest::from_yaml(yaml).unwrap();
        assert_eq!(manifest.name, "test-hook");
        assert_eq!(manifest.version, "0.1.0");
        assert_eq!(manifest.wasm, "hook.wasm");
        assert_eq!(manifest.hook_points, vec!["PreToolUse"]);
        assert_eq!(manifest.matcher, Some("bash".to_string()));
        assert_eq!(manifest.failure_mode, "fail_closed");
        assert!(manifest.has_capability("log"));
        assert!(manifest.has_capability("get-context"));
        assert!(!manifest.has_capability("http-request"));
    }

    #[test]
    fn test_parse_minimal_manifest() {
        let yaml = r#"
name: minimal
version: 0.1.0
wasm: plugin.wasm
hook_points:
  - PostToolUse
"#;
        let manifest = PluginManifest::from_yaml(yaml).unwrap();
        assert_eq!(manifest.name, "minimal");
        assert_eq!(manifest.failure_mode, "fail_open");
        assert!(manifest.capabilities.is_empty());
        assert!(manifest.matcher.is_none());
    }

    #[test]
    fn test_invalid_manifest_no_name() {
        let yaml = r#"
name: ""
version: 0.1.0
wasm: plugin.wasm
hook_points:
  - PreToolUse
"#;
        assert!(PluginManifest::from_yaml(yaml).is_err());
    }

    #[test]
    fn test_invalid_failure_mode() {
        let yaml = r#"
name: test
version: 0.1.0
wasm: plugin.wasm
hook_points:
  - PreToolUse
failure_mode: invalid
"#;
        assert!(PluginManifest::from_yaml(yaml).is_err());
    }
}
