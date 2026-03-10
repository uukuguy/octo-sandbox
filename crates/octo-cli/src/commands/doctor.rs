//! Doctor diagnostics command — environment checks + optional auto-repair

use crate::commands::AppState;
use crate::output::{self, TextOutput};
use anyhow::Result;
use serde::Serialize;
use std::path::Path;

/// Run health diagnostics
pub async fn run_doctor(repair: bool, state: &AppState) -> Result<()> {
    let mut checks = Vec::new();

    // Check 1: Database connectivity
    checks.push(check_database(&state.db_path));

    // Check 2: LLM provider configuration
    checks.push(check_provider());

    // Check 3: Working directory
    checks.push(check_working_dir(&state.working_dir));

    // Check 4: Agent catalog
    checks.push(check_agents(state));

    // Check 5: Tool registry
    checks.push(check_tools(state));

    // Check 6: MCP manager
    checks.push(check_mcp(state).await);

    // Check 7: Config file
    checks.push(check_config_file());

    let pass_count = checks.iter().filter(|c| c.status == CheckStatus::Pass).count();
    let warn_count = checks.iter().filter(|c| c.status == CheckStatus::Warn).count();
    let fail_count = checks.iter().filter(|c| c.status == CheckStatus::Fail).count();

    // Auto-repair if requested
    if repair {
        for check in &mut checks {
            if check.status == CheckStatus::Fail {
                if let Some(fix) = &check.fix_hint {
                    if let Some(repaired) = try_repair(&check.name, fix) {
                        check.repair_result = Some(repaired);
                    }
                }
            }
        }
    }

    let out = DoctorOutput {
        checks,
        summary: DoctorSummary {
            total: pass_count + warn_count + fail_count,
            pass: pass_count,
            warn: warn_count,
            fail: fail_count,
            repair_mode: repair,
        },
    };
    output::print_output(&out, &state.output_config);
    Ok(())
}

// ── Types ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize)]
enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Serialize)]
struct CheckResult {
    name: String,
    status: CheckStatus,
    message: String,
    fix_hint: Option<String>,
    repair_result: Option<String>,
}

#[derive(Serialize)]
struct DoctorOutput {
    checks: Vec<CheckResult>,
    summary: DoctorSummary,
}

#[derive(Serialize)]
struct DoctorSummary {
    total: usize,
    pass: usize,
    warn: usize,
    fail: usize,
    repair_mode: bool,
}

impl TextOutput for DoctorOutput {
    fn to_text(&self) -> String {
        let mut out = String::from("Octo Doctor - Health Diagnostics\n");
        out.push_str(&"=".repeat(40));
        out.push('\n');

        for check in &self.checks {
            let icon = match check.status {
                CheckStatus::Pass => "PASS",
                CheckStatus::Warn => "WARN",
                CheckStatus::Fail => "FAIL",
            };
            out.push_str(&format!("[{}] {}: {}\n", icon, check.name, check.message));
            if let Some(hint) = &check.fix_hint {
                out.push_str(&format!("       Fix: {}\n", hint));
            }
            if let Some(result) = &check.repair_result {
                out.push_str(&format!("       Repaired: {}\n", result));
            }
        }

        out.push('\n');
        out.push_str(&format!(
            "Summary: {} pass, {} warn, {} fail (total {})",
            self.summary.pass, self.summary.warn, self.summary.fail, self.summary.total
        ));
        if self.summary.repair_mode {
            out.push_str(" [repair mode]");
        }
        out.push('\n');
        out
    }
}

// ── Check functions ───────────────────────────────────────────

fn check_database(db_path: &Path) -> CheckResult {
    if db_path.exists() {
        CheckResult {
            name: "Database".to_string(),
            status: CheckStatus::Pass,
            message: format!("Found at {}", db_path.display()),
            fix_hint: None,
            repair_result: None,
        }
    } else {
        CheckResult {
            name: "Database".to_string(),
            status: CheckStatus::Warn,
            message: format!("Not found at {} (will be created on first use)", db_path.display()),
            fix_hint: None,
            repair_result: None,
        }
    }
}

fn check_provider() -> CheckResult {
    let provider = std::env::var("LLM_PROVIDER").unwrap_or_else(|_| "anthropic".to_string());
    let key_var = match provider.as_str() {
        "openai" => "OPENAI_API_KEY",
        _ => "ANTHROPIC_API_KEY",
    };

    if std::env::var(key_var).is_ok() {
        CheckResult {
            name: "LLM Provider".to_string(),
            status: CheckStatus::Pass,
            message: format!("{} ({} set)", provider, key_var),
            fix_hint: None,
            repair_result: None,
        }
    } else {
        CheckResult {
            name: "LLM Provider".to_string(),
            status: CheckStatus::Fail,
            message: format!("{} ({} not set)", provider, key_var),
            fix_hint: Some(format!("export {}=<your-api-key>", key_var)),
            repair_result: None,
        }
    }
}

fn check_working_dir(working_dir: &Path) -> CheckResult {
    if working_dir.is_dir() {
        CheckResult {
            name: "Working Directory".to_string(),
            status: CheckStatus::Pass,
            message: format!("{}", working_dir.display()),
            fix_hint: None,
            repair_result: None,
        }
    } else {
        CheckResult {
            name: "Working Directory".to_string(),
            status: CheckStatus::Fail,
            message: format!("{} (not a directory)", working_dir.display()),
            fix_hint: Some("Run octo from a valid directory".to_string()),
            repair_result: None,
        }
    }
}

fn check_agents(state: &AppState) -> CheckResult {
    let agents = state.agent_catalog.list_all();
    CheckResult {
        name: "Agent Catalog".to_string(),
        status: CheckStatus::Pass,
        message: format!("{} agents registered", agents.len()),
        fix_hint: None,
        repair_result: None,
    }
}

fn check_tools(state: &AppState) -> CheckResult {
    let registry = state.agent_runtime.tools();
    let guard = registry.lock().unwrap_or_else(|e| e.into_inner());
    let count = guard.names().len();
    drop(guard);

    CheckResult {
        name: "Tool Registry".to_string(),
        status: if count > 0 {
            CheckStatus::Pass
        } else {
            CheckStatus::Warn
        },
        message: format!("{} tools available", count),
        fix_hint: None,
        repair_result: None,
    }
}

async fn check_mcp(state: &AppState) -> CheckResult {
    let mgr = state.agent_runtime.mcp_manager();
    let guard = mgr.lock().await;
    let count = guard.server_count();
    drop(guard);

    CheckResult {
        name: "MCP Manager".to_string(),
        status: CheckStatus::Pass,
        message: format!("{} servers connected", count),
        fix_hint: None,
        repair_result: None,
    }
}

fn check_config_file() -> CheckResult {
    let config_path = Path::new("config.yaml");
    if config_path.exists() {
        CheckResult {
            name: "Config File".to_string(),
            status: CheckStatus::Pass,
            message: "config.yaml found".to_string(),
            fix_hint: None,
            repair_result: None,
        }
    } else {
        CheckResult {
            name: "Config File".to_string(),
            status: CheckStatus::Warn,
            message: "config.yaml not found (using defaults)".to_string(),
            fix_hint: Some("cp config.default.yaml config.yaml".to_string()),
            repair_result: None,
        }
    }
}

fn try_repair(name: &str, _fix_hint: &str) -> Option<String> {
    match name {
        "Config File" => {
            let default = Path::new("config.default.yaml");
            let target = Path::new("config.yaml");
            if default.exists() && !target.exists() {
                if std::fs::copy(default, target).is_ok() {
                    return Some("Copied config.default.yaml -> config.yaml".to_string());
                }
            }
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_provider_missing() {
        // This test verifies the output format
        let result = check_provider();
        // Provider check depends on env, so just verify structure
        assert!(!result.name.is_empty());
        assert!(!result.message.is_empty());
    }

    #[test]
    fn test_check_working_dir_valid() {
        let result = check_working_dir(Path::new("."));
        assert_eq!(result.status, CheckStatus::Pass);
    }

    #[test]
    fn test_check_working_dir_invalid() {
        let result = check_working_dir(Path::new("/nonexistent/path/xyz"));
        assert_eq!(result.status, CheckStatus::Fail);
    }

    #[test]
    fn test_check_config_file() {
        let result = check_config_file();
        // Status depends on whether config.yaml exists in CWD
        assert!(!result.name.is_empty());
    }

    #[test]
    fn test_doctor_output_format() {
        let out = DoctorOutput {
            checks: vec![
                CheckResult {
                    name: "Test Check".to_string(),
                    status: CheckStatus::Pass,
                    message: "All good".to_string(),
                    fix_hint: None,
                    repair_result: None,
                },
                CheckResult {
                    name: "Failing Check".to_string(),
                    status: CheckStatus::Fail,
                    message: "Something wrong".to_string(),
                    fix_hint: Some("Fix it".to_string()),
                    repair_result: None,
                },
            ],
            summary: DoctorSummary {
                total: 2,
                pass: 1,
                warn: 0,
                fail: 1,
                repair_mode: false,
            },
        };
        let text = out.to_text();
        assert!(text.contains("[PASS]"));
        assert!(text.contains("[FAIL]"));
        assert!(text.contains("1 pass"));
        assert!(text.contains("1 fail"));
    }

    #[test]
    fn test_try_repair_unknown() {
        assert!(try_repair("Unknown", "hint").is_none());
    }

    #[test]
    fn test_check_database_missing() {
        let result = check_database(Path::new("/tmp/nonexistent_test_db.db"));
        assert_eq!(result.status, CheckStatus::Warn);
    }
}
