//! OctoRoot — unified directory management for octo-sandbox.
//!
//! Provides a single source of truth for all file paths used by the system:
//! - Global root: `~/.octo/` (user-level data, API keys, per-project databases)
//! - Project root: `$PWD/.octo/` (declarative configs, project skills, can be git-tracked)
//!
//! # Directory Layout
//!
//! ```text
//! ~/.octo/                              OCTO_GLOBAL_ROOT
//! ├── config.yaml                       Global config (API keys, default provider)
//! ├── skills/                           Global skills
//! ├── cache/                            Cache
//! └── projects/                         Per-project data (path-isolated)
//!     └── _Users_foo_myproject/         path encoding: / → _
//!         ├── meta.json                 { "path": "/Users/foo/myproject", "created_at": "..." }
//!         ├── octo.db                   SQLite (sessions, memory, tools, audit)
//!         ├── workspace/                Agent output files
//!         └── history/                  Session history/snapshots
//!
//! $PWD/.octo/                           OCTO_PROJECT_ROOT (declarative, git-trackable)
//! ├── config.yaml                       Project config overrides
//! └── skills/                           Project skills
//! ```

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Unified directory manager for octo-sandbox.
#[derive(Debug, Clone)]
pub struct OctoRoot {
    /// Global root directory (default: `~/.octo`)
    global_root: PathBuf,
    /// Project root directory (default: `$PWD/.octo`)
    project_root: PathBuf,
    /// Working directory for tool execution (default: `$PWD`)
    working_dir: PathBuf,
    /// Encoded project key for per-project isolation under global root
    project_key: String,
}

impl OctoRoot {
    /// Auto-discover OctoRoot from current environment.
    ///
    /// Uses `dirs::home_dir()` for global root and `std::env::current_dir()` for project root.
    /// Environment variables `OCTO_GLOBAL_ROOT` and `OCTO_PROJECT_ROOT` override defaults.
    pub fn discover() -> Result<Self> {
        let working_dir = std::env::current_dir().context("Failed to get current directory")?;
        Self::with_working_dir(&working_dir)
    }

    /// Create OctoRoot with an explicit working directory.
    pub fn with_working_dir(working_dir: &Path) -> Result<Self> {
        let global_root = if let Ok(env_root) = std::env::var("OCTO_GLOBAL_ROOT") {
            PathBuf::from(env_root)
        } else {
            dirs::home_dir()
                .context("Failed to determine home directory")?
                .join(".octo")
        };

        let project_root = if let Ok(env_root) = std::env::var("OCTO_PROJECT_ROOT") {
            PathBuf::from(env_root)
        } else {
            working_dir.join(".octo")
        };

        let project_key = encode_project_key(working_dir);

        Ok(Self {
            global_root,
            project_root,
            working_dir: working_dir.to_path_buf(),
            project_key,
        })
    }

    // ── Path accessors ──────────────────────────────────────────────

    /// Database path for this project's SQLite storage.
    pub fn db_path(&self) -> PathBuf {
        self.project_data_dir().join("octo.db")
    }

    /// Global skills directory (`~/.octo/skills/`).
    pub fn global_skills_dir(&self) -> PathBuf {
        self.global_root.join("skills")
    }

    /// Project-local skills directory (`$PWD/.octo/skills/`).
    pub fn project_skills_dir(&self) -> PathBuf {
        self.project_root.join("skills")
    }

    /// Agent workspace directory for outputs.
    pub fn workspace_dir(&self) -> PathBuf {
        self.project_data_dir().join("workspace")
    }

    /// Session history directory.
    pub fn history_dir(&self) -> PathBuf {
        self.project_data_dir().join("history")
    }

    /// Global cache directory.
    pub fn cache_dir(&self) -> PathBuf {
        self.global_root.join("cache")
    }

    /// Global configuration file path.
    pub fn global_config(&self) -> PathBuf {
        self.global_root.join("config.yaml")
    }

    /// Project configuration file path.
    pub fn project_config(&self) -> PathBuf {
        self.project_root.join("config.yaml")
    }

    /// Project metadata file (inside global projects dir).
    pub fn project_meta_path(&self) -> PathBuf {
        self.project_data_dir().join("meta.json")
    }

    /// Per-project data directory under global root.
    pub fn project_data_dir(&self) -> PathBuf {
        self.global_root.join("projects").join(&self.project_key)
    }

    /// The working directory (for BashTool execution — stays as `$PWD`).
    pub fn working_dir(&self) -> &Path {
        &self.working_dir
    }

    /// The global root directory.
    pub fn global_root(&self) -> &Path {
        &self.global_root
    }

    /// The project root directory (`$PWD/.octo`).
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// The encoded project key.
    pub fn project_key(&self) -> &str {
        &self.project_key
    }

    /// Skills directories in priority order: [project, global].
    pub fn skills_dirs(&self) -> Vec<PathBuf> {
        vec![self.project_skills_dir(), self.global_skills_dir()]
    }

    // ── Directory management ────────────────────────────────────────

    /// Create all necessary directories and write meta.json.
    pub fn ensure_dirs(&self) -> Result<()> {
        let dirs_to_create = [
            self.project_data_dir(),
            self.workspace_dir(),
            self.history_dir(),
            self.cache_dir(),
            self.global_skills_dir(),
            self.project_skills_dir(),
        ];

        for dir in &dirs_to_create {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("Failed to create directory: {}", dir.display()))?;
        }

        // Write meta.json if it doesn't exist
        let meta_path = self.project_meta_path();
        if !meta_path.exists() {
            let meta = serde_json::json!({
                "path": self.working_dir.to_string_lossy(),
                "created_at": chrono::Utc::now().to_rfc3339(),
            });
            std::fs::write(&meta_path, serde_json::to_string_pretty(&meta)?)
                .with_context(|| format!("Failed to write meta.json: {}", meta_path.display()))?;
        }

        Ok(())
    }

    /// Resolve database path with backward compatibility.
    ///
    /// Priority:
    /// 1. `OCTO_DB_PATH` environment variable
    /// 2. `./data/octo.db` if it exists (server legacy)
    /// 3. `./octo.db` if it exists (CLI legacy)
    /// 4. New default: `~/.octo/projects/<key>/octo.db`
    pub fn resolve_db_path(&self) -> PathBuf {
        // 1. Environment variable override
        if let Ok(env_path) = std::env::var("OCTO_DB_PATH") {
            return PathBuf::from(env_path);
        }

        // 2. Legacy server path
        let legacy_server = self.working_dir.join("data").join("octo.db");
        if legacy_server.exists() {
            return legacy_server;
        }

        // 3. Legacy CLI path
        let legacy_cli = self.working_dir.join("octo.db");
        if legacy_cli.exists() {
            return legacy_cli;
        }

        // 4. New default
        self.db_path()
    }
}

/// Encode a filesystem path into a safe directory name.
///
/// - `/` is replaced with `_`
/// - Leading `/` is stripped
/// - If the result exceeds 200 characters, it is truncated to `first32_sha256first16`
pub fn encode_project_key(path: &Path) -> String {
    let path_str = path.to_string_lossy();
    // Strip leading / and replace remaining / with _
    let encoded = path_str
        .strip_prefix('/')
        .unwrap_or(&path_str)
        .replace('/', "_");

    if encoded.len() > 200 {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(path_str.as_bytes());
        let hash = hex::encode(hasher.finalize());
        let prefix: String = encoded.chars().take(32).collect();
        format!("{}_{}", prefix, &hash[..16])
    } else {
        encoded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_project_key_basic() {
        let key = encode_project_key(Path::new("/Users/foo/myproject"));
        assert_eq!(key, "Users_foo_myproject");
    }

    #[test]
    fn test_encode_project_key_relative() {
        let key = encode_project_key(Path::new("relative/path"));
        assert_eq!(key, "relative_path");
    }

    #[test]
    fn test_encode_project_key_long_path() {
        // Create a path longer than 200 chars
        let long_segment = "a".repeat(100);
        let long_path = format!("/{}/{}/{}", long_segment, long_segment, long_segment);
        let key = encode_project_key(Path::new(&long_path));
        // Should be truncated: 32 chars prefix + _ + 16 chars hash = 49 chars
        assert!(key.len() < 200);
        assert!(key.contains('_'));
    }

    #[test]
    fn test_discover_with_env_override() {
        // Save original values
        let orig_global = std::env::var("OCTO_GLOBAL_ROOT").ok();
        let orig_project = std::env::var("OCTO_PROJECT_ROOT").ok();

        let tmp = tempfile::tempdir().unwrap();
        let global = tmp.path().join("global");
        let project = tmp.path().join("project");

        std::env::set_var("OCTO_GLOBAL_ROOT", &global);
        std::env::set_var("OCTO_PROJECT_ROOT", &project);

        let root = OctoRoot::with_working_dir(tmp.path()).unwrap();
        assert_eq!(root.global_root(), global);
        assert_eq!(root.project_root(), project);

        // Restore
        match orig_global {
            Some(v) => std::env::set_var("OCTO_GLOBAL_ROOT", v),
            None => std::env::remove_var("OCTO_GLOBAL_ROOT"),
        }
        match orig_project {
            Some(v) => std::env::set_var("OCTO_PROJECT_ROOT", v),
            None => std::env::remove_var("OCTO_PROJECT_ROOT"),
        }
    }

    #[test]
    fn test_ensure_dirs_and_meta() {
        let tmp = tempfile::tempdir().unwrap();
        let global = tmp.path().join("global");
        let project = tmp.path().join("project");
        let working = tmp.path().join("work");
        std::fs::create_dir_all(&working).unwrap();

        std::env::set_var("OCTO_GLOBAL_ROOT", &global);
        std::env::set_var("OCTO_PROJECT_ROOT", &project);

        let root = OctoRoot::with_working_dir(&working).unwrap();
        root.ensure_dirs().unwrap();

        // Check directories exist
        assert!(root.project_data_dir().exists());
        assert!(root.workspace_dir().exists());
        assert!(root.history_dir().exists());
        assert!(root.cache_dir().exists());
        assert!(root.global_skills_dir().exists());
        assert!(root.project_skills_dir().exists());

        // Check meta.json
        let meta_path = root.project_meta_path();
        assert!(meta_path.exists());
        let meta: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&meta_path).unwrap()).unwrap();
        assert!(meta["path"].as_str().unwrap().contains("work"));
        assert!(meta["created_at"].is_string());

        // Restore
        std::env::remove_var("OCTO_GLOBAL_ROOT");
        std::env::remove_var("OCTO_PROJECT_ROOT");
    }

    #[test]
    fn test_resolve_db_path_legacy_cli() {
        let tmp = tempfile::tempdir().unwrap();
        let working = tmp.path().join("work");
        std::fs::create_dir_all(&working).unwrap();

        // Create legacy CLI db
        std::fs::write(working.join("octo.db"), b"").unwrap();

        std::env::set_var("OCTO_GLOBAL_ROOT", tmp.path().join("global"));
        std::env::remove_var("OCTO_DB_PATH");

        let root = OctoRoot::with_working_dir(&working).unwrap();
        assert_eq!(root.resolve_db_path(), working.join("octo.db"));

        std::env::remove_var("OCTO_GLOBAL_ROOT");
    }

    #[test]
    fn test_resolve_db_path_legacy_server() {
        let tmp = tempfile::tempdir().unwrap();
        let working = tmp.path().join("work");
        std::fs::create_dir_all(working.join("data")).unwrap();

        // Create legacy server db
        std::fs::write(working.join("data").join("octo.db"), b"").unwrap();

        std::env::set_var("OCTO_GLOBAL_ROOT", tmp.path().join("global"));
        std::env::remove_var("OCTO_DB_PATH");

        let root = OctoRoot::with_working_dir(&working).unwrap();
        assert_eq!(root.resolve_db_path(), working.join("data").join("octo.db"));

        std::env::remove_var("OCTO_GLOBAL_ROOT");
    }

    #[test]
    fn test_resolve_db_path_env_override() {
        let tmp = tempfile::tempdir().unwrap();
        let custom = tmp.path().join("custom.db");

        std::env::set_var("OCTO_DB_PATH", &custom);
        std::env::set_var("OCTO_GLOBAL_ROOT", tmp.path().join("global"));

        let root = OctoRoot::with_working_dir(tmp.path()).unwrap();
        assert_eq!(root.resolve_db_path(), custom);

        std::env::remove_var("OCTO_DB_PATH");
        std::env::remove_var("OCTO_GLOBAL_ROOT");
    }

    #[test]
    fn test_resolve_db_path_new_default() {
        let tmp = tempfile::tempdir().unwrap();
        let working = tmp.path().join("work");
        std::fs::create_dir_all(&working).unwrap();

        let global = tmp.path().join("global");
        std::env::set_var("OCTO_GLOBAL_ROOT", &global);
        std::env::remove_var("OCTO_DB_PATH");

        let root = OctoRoot::with_working_dir(&working).unwrap();
        // No legacy files exist, should return new default
        let expected = global
            .join("projects")
            .join(encode_project_key(&working))
            .join("octo.db");
        assert_eq!(root.resolve_db_path(), expected);

        std::env::remove_var("OCTO_GLOBAL_ROOT");
    }

    #[test]
    fn test_skills_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("OCTO_GLOBAL_ROOT", tmp.path().join("global"));
        std::env::set_var("OCTO_PROJECT_ROOT", tmp.path().join("project"));

        let root = OctoRoot::with_working_dir(tmp.path()).unwrap();
        let dirs = root.skills_dirs();
        assert_eq!(dirs.len(), 2);
        assert!(dirs[0].to_string_lossy().contains("project"));
        assert!(dirs[1].to_string_lossy().contains("global"));

        std::env::remove_var("OCTO_GLOBAL_ROOT");
        std::env::remove_var("OCTO_PROJECT_ROOT");
    }
}
