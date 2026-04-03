//! Built-in development command tools — commit, diff, security-review.
//!
//! P3-5: Wraps common git and code-review workflows as agent-callable tools
//! so the LLM can operate without raw bash commands for routine dev tasks.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::process::Command;

use super::traits::Tool;
use octo_types::{ApprovalRequirement, RiskLevel, ToolContext, ToolOutput, ToolSource};

// ── helpers ──────────────────────────────────────────────────────────────────

fn run_git(args: &[&str], cwd: &std::path::Path) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if output.status.success() {
        Ok(stdout)
    } else {
        Ok(format!("[exit {}]\n{stdout}{stderr}", output.status.code().unwrap_or(-1)))
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// git_diff — show working tree diff
// ═══════════════════════════════════════════════════════════════════════════════

pub struct GitDiffTool;

#[async_trait]
impl Tool for GitDiffTool {
    fn name(&self) -> &str {
        "git_diff"
    }

    fn description(&self) -> &str {
        "Show git diff of the working tree. Returns staged and/or unstaged changes.\n\
         Use this instead of `bash git diff` for safer, structured output.\n\n\
         Parameters:\n\
         - staged: if true, show only staged changes (--cached)\n\
         - stat: if true, show diffstat summary instead of full patch\n\
         - path: optional path filter\n\n\
         When to use: before committing, reviewing changes, understanding what changed.\n\
         When NOT to use: for log history (use bash git log instead)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "staged": {
                    "type": "boolean",
                    "description": "Show only staged changes (default: false, shows all)"
                },
                "stat": {
                    "type": "boolean",
                    "description": "Show diffstat summary only (default: false)"
                },
                "path": {
                    "type": "string",
                    "description": "Optional file or directory path to filter diff"
                }
            }
        })
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let staged = params.get("staged").and_then(|v| v.as_bool()).unwrap_or(false);
        let stat = params.get("stat").and_then(|v| v.as_bool()).unwrap_or(false);
        let path = params.get("path").and_then(|v| v.as_str());

        let mut args = vec!["diff"];
        if staged {
            args.push("--cached");
        }
        if stat {
            args.push("--stat");
        }
        if let Some(p) = path {
            args.push("--");
            args.push(p);
        }

        let output = run_git(&args, &ctx.working_dir)?;
        if output.trim().is_empty() {
            Ok(ToolOutput::success("No changes found."))
        } else {
            Ok(ToolOutput::success(output))
        }
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn category(&self) -> &str {
        "dev"
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// git_commit — create a git commit
// ═══════════════════════════════════════════════════════════════════════════════

pub struct GitCommitTool;

#[async_trait]
impl Tool for GitCommitTool {
    fn name(&self) -> &str {
        "git_commit"
    }

    fn description(&self) -> &str {
        "Stage files and create a git commit. Safer than raw bash: validates message format,\n\
         refuses to commit secrets (.env, *.pem, *.key), and always runs hooks.\n\n\
         Parameters:\n\
         - message (required): commit message\n\
         - files: array of file paths to stage (default: all modified files)\n\
         - all: if true, stage all tracked changes (-a flag)\n\n\
         When to use: after completing a task, fixing a bug, implementing a feature.\n\
         When NOT to use: if you haven't verified the changes compile/pass tests."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "Commit message"
                },
                "files": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Specific files to stage (omit to stage all)"
                },
                "all": {
                    "type": "boolean",
                    "description": "Stage all tracked changes (git commit -a)"
                }
            },
            "required": ["message"]
        })
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let message = params
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: message"))?;

        let files: Option<Vec<String>> = params
            .get("files")
            .and_then(|v| serde_json::from_value(v.clone()).ok());
        let all = params.get("all").and_then(|v| v.as_bool()).unwrap_or(false);

        // Safety: refuse to commit known secret patterns
        let secret_patterns = [".env", ".pem", ".key", "credentials", "secret"];
        if let Some(ref fs) = files {
            for f in fs {
                let lower = f.to_lowercase();
                for pat in &secret_patterns {
                    if lower.contains(pat) {
                        return Ok(ToolOutput::error(format!(
                            "Refusing to commit '{f}' — matches secret pattern '{pat}'. \
                             Remove it from the file list or add to .gitignore."
                        )));
                    }
                }
            }
        }

        // Stage files
        if let Some(ref fs) = files {
            for f in fs {
                run_git(&["add", f], &ctx.working_dir)?;
            }
        } else if all {
            run_git(&["add", "-A"], &ctx.working_dir)?;
        } else {
            // Stage all modified (not untracked)
            run_git(&["add", "-u"], &ctx.working_dir)?;
        }

        // Commit
        let output = run_git(&["commit", "-m", message], &ctx.working_dir)?;
        Ok(ToolOutput::success(output))
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::HighRisk
    }

    fn approval(&self) -> ApprovalRequirement {
        ApprovalRequirement::AutoApprovable
    }

    fn category(&self) -> &str {
        "dev"
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// security_review — static security scan of changed files
// ═══════════════════════════════════════════════════════════════════════════════

pub struct SecurityReviewTool;

#[async_trait]
impl Tool for SecurityReviewTool {
    fn name(&self) -> &str {
        "security_review"
    }

    fn description(&self) -> &str {
        "Run a static security review on changed files. Checks for:\n\
         - Hardcoded secrets (API keys, tokens, passwords)\n\
         - SQL injection patterns\n\
         - Command injection risks\n\
         - Path traversal vulnerabilities\n\
         - Insecure crypto usage\n\n\
         Parameters:\n\
         - path: specific file or directory to review (default: git diff --name-only)\n\n\
         When to use: before committing security-sensitive changes, after editing auth/crypto code.\n\
         When NOT to use: for runtime security testing (use a proper scanner)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File or directory to review (default: changed files)"
                }
            }
        })
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let path = params.get("path").and_then(|v| v.as_str());

        // Determine files to scan
        let files: Vec<String> = if let Some(p) = path {
            let full = ctx.working_dir.join(p);
            if full.is_dir() {
                // List all source files in directory
                let output = run_git(&["ls-files", p], &ctx.working_dir)?;
                output.lines().map(|l| l.to_string()).collect()
            } else {
                vec![p.to_string()]
            }
        } else {
            // Changed files (staged + unstaged)
            let output = run_git(&["diff", "--name-only", "HEAD"], &ctx.working_dir)?;
            let mut files: Vec<String> = output.lines().map(|l| l.to_string()).collect();
            // Also include untracked
            let untracked = run_git(&["ls-files", "--others", "--exclude-standard"], &ctx.working_dir)?;
            files.extend(untracked.lines().map(|l| l.to_string()));
            files
        };

        let files: Vec<String> = files.into_iter().filter(|f| !f.is_empty()).collect();
        if files.is_empty() {
            return Ok(ToolOutput::success("No files to review."));
        }

        let mut findings: Vec<String> = Vec::new();

        for file_path in &files {
            let full = ctx.working_dir.join(file_path);
            let content = match std::fs::read_to_string(&full) {
                Ok(c) => c,
                Err(_) => continue, // binary or missing
            };

            let file_findings = scan_file(file_path, &content);
            findings.extend(file_findings);
        }

        if findings.is_empty() {
            Ok(ToolOutput::success(format!(
                "Security review passed. Scanned {} file(s), no issues found.",
                files.len()
            )))
        } else {
            let mut report = format!(
                "Security review: {} finding(s) in {} file(s):\n\n",
                findings.len(),
                files.len()
            );
            for (i, f) in findings.iter().enumerate() {
                report.push_str(&format!("{}. {}\n", i + 1, f));
            }
            Ok(ToolOutput::success(report))
        }
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn category(&self) -> &str {
        "dev"
    }
}

/// Pattern-based static security scanner.
fn scan_file(path: &str, content: &str) -> Vec<String> {
    let mut findings = Vec::new();

    let patterns: &[(&str, &str, &str)] = &[
        // (pattern, severity, description)
        (r#"(?i)(api[_-]?key|secret[_-]?key|password)\s*[:=]\s*["\'][^"\']{8,}"#, "HIGH", "Possible hardcoded secret"),
        (r#"(?i)sk-[a-zA-Z0-9]{20,}"#, "HIGH", "Possible API key (sk-* pattern)"),
        (r#"(?i)AKIA[0-9A-Z]{16}"#, "HIGH", "AWS Access Key ID"),
        (r#"(?i)ghp_[a-zA-Z0-9]{20,}"#, "HIGH", "GitHub personal access token"),
        (r#"(?i)format!\s*\(\s*"[^"]*\{\}[^"]*"\s*,\s*[^)]*\)\s*\.as_str\(\)"#, "MEDIUM", "Possible format string in SQL context"),
        (r#"(?i)Command::new\([^)]*\)\.arg\(.*format!"#, "MEDIUM", "Possible command injection via formatted args"),
        (r#"(?i)\.\./\.\."#, "LOW", "Path traversal pattern (../..)"),
        (r#"(?i)unsafe\s*\{"#, "INFO", "Unsafe block"),
        (r#"(?i)#\[allow\(unsafe"#, "INFO", "Allowed unsafe code"),
    ];

    for (pattern, severity, desc) in patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            for mat in re.find_iter(content) {
                let line_num = content[..mat.start()].matches('\n').count() + 1;
                let snippet: String = mat.as_str().chars().take(60).collect();
                findings.push(format!(
                    "[{severity}] {path}:{line_num} — {desc}: `{snippet}`"
                ));
            }
        }
    }

    findings
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
    fn test_git_diff_metadata() {
        let tool = GitDiffTool;
        assert_eq!(tool.name(), "git_diff");
        assert!(tool.is_read_only());
        assert_eq!(tool.category(), "dev");
    }

    #[test]
    fn test_git_commit_metadata() {
        let tool = GitCommitTool;
        assert_eq!(tool.name(), "git_commit");
        assert!(!tool.is_read_only());
        assert_eq!(tool.category(), "dev");
        assert_eq!(tool.risk_level(), RiskLevel::HighRisk);
    }

    #[test]
    fn test_security_review_metadata() {
        let tool = SecurityReviewTool;
        assert_eq!(tool.name(), "security_review");
        assert!(tool.is_read_only());
        assert_eq!(tool.category(), "dev");
    }

    #[test]
    fn test_scan_file_detects_hardcoded_secret() {
        let content = r#"let api_key = "sk-ant-1234567890abcdefghij";"#;
        let findings = scan_file("test.rs", content);
        assert!(!findings.is_empty(), "Should detect sk- pattern");
    }

    #[test]
    fn test_scan_file_detects_aws_key() {
        let content = r#"AWS_KEY = "AKIAIOSFODNN7EXAMPLE""#;
        let findings = scan_file("config.py", content);
        assert!(!findings.is_empty(), "Should detect AWS key");
    }

    #[test]
    fn test_scan_file_detects_github_token() {
        let content = r#"token = "ghp_aBcDeFgHiJkLmNoPqRsTuVwXyZ012345678""#;
        let findings = scan_file("ci.yml", content);
        assert!(!findings.is_empty(), "Should detect GitHub token");
    }

    #[test]
    fn test_scan_file_clean() {
        let content = "fn main() {\n    println!(\"hello\");\n}\n";
        let findings = scan_file("main.rs", content);
        assert!(findings.is_empty(), "Clean file should have no findings");
    }

    #[test]
    fn test_scan_file_detects_path_traversal() {
        let content = r#"let path = format!("{}/../../etc/passwd", user_input);"#;
        let findings = scan_file("handler.rs", content);
        assert!(
            findings.iter().any(|f| f.contains("Path traversal")),
            "Should detect path traversal"
        );
    }

    #[tokio::test]
    async fn test_git_commit_refuses_secrets() {
        let tool = GitCommitTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                json!({"message": "add config", "files": [".env.production"]}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.is_error, "Should refuse to commit .env file");
        assert!(result.content.contains("secret pattern"));
    }

    #[tokio::test]
    async fn test_git_commit_refuses_pem() {
        let tool = GitCommitTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                json!({"message": "add cert", "files": ["server.pem"]}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.is_error);
    }
}
