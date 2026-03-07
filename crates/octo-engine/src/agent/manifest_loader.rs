//! AgentManifestLoader — scans a directory for YAML agent definitions.
//!
//! Each `.yaml` / `.yml` file in the directory is parsed as an [`AgentYamlDef`]
//! and registered into the provided [`AgentCatalog`]. Duplicate names are skipped
//! with a warning; parse failures are also logged and skipped so a single bad file
//! does not prevent other agents from loading.

use std::path::PathBuf;

use tracing::{debug, info, warn};

use super::catalog::AgentCatalog;
use super::yaml_def::AgentYamlDef;

/// Loads declarative YAML agent definitions from a directory and registers
/// them into an `AgentCatalog`.
pub struct AgentManifestLoader {
    agents_dir: PathBuf,
}

impl AgentManifestLoader {
    /// Create a new loader pointing at `agents_dir`.
    pub fn new(agents_dir: impl Into<PathBuf>) -> Self {
        Self {
            agents_dir: agents_dir.into(),
        }
    }

    /// Scan the directory and register all valid YAML agent definitions.
    ///
    /// Returns the number of agents successfully loaded.
    /// If the directory does not exist, returns `Ok(0)` without error.
    pub fn load_all(&self, catalog: &AgentCatalog) -> anyhow::Result<usize> {
        if !self.agents_dir.is_dir() {
            debug!(dir = %self.agents_dir.display(), "agents_dir does not exist — skipping");
            return Ok(0);
        }

        let mut count = 0;
        let mut seen_names = std::collections::HashSet::new();

        for entry in std::fs::read_dir(&self.agents_dir)?.flatten() {
            let path = entry.path();
            if !Self::is_yaml_file(&path) {
                continue;
            }

            let base_dir = path.parent().unwrap_or(std::path::Path::new("."));
            match AgentYamlDef::from_file(&path)
                .and_then(|def| def.into_manifest(base_dir))
            {
                Ok(manifest) => {
                    if seen_names.contains(&manifest.name) {
                        warn!(
                            name = %manifest.name,
                            path = %path.display(),
                            "Duplicate agent name — skipping"
                        );
                        continue;
                    }
                    seen_names.insert(manifest.name.clone());
                    catalog.register(manifest, None);
                    count += 1;
                }
                Err(e) => {
                    warn!(path = %path.display(), error = %e, "Failed to load agent YAML");
                }
            }
        }

        info!(count, dir = %self.agents_dir.display(), "Loaded agent YAML manifests");
        Ok(count)
    }

    /// Returns `true` if the path is a regular file with `.yaml` or `.yml` extension.
    fn is_yaml_file(path: &std::path::Path) -> bool {
        path.is_file()
            && path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e == "yaml" || e == "yml")
                .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_catalog() -> AgentCatalog {
        AgentCatalog::new()
    }

    #[test]
    fn test_nonexistent_dir_returns_zero() {
        let loader = AgentManifestLoader::new("/tmp/definitely_does_not_exist_octo_test");
        let catalog = make_catalog();
        let count = loader.load_all(&catalog).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_loads_valid_yaml_files() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("coder.yaml"), "name: coder\ntype: coder\n").unwrap();
        fs::write(
            dir.path().join("reviewer.yaml"),
            "name: reviewer\ntype: reviewer\n",
        )
        .unwrap();

        let loader = AgentManifestLoader::new(dir.path());
        let catalog = make_catalog();
        let count = loader.load_all(&catalog).unwrap();
        assert_eq!(count, 2);
        assert_eq!(catalog.len(), 2);
    }

    #[test]
    fn test_skips_invalid_yaml() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("bad.yaml"), "{ not valid yaml: [").unwrap();
        fs::write(dir.path().join("good.yaml"), "name: good-agent\n").unwrap();

        let loader = AgentManifestLoader::new(dir.path());
        let catalog = make_catalog();
        let count = loader.load_all(&catalog).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_skips_duplicate_names() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.yaml"), "name: same\n").unwrap();
        fs::write(dir.path().join("b.yaml"), "name: same\n").unwrap();

        let loader = AgentManifestLoader::new(dir.path());
        let catalog = make_catalog();
        let count = loader.load_all(&catalog).unwrap();
        assert_eq!(count, 1);
        assert_eq!(catalog.len(), 1);
    }

    #[test]
    fn test_ignores_non_yaml_files() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("agent.yaml"), "name: valid\n").unwrap();
        fs::write(dir.path().join("README.md"), "## Agents\n").unwrap();
        fs::write(dir.path().join("config.toml"), "[section]\nkey = 'val'\n").unwrap();

        let loader = AgentManifestLoader::new(dir.path());
        let catalog = make_catalog();
        let count = loader.load_all(&catalog).unwrap();
        assert_eq!(count, 1);
    }
}
