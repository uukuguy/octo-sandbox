# Phase AA — Octo 部署配置架构 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement layered config loading (global → project → local → env) with OctoRoot integration, credential separation, and hardcoded path fixes.

**Architecture:** Config::load() gains OctoRoot awareness to merge configs from `~/.octo/config.yaml` → `$PWD/.octo/config.yaml` → `$PWD/.octo/config.local.yaml` → env vars. Credentials are loaded from a separate `~/.octo/credentials.yaml` file. Hardcoded paths (`./data/tls`, `./data/certs`) are replaced with OctoRoot methods.

**Tech Stack:** Rust, serde_yaml, OctoRoot (existing)

**Design doc:** `docs/design/DEPLOYMENT_CONFIG_DESIGN.md`

---

## Group 1: OctoRoot Path Extensions (AA-T1)

### Task AA-T1: Add new path methods to OctoRoot

**Files:**
- Modify: `crates/octo-engine/src/root.rs:79-154` (path accessors section)
- Test: `crates/octo-engine/src/root.rs` (inline tests)

**Step 1: Write the failing tests**

Add these tests at the bottom of `mod tests` in `root.rs`:

```rust
#[test]
fn test_project_local_config() {
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("OCTO_GLOBAL_ROOT", tmp.path().join("global"));
    std::env::set_var("OCTO_PROJECT_ROOT", tmp.path().join("project"));
    let root = OctoRoot::with_working_dir(tmp.path()).unwrap();
    assert_eq!(root.project_local_config(), tmp.path().join("project").join("config.local.yaml"));
    std::env::remove_var("OCTO_GLOBAL_ROOT");
    std::env::remove_var("OCTO_PROJECT_ROOT");
}

#[test]
fn test_credentials_path() {
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("OCTO_GLOBAL_ROOT", tmp.path().join("global"));
    let root = OctoRoot::with_working_dir(tmp.path()).unwrap();
    assert_eq!(root.credentials_path(), tmp.path().join("global").join("credentials.yaml"));
    std::env::remove_var("OCTO_GLOBAL_ROOT");
}

#[test]
fn test_tls_dir() {
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("OCTO_GLOBAL_ROOT", tmp.path().join("global"));
    let root = OctoRoot::with_working_dir(tmp.path()).unwrap();
    assert_eq!(root.tls_dir(), tmp.path().join("global").join("tls"));
    std::env::remove_var("OCTO_GLOBAL_ROOT");
}

#[test]
fn test_mcp_dirs() {
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("OCTO_GLOBAL_ROOT", tmp.path().join("global"));
    std::env::set_var("OCTO_PROJECT_ROOT", tmp.path().join("project"));
    let root = OctoRoot::with_working_dir(tmp.path()).unwrap();
    assert_eq!(root.global_mcp_dir(), tmp.path().join("global").join("mcp"));
    assert_eq!(root.project_mcp_dir(), tmp.path().join("project").join("mcp"));
    std::env::remove_var("OCTO_GLOBAL_ROOT");
    std::env::remove_var("OCTO_PROJECT_ROOT");
}

#[test]
fn test_eval_config() {
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("OCTO_PROJECT_ROOT", tmp.path().join("project"));
    let root = OctoRoot::with_working_dir(tmp.path()).unwrap();
    assert_eq!(root.eval_config(), tmp.path().join("project").join("eval.toml"));
    std::env::remove_var("OCTO_PROJECT_ROOT");
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p octo-engine root::tests -- --test-threads=1`
Expected: FAIL — methods not found

**Step 3: Add the new path methods**

Add after the `skills_dirs()` method (line 154) in the path accessors section:

```rust
    /// Project-local override config (git-ignored): `$PWD/.octo/config.local.yaml`.
    pub fn project_local_config(&self) -> PathBuf {
        self.project_root.join("config.local.yaml")
    }

    /// Global credentials file: `~/.octo/credentials.yaml`.
    pub fn credentials_path(&self) -> PathBuf {
        self.global_root.join("credentials.yaml")
    }

    /// Global TLS directory: `~/.octo/tls/`.
    pub fn tls_dir(&self) -> PathBuf {
        self.global_root.join("tls")
    }

    /// Global MCP directory: `~/.octo/mcp/`.
    pub fn global_mcp_dir(&self) -> PathBuf {
        self.global_root.join("mcp")
    }

    /// Project MCP directory: `$PWD/.octo/mcp/`.
    pub fn project_mcp_dir(&self) -> PathBuf {
        self.project_root.join("mcp")
    }

    /// Project eval config: `$PWD/.octo/eval.toml`.
    pub fn eval_config(&self) -> PathBuf {
        self.project_root.join("eval.toml")
    }
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p octo-engine root::tests -- --test-threads=1`
Expected: ALL PASS

**Step 5: Commit**

```bash
git add crates/octo-engine/src/root.rs
git commit -m "feat(root): add path methods for local config, credentials, tls, mcp, eval"
```

---

## Group 2: Layered Config Loading (AA-T2)

### Task AA-T2: Implement three-layer config merge in Config::load()

**Files:**
- Modify: `crates/octo-server/src/config.rs:157-196` (Config::load method)
- Test: `crates/octo-server/tests/config_layered.rs` (new integration test)

**Step 1: Write the failing integration test**

Create `crates/octo-server/tests/config_layered.rs`:

```rust
//! Integration tests for layered config loading.

use std::path::Path;
use tempfile::tempdir;

/// Helper: write a YAML config file.
fn write_yaml(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, content).unwrap();
}

#[test]
fn test_layered_config_merge() {
    let tmp = tempdir().unwrap();
    let global_root = tmp.path().join("global");
    let project_root = tmp.path().join("project");

    // Global config: port 3001, host 0.0.0.0
    write_yaml(
        &global_root.join("config.yaml"),
        "server:\n  host: \"0.0.0.0\"\n  port: 3001\nlogging:\n  level: \"info\"\n",
    );

    // Project config: port 4000 (overrides global)
    write_yaml(
        &project_root.join("config.yaml"),
        "server:\n  port: 4000\n",
    );

    // Local config: logging override
    write_yaml(
        &project_root.join("config.local.yaml"),
        "logging:\n  level: \"debug\"\n",
    );

    std::env::set_var("OCTO_GLOBAL_ROOT", &global_root);
    std::env::set_var("OCTO_PROJECT_ROOT", &project_root);

    let octo_root = octo_engine::OctoRoot::with_working_dir(tmp.path()).unwrap();

    // Load without explicit config path
    let config = octo_server_config::Config::load(None, None, None, &octo_root);

    assert_eq!(config.server.host, "0.0.0.0"); // from global
    assert_eq!(config.server.port, 4000);       // from project (overrides global)
    assert_eq!(config.logging.level, "debug");  // from local (overrides project)

    std::env::remove_var("OCTO_GLOBAL_ROOT");
    std::env::remove_var("OCTO_PROJECT_ROOT");
}

#[test]
fn test_explicit_config_skips_auto_discovery() {
    let tmp = tempdir().unwrap();
    let global_root = tmp.path().join("global");

    // Global config with port 3001
    write_yaml(
        &global_root.join("config.yaml"),
        "server:\n  port: 3001\n",
    );

    // Explicit config with port 9999
    let explicit = tmp.path().join("explicit.yaml");
    write_yaml(&explicit, "server:\n  port: 9999\n");

    std::env::set_var("OCTO_GLOBAL_ROOT", &global_root);
    let octo_root = octo_engine::OctoRoot::with_working_dir(tmp.path()).unwrap();

    let config = octo_server_config::Config::load(Some(&explicit), None, None, &octo_root);

    // Explicit config wins, global NOT merged
    assert_eq!(config.server.port, 9999);

    std::env::remove_var("OCTO_GLOBAL_ROOT");
}

#[test]
fn test_legacy_cwd_config_fallback() {
    let tmp = tempdir().unwrap();
    let global_root = tmp.path().join("global");
    let project_root = tmp.path().join(".octo");

    // No .octo/config.yaml, but legacy config.yaml at CWD
    let legacy_path = tmp.path().join("config.yaml");
    write_yaml(&legacy_path, "server:\n  port: 7777\n");

    std::env::set_var("OCTO_GLOBAL_ROOT", &global_root);
    std::env::set_var("OCTO_PROJECT_ROOT", &project_root);

    let octo_root = octo_engine::OctoRoot::with_working_dir(tmp.path()).unwrap();
    let config = octo_server_config::Config::load(None, None, None, &octo_root);

    // Legacy fallback should pick it up
    assert_eq!(config.server.port, 7777);

    std::env::remove_var("OCTO_GLOBAL_ROOT");
    std::env::remove_var("OCTO_PROJECT_ROOT");
}
```

> **NOTE**: The test references `octo_server_config::Config`. Since `Config` is in `octo-server`'s private `config` module, we need to either:
> (a) make the `config` module `pub` in `octo-server/src/lib.rs`, or
> (b) extract Config to a shared location.
>
> **Decision**: Add a `lib.rs` to octo-server that re-exports `pub mod config;`, since octo-server is a binary crate. This is the minimal change. Add `[[bin]]` and `[lib]` sections to `Cargo.toml` if not already present.

**Step 2: Add a helper function for YAML merging**

Add to `config.rs` before `impl Config`:

```rust
/// Load a YAML config file, returning None if missing or invalid.
fn load_yaml_config(path: &Path) -> Option<Config> {
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(path).ok()?;
    match serde_yaml::from_str::<Config>(&content) {
        Ok(cfg) => Some(cfg),
        Err(e) => {
            tracing::warn!("Failed to parse {}: {}", path.display(), e);
            None
        }
    }
}

/// Merge two configs: `overlay` fields override `base` fields.
/// Uses serde_yaml Value-level merge for field-level overrides.
fn merge_configs(base: Config, overlay: Config) -> Config {
    let base_val = serde_yaml::to_value(&base).unwrap_or(serde_yaml::Value::Null);
    let overlay_val = serde_yaml::to_value(&overlay).unwrap_or(serde_yaml::Value::Null);
    let merged = merge_yaml_values(base_val, overlay_val);
    serde_yaml::from_value(merged).unwrap_or(base)
}

/// Recursively merge two YAML Values. overlay wins for scalars/sequences.
fn merge_yaml_values(base: serde_yaml::Value, overlay: serde_yaml::Value) -> serde_yaml::Value {
    use serde_yaml::Value;
    match (base, overlay) {
        (Value::Mapping(mut base_map), Value::Mapping(overlay_map)) => {
            for (key, overlay_v) in overlay_map {
                let merged = if let Some(base_v) = base_map.remove(&key) {
                    merge_yaml_values(base_v, overlay_v)
                } else {
                    overlay_v
                };
                base_map.insert(key, merged);
            }
            Value::Mapping(base_map)
        }
        // For scalars, sequences, and mismatched types: overlay wins
        (_base, overlay) => overlay,
    }
}
```

**Step 3: Rewrite Config::load() with OctoRoot integration**

Replace the current `Config::load()` method:

```rust
    /// Load configuration with layered priority.
    ///
    /// Priority (lowest to highest):
    /// 1. Code defaults (impl Default)
    /// 2. Global config: `~/.octo/config.yaml`
    /// 3. Project config: `$PWD/.octo/config.yaml`
    /// 4. Project local config: `$PWD/.octo/config.local.yaml`
    /// 5. Legacy fallback: `$PWD/config.yaml` (if no .octo configs found)
    /// 6. CLI arguments: --port, --host
    /// 7. Environment variables: OCTO_*, ANTHROPIC_*, OPENAI_*
    ///
    /// When `explicit_config` is provided (--config flag), it replaces
    /// steps 2-5 entirely (only that file + CLI + env apply).
    pub fn load(
        explicit_config: Option<&PathBuf>,
        cli_port: Option<u16>,
        cli_host: Option<&str>,
        octo_root: &octo_engine::OctoRoot,
    ) -> Self {
        let mut config = if let Some(path) = explicit_config {
            // Explicit --config: use only this file, skip auto-discovery
            load_yaml_config(path).unwrap_or_default()
        } else {
            // Auto-discovery: merge global → project → local
            let mut cfg = Config::default();

            // Layer 1: Global config
            if let Some(global) = load_yaml_config(&octo_root.global_config()) {
                tracing::debug!("Loaded global config: {}", octo_root.global_config().display());
                cfg = merge_configs(cfg, global);
            }

            // Layer 2: Project config
            if let Some(project) = load_yaml_config(&octo_root.project_config()) {
                tracing::debug!("Loaded project config: {}", octo_root.project_config().display());
                cfg = merge_configs(cfg, project);
            }

            // Layer 3: Project local config
            if let Some(local) = load_yaml_config(&octo_root.project_local_config()) {
                tracing::debug!("Loaded local config: {}", octo_root.project_local_config().display());
                cfg = merge_configs(cfg, local);
            }

            // Legacy fallback: $PWD/config.yaml (if no project config was found)
            let legacy_path = octo_root.working_dir().join("config.yaml");
            if !octo_root.project_config().exists() && legacy_path.exists() {
                tracing::warn!(
                    "Found config.yaml at project root (legacy location). \
                     Please move it to .octo/config.yaml: \
                     mv config.yaml .octo/config.yaml"
                );
                if let Some(legacy) = load_yaml_config(&legacy_path) {
                    cfg = merge_configs(cfg, legacy);
                }
            }

            cfg
        };

        // CLI arguments override
        if let Some(port) = cli_port {
            config.server.port = port;
        }
        if let Some(host) = cli_host {
            config.server.host = host.to_string();
        }

        // Environment variables (highest priority) — existing code unchanged
        // ... (keep all existing env var overrides from line 201 onwards)
```

> **IMPORTANT**: Keep all the existing environment variable override code (lines 201-317) exactly as-is. Only replace lines 164-196.

**Step 4: Update all callers to pass OctoRoot**

There are two callers of `Config::load()`:

**(a) `crates/octo-server/src/main.rs:71`**

Change from:
```rust
let cfg = config::Config::load(config_path.as_ref(), cli_port, cli_host);
```
To:
```rust
let cfg = config::Config::load(config_path.as_ref(), cli_port, cli_host, &octo_root);
```

> Note: `octo_root` is already created at line 100. Move the `OctoRoot::discover()` block (lines 99-108) BEFORE `Config::load()` (line 71). This means reordering: discover OctoRoot first, then load config.

**(b) Check for any other callers** with `grep -r "Config::load" crates/` — update them all.

**Step 5: Run tests**

Run: `cargo test --workspace -- --test-threads=1`
Expected: ALL PASS

**Step 6: Commit**

```bash
git add crates/octo-server/src/config.rs crates/octo-server/src/main.rs
git commit -m "feat(config): layered config loading — global → project → local → env"
```

---

## Group 3: Credentials Loading (AA-T3)

### Task AA-T3: Load credentials from ~/.octo/credentials.yaml

**Files:**
- Modify: `crates/octo-server/src/config.rs` (add credentials loading after merge)
- Test: `crates/octo-server/tests/config_layered.rs` (add credentials test)

**Step 1: Define CredentialsFile struct**

Add to `config.rs`:

```rust
/// Credentials file structure (~/.octo/credentials.yaml)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CredentialsFile {
    #[serde(default)]
    pub providers: std::collections::HashMap<String, ProviderCredential>,
}

/// Per-provider credential entry
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderCredential {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}
```

**Step 2: Add credential injection to Config::load()**

After the environment variable overrides section, add credential injection (BEFORE env vars, so env vars still win):

```rust
        // Credentials file (between config merge and env overrides)
        // Priority: env vars > credentials.yaml > config.yaml
        let cred_path = octo_root.credentials_path();
        if cred_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&cred_path) {
                match serde_yaml::from_str::<CredentialsFile>(&content) {
                    Ok(creds) => {
                        // Inject provider credentials (only if not already set by config)
                        let provider_name = config.provider.name.clone();
                        if let Some(cred) = creds.providers.get(&provider_name) {
                            if config.provider.api_key.is_none() {
                                config.provider.api_key = cred.api_key.clone();
                            }
                            if config.provider.base_url.is_none() {
                                config.provider.base_url = cred.base_url.clone();
                            }
                        }
                        tracing::debug!("Loaded credentials from {}", cred_path.display());
                    }
                    Err(e) => tracing::warn!("Failed to parse credentials: {}", e),
                }
            }
        }
```

> Place this code AFTER the config merge (global/project/local) but BEFORE the env var overrides. This ensures the priority chain: `env > credentials > config`.

**Step 3: Write test**

Add to `config_layered.rs`:

```rust
#[test]
fn test_credentials_loaded_from_global() {
    let tmp = tempdir().unwrap();
    let global_root = tmp.path().join("global");

    // Config with provider name but no key
    write_yaml(
        &global_root.join("config.yaml"),
        "provider:\n  name: anthropic\n",
    );

    // Credentials with the key
    write_yaml(
        &global_root.join("credentials.yaml"),
        "providers:\n  anthropic:\n    api_key: \"sk-test-123\"\n",
    );

    std::env::set_var("OCTO_GLOBAL_ROOT", &global_root);
    // Remove any env API key so credentials.yaml takes effect
    let orig = std::env::var("ANTHROPIC_API_KEY").ok();
    std::env::remove_var("ANTHROPIC_API_KEY");

    let octo_root = octo_engine::OctoRoot::with_working_dir(tmp.path()).unwrap();
    let config = octo_server_config::Config::load(None, None, None, &octo_root);

    assert_eq!(config.provider.api_key, Some("sk-test-123".to_string()));

    // Restore
    if let Some(k) = orig {
        std::env::set_var("ANTHROPIC_API_KEY", k);
    }
    std::env::remove_var("OCTO_GLOBAL_ROOT");
}
```

**Step 4: Run tests**

Run: `cargo test --workspace -- --test-threads=1`
Expected: ALL PASS

**Step 5: Commit**

```bash
git add crates/octo-server/src/config.rs crates/octo-server/tests/config_layered.rs
git commit -m "feat(config): load credentials from ~/.octo/credentials.yaml"
```

---

## Group 4: Hardcoded Path Fixes (AA-T4)

### Task AA-T4: Replace hardcoded TLS/cert paths with OctoRoot

**Files:**
- Modify: `crates/octo-server/src/main.rs:297` (TLS self-signed dir)
- Modify: `crates/octo-cli/src/main.rs:133` (dashboard cert dir)

**Step 1: Fix octo-server TLS path**

In `crates/octo-server/src/main.rs`, change line ~297:

From:
```rust
            let tls_dir = cfg
                .tls
                .self_signed_dir
                .clone()
                .unwrap_or_else(|| PathBuf::from("./data/tls"));
```

To:
```rust
            let tls_dir = cfg
                .tls
                .self_signed_dir
                .clone()
                .unwrap_or_else(|| octo_root.tls_dir());
```

**Step 2: Fix octo-cli dashboard cert path**

In `crates/octo-cli/src/main.rs`, change line ~133:

From:
```rust
                let cert_dir = std::path::PathBuf::from("./data/certs");
```

To:
```rust
                let cert_dir = octo_root.tls_dir();
```

> Note: `octo_root` is already available in `main.rs` scope (line 49-55).

**Step 3: Verify compilation**

Run: `cargo check --workspace`
Expected: No errors

**Step 4: Run tests**

Run: `cargo test --workspace -- --test-threads=1`
Expected: ALL PASS

**Step 5: Commit**

```bash
git add crates/octo-server/src/main.rs crates/octo-cli/src/main.rs
git commit -m "fix: replace hardcoded TLS/cert paths with OctoRoot::tls_dir()"
```

---

## Group 5: Config CLI Enhancement (AA-T5)

### Task AA-T5: Enhance `octo config show` to display config sources

**Files:**
- Modify: `crates/octo-cli/src/commands/config.rs:152-200` (show_config function)

**Step 1: Update show_config to show source information**

The current `show_config` already lists config entries with source. Enhance it to show the layered source discovery:

```rust
async fn show_config(state: &AppState) -> Result<()> {
    let root = &state.octo_root;

    // Show config source chain
    println!("Configuration Sources (highest priority first):");

    // Check env vars
    let env_count = ["OCTO_PORT", "OCTO_HOST", "OCTO_LOG", "LLM_PROVIDER",
                     "ANTHROPIC_API_KEY", "OPENAI_API_KEY"]
        .iter()
        .filter(|k| std::env::var(k).is_ok())
        .count();
    if env_count > 0 {
        println!("  \u{2705} Env:     {} OCTO_*/provider vars set", env_count);
    }

    // Check local config
    let local_path = root.project_local_config();
    if local_path.exists() {
        println!("  \u{2705} Local:   {}", local_path.display());
    } else {
        println!("  \u{2500}  Local:   {} (not found)", local_path.display());
    }

    // Check project config
    let project_path = root.project_config();
    if project_path.exists() {
        println!("  \u{2705} Project: {}", project_path.display());
    } else {
        println!("  \u{2500}  Project: {} (not found)", project_path.display());
    }

    // Check global config
    let global_path = root.global_config();
    if global_path.exists() {
        println!("  \u{2705} Global:  {}", global_path.display());
    } else {
        println!("  \u{2500}  Global:  {} (not found)", global_path.display());
    }

    // Check legacy
    let legacy = root.working_dir().join("config.yaml");
    if legacy.exists() && !project_path.exists() {
        println!("  \u{26A0}\u{FE0F}  Legacy:  {} (move to .octo/config.yaml)", legacy.display());
    }

    // Check credentials
    let cred_path = root.credentials_path();
    if cred_path.exists() {
        println!("  \u{2705} Creds:   {}", cred_path.display());
    } else {
        println!("  \u{2500}  Creds:   {} (not found)", cred_path.display());
    }

    println!();

    // ... keep existing effective config display ...
```

> Keep the rest of the existing show_config function that displays effective config entries.

**Step 2: Verify compilation and test**

Run: `cargo check -p octo-cli && cargo test -p octo-cli -- --test-threads=1`
Expected: PASS

**Step 3: Commit**

```bash
git add crates/octo-cli/src/commands/config.rs
git commit -m "feat(cli): enhance config show with layered source display"
```

---

## Group 6: Server main.rs reorder (AA-T2b)

### Task AA-T2b: Reorder octo-server main.rs — OctoRoot before Config

**Files:**
- Modify: `crates/octo-server/src/main.rs`

**Step 1: Move OctoRoot discovery before Config::load()**

Current order (main.rs):
```
line 68:  dotenvy
line 71:  Config::load(...)           ← needs OctoRoot
line 99:  OctoRoot::discover()        ← created too late
```

New order:
```
line 68:  dotenvy
line 70:  OctoRoot::discover()        ← moved up
line 75:  Config::load(..., &octo_root)
```

This is a code reorder, not a logic change. Move the `OctoRoot::discover()` block (lines 99-108) to right after `dotenvy::dotenv_override().ok()` (line 68), before `Config::load()`.

**Step 2: Verify**

Run: `cargo check -p octo-server && cargo test -p octo-server -- --test-threads=1`
Expected: PASS

**Step 3: Commit**

```bash
git add crates/octo-server/src/main.rs
git commit -m "refactor(server): reorder init — OctoRoot before Config::load()"
```

---

## Summary

| Task | Description | Dependencies | Estimated Size |
|------|-------------|-------------|---------------|
| AA-T1 | OctoRoot path extensions | None | Small |
| AA-T2b | Server main.rs reorder | None | Small |
| AA-T2 | Config::load() layered merge | AA-T1, AA-T2b | Medium |
| AA-T3 | Credentials loading | AA-T2 | Medium |
| AA-T4 | Hardcoded path fixes | AA-T1 | Small |
| AA-T5 | Config CLI enhancement | AA-T1 | Small |

**Execution order**: AA-T1 → AA-T2b → AA-T2 → AA-T3 → AA-T4 → AA-T5

**Parallel pairs**: (AA-T1, AA-T2b) can run in parallel. (AA-T4, AA-T5) can run in parallel after AA-T2.

**Total tests expected**: ~2383 + new tests (~2390+)

---

## Deferred (not in this phase)

| ID | Description | Precondition |
|----|-------------|-------------|
| AA-D1 | `octo auth login/status/logout` commands | UX design for interactive credential setup |
| AA-D2 | `octo init` command — project template creation | After config loading stabilizes | ✅ 已补 @ e85383a |
| AA-D3 | XDG Base Directory support (`$XDG_CONFIG_HOME/octo/`) | Low priority, OCTO_GLOBAL_ROOT already covers |
| AA-D4 | Config file watcher (hot-reload) | Future enhancement |
