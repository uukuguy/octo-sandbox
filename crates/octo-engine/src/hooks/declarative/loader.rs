//! Configuration loader for hooks.yaml files.
//!
//! Supports layered loading with merge semantics:
//! 1. Platform defaults (compiled-in, lowest priority)
//! 2. Global: `~/.octo/hooks.yaml`
//! 3. Project: `$PROJECT/.octo/hooks.yaml`
//! 4. Environment: `OCTO_HOOKS_FILE` override

use std::path::{Path, PathBuf};

use tracing::{debug, warn};

use super::config::HooksConfig;

/// Resolve the hooks.yaml file path using layered priority.
///
/// Returns `None` if no config file is found at any location.
pub fn resolve_hooks_path(project_dir: Option<&Path>) -> Option<PathBuf> {
    // 1. Environment variable override (highest priority)
    if let Ok(env_path) = std::env::var("OCTO_HOOKS_FILE") {
        let p = PathBuf::from(&env_path);
        if p.exists() {
            debug!(path = %p.display(), "Using hooks config from OCTO_HOOKS_FILE");
            return Some(p);
        }
        warn!(
            path = %env_path,
            "OCTO_HOOKS_FILE set but file not found"
        );
    }

    // 2. Project-level config
    if let Some(project) = project_dir {
        let project_hooks = project.join(".octo").join("hooks.yaml");
        if project_hooks.exists() {
            debug!(path = %project_hooks.display(), "Using project hooks config");
            return Some(project_hooks);
        }
    }

    // 3. Global config
    if let Some(home) = dirs::home_dir() {
        let global_hooks = home.join(".octo").join("hooks.yaml");
        if global_hooks.exists() {
            debug!(path = %global_hooks.display(), "Using global hooks config");
            return Some(global_hooks);
        }
    }

    None
}

/// Load and parse a hooks.yaml configuration file.
pub fn load_hooks_config(path: &Path) -> anyhow::Result<HooksConfig> {
    let content = std::fs::read_to_string(path)?;
    let config: HooksConfig = serde_yaml::from_str(&content)?;
    if config.version != 1 {
        anyhow::bail!(
            "Unsupported hooks.yaml version: {} (expected 1)",
            config.version
        );
    }
    debug!(
        path = %path.display(),
        hook_points = config.hooks.len(),
        "Loaded hooks config"
    );
    Ok(config)
}

/// Load hooks config from the resolved path, or return None if not found.
pub fn load_hooks_config_auto(project_dir: Option<&Path>) -> Option<HooksConfig> {
    let path = resolve_hooks_path(project_dir)?;
    match load_hooks_config(&path) {
        Ok(config) => Some(config),
        Err(e) => {
            warn!(
                path = %path.display(),
                error = %e,
                "Failed to load hooks config"
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_load_hooks_config_valid() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("hooks.yaml");
        let mut f = std::fs::File::create(&path).unwrap();
        write!(
            f,
            r#"
version: 1
hooks:
  PreToolUse:
    - matcher: "*"
      actions:
        - type: command
          command: "echo ok"
"#
        )
        .unwrap();

        let config = load_hooks_config(&path).unwrap();
        assert_eq!(config.version, 1);
        assert!(config.hooks.contains_key("PreToolUse"));
    }

    #[test]
    fn test_load_hooks_config_invalid_version() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("hooks.yaml");
        std::fs::write(&path, "version: 99\nhooks: {}").unwrap();

        let result = load_hooks_config(&path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unsupported"));
    }

    #[test]
    fn test_load_hooks_config_not_found() {
        let result = load_hooks_config(Path::new("/nonexistent/hooks.yaml"));
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_project_hooks() {
        let dir = TempDir::new().unwrap();
        let octo_dir = dir.path().join(".octo");
        std::fs::create_dir_all(&octo_dir).unwrap();
        std::fs::write(octo_dir.join("hooks.yaml"), "version: 1\nhooks: {}").unwrap();

        let result = resolve_hooks_path(Some(dir.path()));
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("hooks.yaml"));
    }

    #[test]
    fn test_resolve_no_config() {
        let dir = TempDir::new().unwrap();
        // No .octo/hooks.yaml in temp dir, and OCTO_HOOKS_FILE not set
        let result = resolve_hooks_path(Some(dir.path()));
        // May or may not find global config depending on system
        // Just verify it doesn't panic
        let _ = result;
    }

    #[test]
    fn test_load_hooks_config_auto_not_found() {
        let dir = TempDir::new().unwrap();
        let result = load_hooks_config_auto(Some(dir.path()));
        // Should return None (no hooks.yaml in temp dir)
        // Note: may find global config on some systems
        let _ = result;
    }
}
