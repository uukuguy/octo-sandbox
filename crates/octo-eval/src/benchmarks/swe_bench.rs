//! SWE-bench benchmark adapter — end-to-end code repair evaluation.
//!
//! SWE-bench evaluates agent ability to fix real GitHub issues.
//! Requires Docker sandbox for full verification; supports mock fallback.

use std::path::PathBuf;

use std::collections::HashMap;

use serde::Deserialize;

use crate::benchmarks::{ExternalBenchmark, MetricDefinition};
use crate::score::{EvalScore, ScoreDetails};
use crate::task::{AgentOutput, Difficulty, EvalTask, TaskMetadata};

/// A single SWE-bench evaluation record parsed from JSONL
#[derive(Debug, Clone, Deserialize)]
pub struct SweBenchRecord {
    pub instance_id: String,
    pub repo: String,
    #[serde(default)]
    pub base_commit: String,
    #[serde(default)]
    pub patch: String,
    #[serde(default)]
    pub test_patch: String,
    pub problem_statement: String,
    #[serde(default)]
    pub hints_text: String,
    #[serde(default)]
    pub fail_to_pass: String,
    #[serde(default)]
    pub pass_to_pass: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub environment_setup_commit: String,
    #[serde(default)]
    pub created_at: String,
}

/// EvalTask implementation for a single SWE-bench task
pub struct SweBenchTask {
    record: SweBenchRecord,
    /// Full prompt with SWE-bench instructions (built once at construction)
    prompt: String,
}

impl SweBenchTask {
    pub fn new(record: SweBenchRecord) -> Self {
        let mut prompt = String::from(
            "You are a software engineer tasked with fixing a bug in a Python project.\n\n",
        );
        prompt.push_str("## Problem Statement\n\n");
        prompt.push_str(&record.problem_statement);
        prompt.push_str("\n\n");

        if !record.hints_text.is_empty() {
            prompt.push_str("## Hints\n\n");
            prompt.push_str(&record.hints_text);
            prompt.push_str("\n\n");
        }

        prompt.push_str("## Instructions\n\n");
        prompt.push_str("1. Explore the repository structure to understand the codebase.\n");
        prompt.push_str("2. Read relevant source files to understand the bug.\n");
        prompt.push_str("3. Implement a fix for the described issue.\n");
        prompt.push_str("4. After fixing, run `git diff` to show your changes.\n\n");
        prompt.push_str("Important:\n");
        prompt.push_str("- Do NOT modify test files.\n");
        prompt.push_str("- Make minimal, focused changes to fix the issue.\n");
        prompt.push_str("- Your final output should include the git diff of your changes.\n");

        Self { record, prompt }
    }

    /// Extract a unified diff patch from agent output text.
    /// Looks for `diff --git` blocks or `--- a/` / `+++ b/` pairs.
    pub fn extract_patch(text: &str) -> Option<String> {
        // Strategy 1: Find diff --git blocks
        if let Some(start) = text.find("diff --git") {
            // Extract from the first `diff --git` to the end of the diff content
            let diff_text = &text[start..];
            // Find where the diff ends (next non-diff content or end of text)
            let patch = diff_text
                .lines()
                .take_while(|line| {
                    line.starts_with("diff --git")
                        || line.starts_with("---")
                        || line.starts_with("+++")
                        || line.starts_with("@@")
                        || line.starts_with('+')
                        || line.starts_with('-')
                        || line.starts_with(' ')
                        || line.starts_with("index ")
                        || line.starts_with("new file")
                        || line.starts_with("deleted file")
                        || line.starts_with("old mode")
                        || line.starts_with("new mode")
                        || line.is_empty()
                })
                .collect::<Vec<_>>()
                .join("\n");
            if !patch.is_empty() {
                return Some(patch);
            }
        }

        // Strategy 2: Look for --- a/ and +++ b/ patterns
        if text.contains("--- a/") && text.contains("+++ b/") {
            let lines: Vec<&str> = text.lines().collect();
            let mut in_diff = false;
            let mut patch_lines = Vec::new();
            for line in &lines {
                if line.starts_with("--- a/") {
                    in_diff = true;
                }
                if in_diff {
                    patch_lines.push(*line);
                    // End of hunk detection: non-diff line after we started
                    if !line.starts_with("---")
                        && !line.starts_with("+++")
                        && !line.starts_with("@@")
                        && !line.starts_with('+')
                        && !line.starts_with('-')
                        && !line.starts_with(' ')
                        && !line.is_empty()
                    {
                        patch_lines.pop(); // remove the non-diff line
                        break;
                    }
                }
            }
            if !patch_lines.is_empty() {
                return Some(patch_lines.join("\n"));
            }
        }

        None
    }

    /// Generate a predictions.jsonl entry for the swebench harness.
    pub fn to_prediction(&self, model_patch: &str, model_name: &str) -> serde_json::Value {
        serde_json::json!({
            "instance_id": self.record.instance_id,
            "model_name_or_path": model_name,
            "model_patch": model_patch,
        })
    }

    /// Get the instance_id for this task
    pub fn instance_id(&self) -> &str {
        &self.record.instance_id
    }

    /// Get the record for this task
    pub fn record(&self) -> &SweBenchRecord {
        &self.record
    }

    /// Classify difficulty based on patch size and test complexity
    pub fn classify_difficulty(record: &SweBenchRecord) -> Difficulty {
        let patch_lines = record.patch.lines().count();
        let fail_tests: Vec<String> = serde_json::from_str(&record.fail_to_pass)
            .unwrap_or_default();
        let test_count = fail_tests.len();

        if patch_lines <= 10 && test_count <= 1 {
            Difficulty::Easy
        } else if patch_lines <= 50 && test_count <= 3 {
            Difficulty::Medium
        } else {
            Difficulty::Hard
        }
    }
}

impl EvalTask for SweBenchTask {
    fn id(&self) -> &str {
        &self.record.instance_id
    }

    fn prompt(&self) -> &str {
        &self.prompt
    }

    fn available_tools(&self) -> Option<Vec<octo_types::tool::ToolSpec>> {
        // SWE-bench tasks need bash to explore the codebase and apply patches
        Some(vec![
            octo_types::tool::ToolSpec {
                name: "bash".to_string(),
                description: "Execute shell commands to explore the codebase, read files, and apply patches".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": {"type": "string", "description": "The shell command to execute"}
                    },
                    "required": ["command"]
                }),
            },
            octo_types::tool::ToolSpec {
                name: "file_read".to_string(),
                description: "Read file contents".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"}
                    },
                    "required": ["path"]
                }),
            },
            octo_types::tool::ToolSpec {
                name: "file_write".to_string(),
                description: "Write content to a file".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"},
                        "content": {"type": "string"}
                    },
                    "required": ["path", "content"]
                }),
            },
        ])
    }

    fn tool_allowlist(&self) -> Option<Vec<String>> {
        Some(vec![
            "bash".into(),
            "file_read".into(),
            "file_write".into(),
        ])
    }

    fn score(&self, output: &AgentOutput) -> EvalScore {
        // Extract all text from agent output (messages + tool calls)
        let mut all_text = String::new();
        for msg in &output.messages {
            all_text.push_str(&msg.text_content());
            all_text.push('\n');
        }
        for tc in &output.tool_calls {
            all_text.push_str(&tc.output);
            all_text.push('\n');
        }

        // Try to extract a unified diff from the output
        let model_patch = Self::extract_patch(&all_text);

        let (passed, score) = if model_patch.is_some() {
            // Agent produced a patch — this is a meaningful attempt
            // Full verification requires running swebench harness (external step)
            // Score 0.5 for producing a patch; 1.0 only after harness verification
            (false, 0.5)
        } else {
            // No patch produced
            (false, 0.0)
        };

        EvalScore {
            passed,
            score,
            details: ScoreDetails::SweVerify {
                instance_id: self.record.instance_id.clone(),
                fail_to_pass_passed: false,
                pass_to_pass_passed: false,
                fail_to_pass_count: 0,
                pass_to_pass_count: 0,
                execution_time_ms: 0,
            },
            dimensions: {
                let mut d = HashMap::new();
                d.insert("has_patch".into(), if model_patch.is_some() { 1.0 } else { 0.0 });
                d
            },
            failure_class: None,
        }
    }

    fn scoring_data(&self) -> serde_json::Value {
        serde_json::json!({
            "benchmark": "swe_bench",
            "instance_id": self.record.instance_id,
            "repo": self.record.repo,
            "base_commit": self.record.base_commit,
            "problem_statement": self.record.problem_statement,
        })
    }

    fn metadata(&self) -> TaskMetadata {
        TaskMetadata {
            category: format!("swe-bench:{}", self.record.repo),
            difficulty: Self::classify_difficulty(&self.record),
            expected_steps: None,
            tags: vec!["external".into(), "swe_bench".into()],
        }
    }
}

/// SWE-bench benchmark adapter
pub struct SweBenchmark {
    dataset_path: Option<PathBuf>,
}

impl SweBenchmark {
    pub fn new() -> Self {
        Self {
            dataset_path: None,
        }
    }

    pub fn with_dataset(path: PathBuf) -> Self {
        Self {
            dataset_path: Some(path),
        }
    }

    fn default_dataset_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("datasets/swe_bench_lite.jsonl")
    }

    fn is_docker_available() -> bool {
        std::env::var("DOCKER_AVAILABLE")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false)
    }

    pub fn load_from_jsonl(path: &std::path::Path) -> anyhow::Result<Vec<Box<dyn EvalTask>>> {
        let content = std::fs::read_to_string(path)?;
        let mut tasks: Vec<Box<dyn EvalTask>> = Vec::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let record: SweBenchRecord = serde_json::from_str(line)?;
            tasks.push(Box::new(SweBenchTask::new(record)));
        }

        Ok(tasks)
    }
}

impl ExternalBenchmark for SweBenchmark {
    fn name(&self) -> &str {
        "swe_bench"
    }

    fn description(&self) -> &str {
        "SWE-bench Lite — end-to-end code repair evaluation with Docker verification"
    }

    fn load_tasks(&self) -> anyhow::Result<Vec<Box<dyn EvalTask>>> {
        let path = self
            .dataset_path
            .clone()
            .unwrap_or_else(Self::default_dataset_path);

        if !path.exists() {
            anyhow::bail!(
                "SWE-bench dataset not found at {}. Download or create swe_bench_lite.jsonl.",
                path.display()
            );
        }

        Self::load_from_jsonl(&path)
    }

    fn requires_sandbox(&self) -> bool {
        true
    }

    fn sandbox_available(&self) -> bool {
        Self::is_docker_available()
    }

    fn custom_metrics(&self) -> Vec<MetricDefinition> {
        vec![
            MetricDefinition {
                name: "resolve_rate".into(),
                description: "Percentage of issues successfully resolved".into(),
                unit: crate::benchmarks::MetricUnit::Percentage,
            },
            MetricDefinition {
                name: "avg_patch_size".into(),
                description: "Average patch size in lines".into(),
                unit: crate::benchmarks::MetricUnit::Count,
            },
        ]
    }
}

/// SWE-bench harness integration for official verification.
/// This runs the official swebench Python package to verify patches.
pub struct SweBenchHarness;

impl SweBenchHarness {
    /// Write predictions to a JSONL file for the swebench harness.
    pub fn write_predictions(
        predictions: &[(String, String)], // (instance_id, model_patch)
        model_name: &str,
        output_path: &std::path::Path,
    ) -> anyhow::Result<()> {
        let mut file = std::fs::File::create(output_path)?;
        use std::io::Write;
        for (instance_id, patch) in predictions {
            let entry = serde_json::json!({
                "instance_id": instance_id,
                "model_name_or_path": model_name,
                "model_patch": patch,
            });
            writeln!(file, "{}", serde_json::to_string(&entry)?)?;
        }
        Ok(())
    }

    /// Run the official swebench harness verification.
    /// Requires: pip install swebench
    /// Returns: Map of instance_id -> resolved (true/false)
    pub fn run_evaluation(
        predictions_path: &std::path::Path,
        dataset_name: &str,
        max_workers: usize,
    ) -> anyhow::Result<HashMap<String, bool>> {
        use std::process::Command;

        let output = Command::new("python3")
            .args([
                "-m",
                "swebench.harness.run_evaluation",
                "--dataset_name",
                dataset_name,
                "--predictions_path",
                &predictions_path.to_string_lossy(),
                "--max_workers",
                &max_workers.to_string(),
                "--run_id",
                "octo-eval",
            ])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("swebench harness failed: {}", stderr);
        }

        // Parse harness output — the harness writes results to a JSON file
        // For now, return empty map; actual parsing depends on harness output format
        let stdout = String::from_utf8_lossy(&output.stdout);
        tracing::info!("Harness output: {}", stdout);

        // TODO: Parse actual harness results from output directory
        Ok(HashMap::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_swe_bench_record_deserialize() {
        let json = r#"{"instance_id":"django__django-16527","repo":"django/django","problem_statement":"Fix issue with QuerySet"}"#;
        let record: SweBenchRecord = serde_json::from_str(json).unwrap();
        assert_eq!(record.instance_id, "django__django-16527");
        assert_eq!(record.repo, "django/django");
    }

    #[test]
    fn test_swe_difficulty_classification() {
        let easy = SweBenchRecord {
            instance_id: "test".into(),
            repo: "test/test".into(),
            base_commit: String::new(),
            patch: "line1\nline2\n".into(),
            test_patch: String::new(),
            problem_statement: String::new(),
            hints_text: String::new(),
            fail_to_pass: "[\"test_one\"]".into(),
            pass_to_pass: "[]".into(),
            version: String::new(),
            environment_setup_commit: String::new(),
            created_at: String::new(),
        };
        assert_eq!(SweBenchTask::classify_difficulty(&easy), Difficulty::Easy);
    }

    #[test]
    fn test_swe_benchmark_trait() {
        let bm = SweBenchmark::new();
        assert_eq!(bm.name(), "swe_bench");
        assert!(bm.requires_sandbox());
        assert!(bm.custom_verifier().is_none());
        assert_eq!(bm.custom_metrics().len(), 2);
    }

    #[test]
    fn test_swe_bench_load_full_dataset() {
        let path = SweBenchmark::default_dataset_path();
        if !path.exists() {
            eprintln!("Skipping: dataset not found at {}", path.display());
            return;
        }
        let tasks = SweBenchmark::load_from_jsonl(&path).unwrap();
        assert_eq!(tasks.len(), 300, "SWE-bench Lite should have 300 instances");
        // Verify first and last task have valid IDs
        assert!(!tasks[0].id().is_empty());
        assert!(!tasks[299].id().is_empty());
    }

    #[test]
    fn test_swe_bench_record_deserialize_with_new_fields() {
        let json = r#"{"instance_id":"django__django-16527","repo":"django/django","problem_statement":"Fix issue","version":"4.2","environment_setup_commit":"abc123","created_at":"2023-10-01T00:00:00Z"}"#;
        let record: SweBenchRecord = serde_json::from_str(json).unwrap();
        assert_eq!(record.version, "4.2");
        assert_eq!(record.environment_setup_commit, "abc123");
        assert_eq!(record.created_at, "2023-10-01T00:00:00Z");
    }

    #[test]
    fn test_swe_bench_extract_patch_diff_git() {
        let text = "Here is my fix:\n\ndiff --git a/django/db/models/query.py b/django/db/models/query.py\nindex abc123..def456 100644\n--- a/django/db/models/query.py\n+++ b/django/db/models/query.py\n@@ -100,3 +100,4 @@\n     def filter(self):\n-        return old\n+        return new\n\nDone!";
        let patch = SweBenchTask::extract_patch(text);
        assert!(patch.is_some());
        let p = patch.unwrap();
        assert!(p.starts_with("diff --git"));
        assert!(p.contains("+        return new"));
    }

    #[test]
    fn test_swe_bench_extract_patch_none() {
        let text = "I looked at the code but couldn't figure out the fix.";
        let patch = SweBenchTask::extract_patch(text);
        assert!(patch.is_none());
    }

    #[test]
    fn test_swe_bench_scoring_with_patch() {
        let record = SweBenchRecord {
            instance_id: "test__test-001".into(),
            repo: "test/test".into(),
            base_commit: String::new(),
            patch: String::new(),
            test_patch: String::new(),
            problem_statement: "Fix the bug".into(),
            hints_text: String::new(),
            fail_to_pass: "[]".into(),
            pass_to_pass: "[]".into(),
            version: String::new(),
            environment_setup_commit: String::new(),
            created_at: String::new(),
        };
        let task = SweBenchTask::new(record);

        // Agent produced a valid patch
        let output = AgentOutput {
            messages: vec![octo_types::ChatMessage::assistant(
                "diff --git a/test.py b/test.py\n--- a/test.py\n+++ b/test.py\n@@ -1 +1 @@\n-old\n+new\n",
            )],
            ..Default::default()
        };
        let score = task.score(&output);
        assert_eq!(score.score, 0.5); // Patch produced but not verified
        assert!(!score.passed); // Not verified by harness

        // Agent produced no patch
        let output_no_patch = AgentOutput {
            messages: vec![octo_types::ChatMessage::assistant("I couldn't fix this.")],
            ..Default::default()
        };
        let score_no = task.score(&output_no_patch);
        assert_eq!(score_no.score, 0.0);
    }

    #[test]
    fn test_swe_bench_to_prediction() {
        let record = SweBenchRecord {
            instance_id: "django__django-16527".into(),
            repo: "django/django".into(),
            base_commit: String::new(),
            patch: String::new(),
            test_patch: String::new(),
            problem_statement: "Fix issue".into(),
            hints_text: String::new(),
            fail_to_pass: "[]".into(),
            pass_to_pass: "[]".into(),
            version: String::new(),
            environment_setup_commit: String::new(),
            created_at: String::new(),
        };
        let task = SweBenchTask::new(record);
        let pred = task.to_prediction("diff --git ...", "octo-agent/test");
        assert_eq!(pred["instance_id"], "django__django-16527");
        assert_eq!(pred["model_name_or_path"], "octo-agent/test");
    }
}
