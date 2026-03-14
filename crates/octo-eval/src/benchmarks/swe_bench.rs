//! SWE-bench benchmark adapter — end-to-end code repair evaluation.
//!
//! SWE-bench evaluates agent ability to fix real GitHub issues.
//! Requires Docker sandbox for full verification; supports mock fallback.

use std::path::PathBuf;

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
}

/// EvalTask implementation for a single SWE-bench task
pub struct SweBenchTask {
    record: SweBenchRecord,
}

impl SweBenchTask {
    pub fn new(record: SweBenchRecord) -> Self {
        Self { record }
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
        &self.record.problem_statement
    }

    fn available_tools(&self) -> Option<Vec<octo_types::tool::ToolSpec>> {
        None
    }

    fn tool_allowlist(&self) -> Option<Vec<String>> {
        Some(vec![
            "bash".into(),
            "file_read".into(),
            "file_write".into(),
        ])
    }

    fn score(&self, output: &AgentOutput) -> EvalScore {
        // In mock mode: check if agent produced any patch-like output
        let text = output
            .messages
            .last()
            .map(|m| m.text_content())
            .unwrap_or_default();

        let has_diff = text.contains("diff --git")
            || text.contains("--- a/")
            || text.contains("+++ b/");

        EvalScore {
            passed: has_diff,
            score: if has_diff { 0.5 } else { 0.0 },
            details: ScoreDetails::SweVerify {
                instance_id: self.record.instance_id.clone(),
                fail_to_pass_passed: false,
                pass_to_pass_passed: false,
                fail_to_pass_count: 0,
                pass_to_pass_count: 0,
                execution_time_ms: 0,
            },
        }
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
}
