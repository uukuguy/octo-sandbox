//! Plugin discovery and loading.
//!
//! Scans plugin directories for valid plugin packages (directories containing
//! `manifest.yaml` + `.wasm` file) and loads them.

use std::path::{Path, PathBuf};

use super::manifest::PluginManifest;

/// A discovered plugin with its manifest and resolved wasm path.
#[derive(Debug, Clone)]
pub struct DiscoveredPlugin {
    /// Parsed manifest.
    pub manifest: PluginManifest,
    /// Absolute path to the .wasm component file.
    pub wasm_path: PathBuf,
    /// Directory containing the plugin.
    pub plugin_dir: PathBuf,
}

/// Discover all valid plugins from a list of plugin directories.
///
/// Each directory is expected to contain subdirectories, each with a
/// `manifest.yaml` and the referenced `.wasm` file.
///
/// Directories are scanned in order; later directories take precedence
/// (project-level plugins override global plugins with the same name).
pub fn discover_plugins(dirs: &[PathBuf]) -> Vec<DiscoveredPlugin> {
    let mut plugins = Vec::new();
    let mut seen_names = std::collections::HashSet::new();

    // Reverse iteration so later dirs (project-level) override earlier (global)
    for dir in dirs.iter().rev() {
        if !dir.is_dir() {
            tracing::debug!("Plugin directory does not exist: {}", dir.display());
            continue;
        }

        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) => {
                tracing::warn!("Failed to read plugin directory {}: {}", dir.display(), e);
                continue;
            }
        };

        for entry in entries.flatten() {
            let plugin_dir = entry.path();
            if !plugin_dir.is_dir() {
                continue;
            }

            match load_plugin_from_dir(&plugin_dir) {
                Ok(plugin) => {
                    if seen_names.contains(&plugin.manifest.name) {
                        tracing::debug!(
                            "Plugin '{}' already loaded, skipping {}",
                            plugin.manifest.name,
                            plugin_dir.display()
                        );
                        continue;
                    }
                    tracing::info!(
                        "Discovered plugin '{}' v{} at {}",
                        plugin.manifest.name,
                        plugin.manifest.version,
                        plugin_dir.display()
                    );
                    seen_names.insert(plugin.manifest.name.clone());
                    plugins.push(plugin);
                }
                Err(e) => {
                    tracing::debug!(
                        "Skipping {}: {}",
                        plugin_dir.display(),
                        e
                    );
                }
            }
        }
    }

    plugins
}

/// Load a single plugin from its directory.
fn load_plugin_from_dir(plugin_dir: &Path) -> anyhow::Result<DiscoveredPlugin> {
    let manifest_path = plugin_dir.join("manifest.yaml");
    if !manifest_path.is_file() {
        anyhow::bail!("No manifest.yaml found");
    }

    let yaml = std::fs::read_to_string(&manifest_path)?;
    let manifest = PluginManifest::from_yaml(&yaml)?;

    let wasm_path = plugin_dir.join(&manifest.wasm);
    if !wasm_path.is_file() {
        anyhow::bail!(
            "WASM file '{}' not found in {}",
            manifest.wasm,
            plugin_dir.display()
        );
    }

    Ok(DiscoveredPlugin {
        manifest,
        wasm_path,
        plugin_dir: plugin_dir.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_test_plugin(dir: &Path, name: &str, wasm_content: &[u8]) {
        let plugin_dir = dir.join(name);
        fs::create_dir_all(&plugin_dir).unwrap();

        let manifest = format!(
            r#"name: {name}
version: 0.1.0
wasm: hook.wasm
hook_points:
  - PreToolUse
"#
        );
        fs::write(plugin_dir.join("manifest.yaml"), manifest).unwrap();
        fs::write(plugin_dir.join("hook.wasm"), wasm_content).unwrap();
    }

    #[test]
    fn test_discover_plugins_from_directory() {
        let tmp = tempfile::tempdir().unwrap();
        create_test_plugin(tmp.path(), "plugin-a", b"wasm-a");
        create_test_plugin(tmp.path(), "plugin-b", b"wasm-b");

        let plugins = discover_plugins(&[tmp.path().to_path_buf()]);
        assert_eq!(plugins.len(), 2);

        let names: Vec<&str> = plugins.iter().map(|p| p.manifest.name.as_str()).collect();
        assert!(names.contains(&"plugin-a"));
        assert!(names.contains(&"plugin-b"));
    }

    #[test]
    fn test_discover_plugins_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let plugins = discover_plugins(&[tmp.path().to_path_buf()]);
        assert!(plugins.is_empty());
    }

    #[test]
    fn test_discover_plugins_nonexistent_dir() {
        let plugins = discover_plugins(&[PathBuf::from("/nonexistent/plugin/dir")]);
        assert!(plugins.is_empty());
    }

    #[test]
    fn test_discover_plugins_missing_wasm() {
        let tmp = tempfile::tempdir().unwrap();
        let plugin_dir = tmp.path().join("broken");
        fs::create_dir_all(&plugin_dir).unwrap();
        fs::write(
            plugin_dir.join("manifest.yaml"),
            "name: broken\nversion: 0.1.0\nwasm: missing.wasm\nhook_points:\n  - PreToolUse\n",
        )
        .unwrap();

        let plugins = discover_plugins(&[tmp.path().to_path_buf()]);
        assert!(plugins.is_empty());
    }

    #[test]
    fn test_project_overrides_global() {
        let global = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        create_test_plugin(global.path(), "shared", b"global-wasm");
        create_test_plugin(project.path(), "shared", b"project-wasm");

        // Project dir comes after global, so it takes precedence
        let plugins = discover_plugins(&[global.path().to_path_buf(), project.path().to_path_buf()]);
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].manifest.name, "shared");
        // Project-level should win (later dirs override)
        assert!(plugins[0].plugin_dir.starts_with(project.path()));
    }
}
