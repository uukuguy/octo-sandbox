//! Config commands implementation — show/validate/init/get/set/paths

use crate::commands::{AppState, ConfigCommands};
use crate::output::{self, TextOutput};
use anyhow::Result;
use serde::Serialize;
use std::path::PathBuf;

/// Handle config commands
pub async fn handle_config(action: ConfigCommands, state: &AppState) -> Result<()> {
    match action {
        ConfigCommands::Show => show_config(state).await?,
        ConfigCommands::Validate => validate_config(state).await?,
        ConfigCommands::Init => init_config(state).await?,
        ConfigCommands::Get { key } => get_config(key, state).await?,
        ConfigCommands::Set { key, value } => set_config(key, value, state).await?,
        ConfigCommands::Paths => show_paths(state).await?,
    }
    Ok(())
}

// ── Output types ──────────────────────────────────────────────

#[derive(Serialize)]
struct ConfigShowOutput {
    entries: Vec<ConfigEntry>,
}

#[derive(Serialize)]
struct ConfigEntry {
    key: String,
    value: String,
    source: String,
}

impl TextOutput for ConfigShowOutput {
    fn to_text(&self) -> String {
        let mut out = String::from("Current Configuration:\n\n");
        let max_key = self.entries.iter().map(|e| e.key.len()).max().unwrap_or(20);
        for e in &self.entries {
            let val_display = if e.key.contains("KEY") && e.value != "not set" {
                "***set***".to_string()
            } else {
                e.value.clone()
            };
            out.push_str(&format!(
                "  {:<width$}  {}  ({})\n",
                e.key,
                val_display,
                e.source,
                width = max_key
            ));
        }
        out
    }
}

#[derive(Serialize)]
struct ConfigValidateOutput {
    checks: Vec<ValidateCheck>,
    overall: bool,
}

#[derive(Serialize)]
struct ValidateCheck {
    name: String,
    ok: bool,
    message: String,
}

impl TextOutput for ConfigValidateOutput {
    fn to_text(&self) -> String {
        let mut out = String::from("Configuration Validation:\n\n");
        for c in &self.checks {
            let icon = if c.ok { "OK" } else { "FAIL" };
            out.push_str(&format!("  [{}] {}: {}\n", icon, c.name, c.message));
        }
        out.push('\n');
        if self.overall {
            out.push_str("Validation PASSED\n");
        } else {
            out.push_str("Validation FAILED\n");
        }
        out
    }
}

#[derive(Serialize)]
struct ConfigGetOutput {
    key: String,
    value: Option<String>,
}

impl TextOutput for ConfigGetOutput {
    fn to_text(&self) -> String {
        match &self.value {
            Some(v) => format!("{} = {}", self.key, v),
            None => format!("{}: not found", self.key),
        }
    }
}

#[derive(Serialize)]
struct ConfigSetOutput {
    key: String,
    value: String,
    message: String,
}

impl TextOutput for ConfigSetOutput {
    fn to_text(&self) -> String {
        format!("{}: {} -> {} ({})", self.key, self.key, self.value, self.message)
    }
}

#[derive(Serialize)]
struct ConfigPathsOutput {
    paths: Vec<ConfigPathEntry>,
}

#[derive(Serialize)]
struct ConfigPathEntry {
    name: String,
    path: String,
    exists: bool,
}

impl TextOutput for ConfigPathsOutput {
    fn to_text(&self) -> String {
        let mut out = String::from("Configuration Paths:\n\n");
        for p in &self.paths {
            let icon = if p.exists { "found" } else { "missing" };
            out.push_str(&format!("  {:<20} {} ({})\n", p.name, p.path, icon));
        }
        out
    }
}

#[derive(Serialize)]
struct ConfigInitOutput {
    message: String,
}

impl TextOutput for ConfigInitOutput {
    fn to_text(&self) -> String {
        self.message.clone()
    }
}

// ── Handlers ──────────────────────────────────────────────────

async fn show_config(state: &AppState) -> Result<()> {
    let root = &state.grid_root;

    // Show config source chain
    println!("Configuration Sources (highest priority first):");

    // Check env vars
    let env_count = [
        "GRID_PORT",
        "GRID_HOST",
        "GRID_LOG",
        "LLM_PROVIDER",
        "ANTHROPIC_API_KEY",
        "OPENAI_API_KEY",
    ]
    .iter()
    .filter(|k| std::env::var(k).is_ok())
    .count();
    if env_count > 0 {
        println!("  \u{2705} Env:     {} GRID_*/provider vars set", env_count);
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
        println!(
            "  \u{26A0}\u{FE0F}  Legacy:  {} (move to .grid/config.yaml)",
            legacy.display()
        );
    }

    // Check credentials
    let cred_path = root.credentials_path();
    if cred_path.exists() {
        println!("  \u{2705} Creds:   {}", cred_path.display());
    } else {
        println!("  \u{2500}  Creds:   {} (not found)", cred_path.display());
    }

    println!();

    // Effective config entries
    let entries = vec![
        ConfigEntry {
            key: "GRID_DB_PATH".to_string(),
            value: std::env::var("GRID_DB_PATH").unwrap_or_else(|_| "not set".to_string()),
            source: "env".to_string(),
        },
        ConfigEntry {
            key: "ANTHROPIC_API_KEY".to_string(),
            value: if std::env::var("ANTHROPIC_API_KEY").is_ok() {
                "configured".to_string()
            } else {
                "not set".to_string()
            },
            source: "env".to_string(),
        },
        ConfigEntry {
            key: "OPENAI_API_KEY".to_string(),
            value: if std::env::var("OPENAI_API_KEY").is_ok() {
                "configured".to_string()
            } else {
                "not set".to_string()
            },
            source: "env".to_string(),
        },
        ConfigEntry {
            key: "LLM_PROVIDER".to_string(),
            value: std::env::var("LLM_PROVIDER").unwrap_or_else(|_| "anthropic".to_string()),
            source: "env/default".to_string(),
        },
        ConfigEntry {
            key: "RUST_LOG".to_string(),
            value: std::env::var("RUST_LOG").unwrap_or_else(|_| "not set".to_string()),
            source: "env".to_string(),
        },
        ConfigEntry {
            key: "database".to_string(),
            value: state.db_path.display().to_string(),
            source: "cli".to_string(),
        },
        ConfigEntry {
            key: "working_dir".to_string(),
            value: state.working_dir.display().to_string(),
            source: "runtime".to_string(),
        },
    ];

    let out = ConfigShowOutput { entries };
    output::print_output(&out, &state.output_config);
    Ok(())
}

async fn validate_config(state: &AppState) -> Result<()> {
    let mut checks = Vec::new();

    // Check provider key
    let provider = std::env::var("LLM_PROVIDER").unwrap_or_else(|_| "anthropic".to_string());
    let key_var = match provider.as_str() {
        "openai" => "OPENAI_API_KEY",
        _ => "ANTHROPIC_API_KEY",
    };
    let key_ok = std::env::var(key_var).is_ok();
    checks.push(ValidateCheck {
        name: format!("Provider ({})", provider),
        ok: key_ok,
        message: if key_ok {
            format!("{} is set", key_var)
        } else {
            format!("{} is NOT set", key_var)
        },
    });

    // Check database path
    checks.push(ValidateCheck {
        name: "Database".to_string(),
        ok: true,
        message: format!("Path: {}", state.db_path.display()),
    });

    // Check working directory
    let wd_ok = state.working_dir.is_dir();
    checks.push(ValidateCheck {
        name: "Working Directory".to_string(),
        ok: wd_ok,
        message: if wd_ok {
            "Valid directory".to_string()
        } else {
            "Not a valid directory".to_string()
        },
    });

    // Check tool count
    let tool_count = {
        let guard = state
            .agent_runtime
            .tools()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        guard.names().len()
    };
    checks.push(ValidateCheck {
        name: "Tools".to_string(),
        ok: tool_count > 0,
        message: format!("{} tools registered", tool_count),
    });

    let overall = checks.iter().all(|c| c.ok);
    let out = ConfigValidateOutput { checks, overall };
    output::print_output(&out, &state.output_config);
    Ok(())
}

async fn init_config(_state: &AppState) -> Result<()> {
    use dialoguer::{Input, Select};

    println!("Octo CLI Configuration Wizard\n");

    // Provider selection
    let providers = vec!["anthropic", "openai"];
    let provider_idx = Select::new()
        .with_prompt("Select LLM provider")
        .items(&providers)
        .default(0)
        .interact()?;
    let provider = providers[provider_idx];

    // API key
    let key_var = match provider {
        "openai" => "OPENAI_API_KEY",
        _ => "ANTHROPIC_API_KEY",
    };
    let api_key: String = Input::new()
        .with_prompt(format!("Enter {} (or press Enter to skip)", key_var))
        .allow_empty(true)
        .interact_text()?;

    // Database path
    let db_path: String = Input::new()
        .with_prompt("Database path")
        .default("grid.db".to_string())
        .interact_text()?;

    // Write .env
    let mut env_content = format!("LLM_PROVIDER={}\n", provider);
    if !api_key.is_empty() {
        env_content.push_str(&format!("{}={}\n", key_var, api_key));
    }
    env_content.push_str(&format!("GRID_DB_PATH={}\n", db_path));

    let env_path = PathBuf::from(".env");
    if env_path.exists() {
        println!("\n.env already exists. Configuration printed below (not overwritten):\n");
        println!("{}", env_content);
    } else {
        std::fs::write(&env_path, &env_content)?;
        println!("\nConfiguration written to .env");
    }

    let out = ConfigInitOutput {
        message: "Configuration wizard complete.".to_string(),
    };
    output::print_output(&out, &_state.output_config);
    Ok(())
}

async fn get_config(key: String, state: &AppState) -> Result<()> {
    let value = match key.as_str() {
        "LLM_PROVIDER" | "ANTHROPIC_API_KEY" | "OPENAI_API_KEY" | "GRID_DB_PATH" | "RUST_LOG"
        | "GRID_HOST" | "GRID_PORT" => std::env::var(&key).ok(),
        "database" | "db_path" => Some(state.db_path.display().to_string()),
        "working_dir" => Some(state.working_dir.display().to_string()),
        _ => {
            // Try env var
            std::env::var(&key).ok()
        }
    };

    let out = ConfigGetOutput { key, value };
    output::print_output(&out, &state.output_config);
    Ok(())
}

async fn set_config(key: String, value: String, state: &AppState) -> Result<()> {
    // For CLI, we can only set environment variables for the current process
    // Persistent changes would require writing to .env or config.yaml
    std::env::set_var(&key, &value);

    let out = ConfigSetOutput {
        key,
        value,
        message: "Set for current session (not persisted)".to_string(),
    };
    output::print_output(&out, &state.output_config);
    Ok(())
}

async fn show_paths(state: &AppState) -> Result<()> {
    let root = &state.grid_root;
    let global_cfg = root.global_config();
    let project_cfg = root.project_config();
    let local_cfg = root.project_local_config();
    let creds = root.credentials_path();

    let paths = vec![
        ConfigPathEntry {
            name: "Global Config".to_string(),
            path: global_cfg.display().to_string(),
            exists: global_cfg.exists(),
        },
        ConfigPathEntry {
            name: "Project Config".to_string(),
            path: project_cfg.display().to_string(),
            exists: project_cfg.exists(),
        },
        ConfigPathEntry {
            name: "Local Config".to_string(),
            path: local_cfg.display().to_string(),
            exists: local_cfg.exists(),
        },
        ConfigPathEntry {
            name: "Credentials".to_string(),
            path: creds.display().to_string(),
            exists: creds.exists(),
        },
        ConfigPathEntry {
            name: "Legacy Config".to_string(),
            path: "config.yaml".to_string(),
            exists: PathBuf::from("config.yaml").exists(),
        },
        ConfigPathEntry {
            name: "Environment".to_string(),
            path: ".env".to_string(),
            exists: PathBuf::from(".env").exists(),
        },
        ConfigPathEntry {
            name: "Database".to_string(),
            path: state.db_path.display().to_string(),
            exists: state.db_path.exists(),
        },
        ConfigPathEntry {
            name: "Working Dir".to_string(),
            path: state.working_dir.display().to_string(),
            exists: state.working_dir.exists(),
        },
    ];

    let out = ConfigPathsOutput { paths };
    output::print_output(&out, &state.output_config);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_show_output() {
        let out = ConfigShowOutput {
            entries: vec![ConfigEntry {
                key: "LLM_PROVIDER".to_string(),
                value: "anthropic".to_string(),
                source: "env".to_string(),
            }],
        };
        let text = out.to_text();
        assert!(text.contains("LLM_PROVIDER"));
        assert!(text.contains("anthropic"));
    }

    #[test]
    fn test_config_validate_output_pass() {
        let out = ConfigValidateOutput {
            checks: vec![ValidateCheck {
                name: "Test".to_string(),
                ok: true,
                message: "OK".to_string(),
            }],
            overall: true,
        };
        assert!(out.to_text().contains("PASSED"));
    }

    #[test]
    fn test_config_validate_output_fail() {
        let out = ConfigValidateOutput {
            checks: vec![ValidateCheck {
                name: "Test".to_string(),
                ok: false,
                message: "Missing".to_string(),
            }],
            overall: false,
        };
        assert!(out.to_text().contains("FAILED"));
    }

    #[test]
    fn test_config_get_found() {
        let out = ConfigGetOutput {
            key: "key".to_string(),
            value: Some("val".to_string()),
        };
        assert!(out.to_text().contains("val"));
    }

    #[test]
    fn test_config_get_not_found() {
        let out = ConfigGetOutput {
            key: "key".to_string(),
            value: None,
        };
        assert!(out.to_text().contains("not found"));
    }

    #[test]
    fn test_config_paths_output() {
        let out = ConfigPathsOutput {
            paths: vec![ConfigPathEntry {
                name: "Test".to_string(),
                path: "/tmp/test".to_string(),
                exists: false,
            }],
        };
        let text = out.to_text();
        assert!(text.contains("Test"));
        assert!(text.contains("missing"));
    }

    #[test]
    fn test_config_set_output() {
        let out = ConfigSetOutput {
            key: "KEY".to_string(),
            value: "VAL".to_string(),
            message: "done".to_string(),
        };
        let text = out.to_text();
        assert!(text.contains("KEY"));
        assert!(text.contains("VAL"));
    }
}
