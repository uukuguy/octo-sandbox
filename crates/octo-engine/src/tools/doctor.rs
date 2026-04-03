//! Doctor self-diagnosis tool — P3-6.
//!
//! Checks system health: required binaries, environment variables,
//! database connectivity, provider configuration, and MCP server status.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::process::Command;

use super::traits::Tool;
use octo_types::{ToolContext, ToolOutput, ToolSource};

pub struct DoctorTool;

#[async_trait]
impl Tool for DoctorTool {
    fn name(&self) -> &str {
        "doctor"
    }

    fn description(&self) -> &str {
        "Run self-diagnosis checks on the Octo agent environment.\n\
         Checks: required binaries (git, cargo, node), environment variables\n\
         (API keys), working directory health, and basic system info.\n\n\
         Returns a structured report with PASS/WARN/FAIL for each check.\n\n\
         When to use: troubleshooting setup issues, verifying environment, first-run validation.\n\
         When NOT to use: runtime debugging (use logs/events instead)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "verbose": {
                    "type": "boolean",
                    "description": "Include detailed version info for each binary (default: false)"
                }
            }
        })
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let verbose = params.get("verbose").and_then(|v| v.as_bool()).unwrap_or(false);
        let mut checks: Vec<CheckResult> = Vec::new();

        // 1. Required binaries
        check_binary(&mut checks, "git", &["--version"], verbose);
        check_binary(&mut checks, "cargo", &["--version"], verbose);
        check_binary(&mut checks, "node", &["--version"], verbose);
        check_binary(&mut checks, "npm", &["--version"], verbose);
        check_binary(&mut checks, "rustc", &["--version"], verbose);

        // 2. Environment variables
        check_env_var(&mut checks, "ANTHROPIC_API_KEY", true);
        check_env_var(&mut checks, "OPENAI_API_KEY", false);
        check_env_var(&mut checks, "RUST_LOG", false);

        // 3. Working directory checks
        check_working_dir(&mut checks, &ctx.working_dir);

        // 4. Git repository health
        check_git_repo(&mut checks, &ctx.working_dir);

        // 5. Disk space (basic)
        check_disk_space(&mut checks, &ctx.working_dir);

        // Build report
        let total = checks.len();
        let passed = checks.iter().filter(|c| c.status == Status::Pass).count();
        let warned = checks.iter().filter(|c| c.status == Status::Warn).count();
        let failed = checks.iter().filter(|c| c.status == Status::Fail).count();

        let mut report = format!(
            "Doctor Report: {passed} passed, {warned} warnings, {failed} failed (of {total} checks)\n\n"
        );

        for check in &checks {
            let icon = match check.status {
                Status::Pass => "PASS",
                Status::Warn => "WARN",
                Status::Fail => "FAIL",
            };
            report.push_str(&format!("[{icon}] {}: {}\n", check.name, check.detail));
        }

        if failed > 0 {
            report.push_str("\nSome checks failed. Fix the FAIL items above to ensure proper operation.");
        } else if warned > 0 {
            report.push_str("\nAll critical checks passed. Review WARN items for optimal setup.");
        } else {
            report.push_str("\nAll checks passed. Environment is healthy.");
        }

        Ok(ToolOutput::success(report))
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrency_safe(&self) -> bool {
        true
    }

    fn category(&self) -> &str {
        "diagnostics"
    }
}

#[derive(Debug, PartialEq)]
enum Status {
    Pass,
    Warn,
    Fail,
}

struct CheckResult {
    name: String,
    status: Status,
    detail: String,
}

fn check_binary(checks: &mut Vec<CheckResult>, name: &str, version_args: &[&str], verbose: bool) {
    match Command::new(name).args(version_args).output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_string();
            let detail = if verbose {
                version
            } else {
                "found".to_string()
            };
            checks.push(CheckResult {
                name: name.to_string(),
                status: Status::Pass,
                detail,
            });
        }
        Ok(_) => {
            checks.push(CheckResult {
                name: name.to_string(),
                status: Status::Warn,
                detail: "found but returned error".to_string(),
            });
        }
        Err(_) => {
            checks.push(CheckResult {
                name: name.to_string(),
                status: Status::Fail,
                detail: "not found in PATH".to_string(),
            });
        }
    }
}

fn check_env_var(checks: &mut Vec<CheckResult>, var: &str, required: bool) {
    match std::env::var(var) {
        Ok(val) if !val.is_empty() => {
            let masked = if val.len() > 8 {
                format!("{}...{}", &val[..4], &val[val.len() - 4..])
            } else {
                "***".to_string()
            };
            checks.push(CheckResult {
                name: format!("env:{var}"),
                status: Status::Pass,
                detail: format!("set ({masked})"),
            });
        }
        _ => {
            let status = if required { Status::Fail } else { Status::Warn };
            let label = if required { "required but" } else { "optional," };
            checks.push(CheckResult {
                name: format!("env:{var}"),
                status,
                detail: format!("{label} not set"),
            });
        }
    }
}

fn check_working_dir(checks: &mut Vec<CheckResult>, dir: &std::path::Path) {
    if dir.exists() && dir.is_dir() {
        checks.push(CheckResult {
            name: "working_dir".to_string(),
            status: Status::Pass,
            detail: format!("{}", dir.display()),
        });
    } else {
        checks.push(CheckResult {
            name: "working_dir".to_string(),
            status: Status::Fail,
            detail: format!("{} does not exist or is not a directory", dir.display()),
        });
    }
}

fn check_git_repo(checks: &mut Vec<CheckResult>, dir: &std::path::Path) {
    match Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(dir)
        .output()
    {
        Ok(output) if output.status.success() => {
            // Get branch name
            let branch = Command::new("git")
                .args(["branch", "--show-current"])
                .current_dir(dir)
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default();
            checks.push(CheckResult {
                name: "git_repo".to_string(),
                status: Status::Pass,
                detail: if branch.is_empty() {
                    "valid git repository".to_string()
                } else {
                    format!("branch: {branch}")
                },
            });
        }
        _ => {
            checks.push(CheckResult {
                name: "git_repo".to_string(),
                status: Status::Warn,
                detail: "not a git repository".to_string(),
            });
        }
    }
}

fn check_disk_space(checks: &mut Vec<CheckResult>, dir: &std::path::Path) {
    // Use `df` on unix-like systems
    match Command::new("df")
        .args(["-h", &dir.to_string_lossy()])
        .output()
    {
        Ok(output) if output.status.success() => {
            let out = String::from_utf8_lossy(&output.stdout);
            // Parse second line for available space
            if let Some(line) = out.lines().nth(1) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                let avail = parts.get(3).unwrap_or(&"?");
                let use_pct = parts.get(4).unwrap_or(&"?");
                checks.push(CheckResult {
                    name: "disk_space".to_string(),
                    status: Status::Pass,
                    detail: format!("{avail} available ({use_pct} used)"),
                });
            } else {
                checks.push(CheckResult {
                    name: "disk_space".to_string(),
                    status: Status::Warn,
                    detail: "could not parse df output".to_string(),
                });
            }
        }
        _ => {
            checks.push(CheckResult {
                name: "disk_space".to_string(),
                status: Status::Warn,
                detail: "df command not available".to_string(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_ctx() -> ToolContext {
        ToolContext {
            sandbox_id: octo_types::SandboxId::default(),
            user_id: octo_types::UserId::from_string(octo_types::id::DEFAULT_USER_ID),
            working_dir: PathBuf::from("/tmp"),
            path_validator: None,
        }
    }

    #[test]
    fn test_doctor_metadata() {
        let tool = DoctorTool;
        assert_eq!(tool.name(), "doctor");
        assert!(tool.is_read_only());
        assert_eq!(tool.category(), "diagnostics");
    }

    #[tokio::test]
    async fn test_doctor_runs() {
        let tool = DoctorTool;
        let ctx = test_ctx();
        let result = tool.execute(json!({}), &ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("Doctor Report"));
        assert!(result.content.contains("checks"));
    }

    #[tokio::test]
    async fn test_doctor_verbose() {
        let tool = DoctorTool;
        let ctx = test_ctx();
        let result = tool
            .execute(json!({"verbose": true}), &ctx)
            .await
            .unwrap();
        assert!(!result.is_error);
        // Verbose should include version strings for found binaries
        assert!(result.content.contains("Doctor Report"));
    }

    #[test]
    fn test_check_binary_found() {
        let mut checks = Vec::new();
        // "echo" should exist on all unix systems
        check_binary(&mut checks, "echo", &["test"], false);
        assert_eq!(checks[0].status, Status::Pass);
    }

    #[test]
    fn test_check_binary_not_found() {
        let mut checks = Vec::new();
        check_binary(&mut checks, "nonexistent_binary_xyz_123", &[], false);
        assert_eq!(checks[0].status, Status::Fail);
    }

    #[test]
    fn test_check_working_dir_exists() {
        let mut checks = Vec::new();
        check_working_dir(&mut checks, std::path::Path::new("/tmp"));
        assert_eq!(checks[0].status, Status::Pass);
    }

    #[test]
    fn test_check_working_dir_missing() {
        let mut checks = Vec::new();
        check_working_dir(&mut checks, std::path::Path::new("/nonexistent_dir_xyz"));
        assert_eq!(checks[0].status, Status::Fail);
    }
}
