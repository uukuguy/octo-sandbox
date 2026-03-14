//! End-to-end programming evaluation suite.
//!
//! Tests agent ability to fix bugs in code by verifying patches against test suites.
//! Each fixture contains buggy source code, a test script, and a known-good fix.
//!
//! **Mock mode** (default): Verifies the scoring/verification pipeline using
//! pre-built fix files.  No LLM provider needed.
//!
//! **Live mode**: When wired through `EvalRunner` with a real provider, the agent
//! loop generates patches which are then verified the same way.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use serde::Deserialize;

use crate::runner::{EvalReport, TaskResult};
use crate::score::{EvalScore, ScoreDetails};
use crate::task::AgentOutput;

/// Fixture manifest — loaded from `manifest.json` in each fixture directory.
#[derive(Debug, Deserialize)]
struct FixtureManifest {
    id: String,
    #[allow(dead_code)]
    name: String,
    test_cmd: String,
    fix_file: String,
    #[allow(dead_code)]
    difficulty: String,
}

/// End-to-end programming evaluation suite.
pub struct E2eSuite;

impl E2eSuite {
    /// Run in mock mode — verifies the scoring pipeline using known-good patches.
    ///
    /// For each fixture:
    /// 1. Copies fixture files to a temp directory
    /// 2. Runs the test on buggy code (should fail)
    /// 3. Applies the known fix (`fix.py`)
    /// 4. Runs the test on fixed code (should pass)
    /// 5. Scores: both conditions must hold for a full pass
    pub async fn run() -> Result<EvalReport> {
        let fixtures_dir = Self::fixtures_dir()?;
        let mut results = Vec::new();

        let mut entries: Vec<_> = std::fs::read_dir(&fixtures_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();
        entries.sort_by_key(|e| e.file_name());

        for entry in &entries {
            let fixture_path = entry.path();
            match Self::run_mock_fixture(&fixture_path).await {
                Ok(result) => results.push(result),
                Err(e) => {
                    let id = fixture_path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    results.push(TaskResult {
                        task_id: format!("e2e-{}", id),
                        output: AgentOutput::default(),
                        score: EvalScore::fail(
                            0.0,
                            ScoreDetails::Custom {
                                message: format!("Fixture error: {}", e),
                            },
                        ),
                        duration_ms: 0,
                    });
                }
            }
        }

        Ok(EvalReport::from_results(results))
    }

    /// Resolve the fixtures directory.
    ///
    /// Tries `CARGO_MANIFEST_DIR/datasets/e2e_fixtures` first (works during
    /// `cargo test`), then falls back to a cwd-relative path.
    fn fixtures_dir() -> Result<PathBuf> {
        let manifest_dir =
            std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
        let dir = PathBuf::from(&manifest_dir).join("datasets/e2e_fixtures");
        if dir.exists() {
            return Ok(dir);
        }

        let cwd_relative = PathBuf::from("crates/octo-eval/datasets/e2e_fixtures");
        if cwd_relative.exists() {
            return Ok(cwd_relative);
        }

        anyhow::bail!(
            "E2E fixtures directory not found (tried {} and crates/octo-eval/datasets/e2e_fixtures)",
            dir.display()
        )
    }

    /// Run a single fixture in mock mode.
    async fn run_mock_fixture(fixture_path: &Path) -> Result<TaskResult> {
        let start = Instant::now();
        let manifest: FixtureManifest = {
            let content = std::fs::read_to_string(fixture_path.join("manifest.json"))?;
            serde_json::from_str(&content)?
        };

        // Create temp dir and copy all fixture files into it
        let tmpdir = tempfile::tempdir()?;
        copy_dir_contents(fixture_path, tmpdir.path())?;

        // Step 1: Run test on buggy code — should FAIL
        let buggy_result = run_test_cmd(&manifest.test_cmd, tmpdir.path());
        let buggy_failed = buggy_result
            .as_ref()
            .map(|r| !r.success)
            .unwrap_or(true);

        // Step 2: Apply fix using manifest's fix_file field.
        // The fix file shares the same basename as the source file it replaces.
        // E.g., for Python fixtures: fix_file="src.py", fix source="fix.py"
        // For Rust fixtures: fix_file="src/lib.rs", fix source="fix.rs"
        let fix_source = fixture_path.join(
            if manifest.fix_file.contains('/') {
                // For nested paths like "src/lib.rs", look for "fix.rs" at fixture root
                "fix.rs".to_string()
            } else {
                // For flat paths like "src.py", look for "fix.py" at fixture root
                format!("fix.{}", manifest.fix_file.rsplit('.').next().unwrap_or("py"))
            }
        );
        let has_fix = fix_source.exists();

        let (fixed_passed, test_output, exit_code) = if has_fix {
            let fix_content = std::fs::read_to_string(&fix_source)?;
            // Ensure parent directory exists for nested fix_file paths (e.g., "src/lib.rs")
            let target_path = tmpdir.path().join(&manifest.fix_file);
            if let Some(parent) = target_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&target_path, &fix_content)?;

            match run_test_cmd(&manifest.test_cmd, tmpdir.path()) {
                Ok(r) => (r.success, r.output, r.exit_code),
                Err(e) => (false, format!("Error: {}", e), -1),
            }
        } else {
            (false, format!("No fix file found at {:?}", fix_source), 1)
        };

        // Score: both conditions required for full pass
        //   1. Buggy code must fail tests (confirms bug exists)
        //   2. Fixed code must pass tests (confirms fix works)
        let passed = buggy_failed && fixed_passed;
        let score = match (buggy_failed, fixed_passed) {
            (true, true) => 1.0,
            (true, false) => 0.25,
            (false, true) => 0.25,
            (false, false) => 0.0,
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(TaskResult {
            task_id: manifest.id,
            output: AgentOutput::default(),
            score: if passed {
                EvalScore::pass(
                    score,
                    ScoreDetails::PatchVerify {
                        test_cmd: manifest.test_cmd,
                        test_output,
                        exit_code,
                    },
                )
            } else {
                EvalScore::fail(
                    score,
                    ScoreDetails::PatchVerify {
                        test_cmd: manifest.test_cmd,
                        test_output,
                        exit_code,
                    },
                )
            },
            duration_ms,
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

struct TestOutput {
    success: bool,
    output: String,
    exit_code: i32,
}

fn run_test_cmd(cmd: &str, working_dir: &Path) -> Result<TestOutput> {
    let output = std::process::Command::new("sh")
        .args(["-c", cmd])
        .current_dir(working_dir)
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout, stderr);

    Ok(TestOutput {
        success: output.status.success(),
        output: combined,
        exit_code: output.status.code().unwrap_or(-1),
    })
}

fn copy_dir_contents(src: &Path, dst: &Path) -> Result<()> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            std::fs::create_dir_all(&dst_path)?;
            copy_dir_contents(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_e2e_suite_runs() {
        let report = E2eSuite::run().await.unwrap();
        assert_eq!(report.total, 14, "Expected 14 e2e fixtures");
        assert!(
            report.passed >= 12,
            "Expected at least 12/14 passed, got {}. Failures: {:?}",
            report.passed,
            report
                .results
                .iter()
                .filter(|r| !r.score.passed)
                .map(|r| (&r.task_id, &r.score.details))
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn test_e2e_all_fixtures_pass() {
        let report = E2eSuite::run().await.unwrap();
        assert_eq!(
            report.passed, 14,
            "Expected all 14 e2e fixtures to pass. Failures: {:?}",
            report
                .results
                .iter()
                .filter(|r| !r.score.passed)
                .map(|r| (&r.task_id, &r.score.details))
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn test_e2e_scores_use_patch_verify() {
        let report = E2eSuite::run().await.unwrap();
        for result in &report.results {
            match &result.score.details {
                ScoreDetails::PatchVerify { .. } => {} // expected
                ScoreDetails::Custom { .. } => {
                    panic!(
                        "Fixture {} used Custom instead of PatchVerify: {:?}",
                        result.task_id, result.score.details
                    );
                }
                other => {
                    panic!(
                        "Fixture {} used unexpected ScoreDetails variant: {:?}",
                        result.task_id, other
                    );
                }
            }
        }
    }

    #[test]
    fn test_copy_dir_contents() {
        let src = tempfile::tempdir().unwrap();
        std::fs::write(src.path().join("file.txt"), "hello").unwrap();
        std::fs::create_dir(src.path().join("sub")).unwrap();
        std::fs::write(src.path().join("sub/nested.txt"), "world").unwrap();

        let dst = tempfile::tempdir().unwrap();
        copy_dir_contents(src.path(), dst.path()).unwrap();

        assert_eq!(
            std::fs::read_to_string(dst.path().join("file.txt")).unwrap(),
            "hello"
        );
        assert_eq!(
            std::fs::read_to_string(dst.path().join("sub/nested.txt")).unwrap(),
            "world"
        );
    }

    #[test]
    fn test_fixtures_dir_exists() {
        let dir = E2eSuite::fixtures_dir().unwrap();
        assert!(dir.exists(), "Fixtures directory should exist: {}", dir.display());

        // Should have exactly 8 fixture subdirectories
        let count = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .count();
        assert_eq!(count, 14, "Expected 14 fixture directories, found {}", count);
    }

    #[test]
    fn test_fixture_manifests_parse() {
        let dir = E2eSuite::fixtures_dir().unwrap();
        for entry in std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()) {
            if !entry.path().is_dir() {
                continue;
            }
            let manifest_path = entry.path().join("manifest.json");
            assert!(
                manifest_path.exists(),
                "Missing manifest.json in {:?}",
                entry.path()
            );
            let content = std::fs::read_to_string(&manifest_path).unwrap();
            let manifest: FixtureManifest = serde_json::from_str(&content).unwrap_or_else(|e| {
                panic!(
                    "Failed to parse manifest in {:?}: {}",
                    entry.path(),
                    e
                );
            });
            assert!(!manifest.id.is_empty());
            assert!(!manifest.test_cmd.is_empty());
            assert!(!manifest.fix_file.is_empty());
        }
    }
}
