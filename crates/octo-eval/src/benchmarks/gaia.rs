//! GAIA benchmark adapter — multi-step reasoning + multi-tool evaluation.
//!
//! GAIA (General AI Assistants) evaluates multi-step reasoning with exact-match scoring.
//! Level 1: single-step, Level 2: multi-step + multi-tool, Level 3: complex long-chain.

use std::path::PathBuf;

use serde::Deserialize;

use crate::benchmarks::{ExternalBenchmark, MetricDefinition};
use crate::score::{EvalScore, ScoreDetails};
use crate::task::{AgentOutput, Difficulty, EvalTask, TaskMetadata};

/// A single GAIA evaluation record parsed from JSONL
#[derive(Debug, Clone, Deserialize)]
pub struct GaiaRecord {
    pub task_id: String,
    pub question: String,
    pub final_answer: String,
    pub level: u32,
    #[serde(default)]
    pub annotator_metadata: Option<GaiaAnnotation>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GaiaAnnotation {
    #[serde(default)]
    pub steps: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub num_steps: u32,
}

/// EvalTask implementation for a single GAIA task
pub struct GaiaTask {
    record: GaiaRecord,
}

impl GaiaTask {
    pub fn new(record: GaiaRecord) -> Self {
        Self { record }
    }

    fn classify_difficulty(level: u32) -> Difficulty {
        match level {
            1 => Difficulty::Easy,
            2 => Difficulty::Medium,
            _ => Difficulty::Hard,
        }
    }
}

impl EvalTask for GaiaTask {
    fn id(&self) -> &str {
        &self.record.task_id
    }

    fn prompt(&self) -> &str {
        &self.record.question
    }

    fn available_tools(&self) -> Option<Vec<octo_types::tool::ToolSpec>> {
        None
    }

    fn score(&self, output: &AgentOutput) -> EvalScore {
        let actual = output
            .messages
            .last()
            .map(|m| m.text_content())
            .unwrap_or_default();

        let normalized_expected = self.record.final_answer.trim().to_lowercase();
        let normalized_actual = actual.trim().to_lowercase();

        let passed = normalized_actual.contains(&normalized_expected);
        EvalScore {
            passed,
            score: if passed { 1.0 } else { 0.0 },
            details: ScoreDetails::GaiaMatch {
                expected: self.record.final_answer.clone(),
                actual,
                level: self.record.level,
            },
        }
    }

    fn metadata(&self) -> TaskMetadata {
        TaskMetadata {
            category: format!("gaia-L{}", self.record.level),
            difficulty: Self::classify_difficulty(self.record.level),
            expected_steps: self
                .record
                .annotator_metadata
                .as_ref()
                .map(|m| m.num_steps),
            tags: vec!["external".into(), "gaia".into()],
        }
    }
}

/// GAIA benchmark adapter
pub struct GaiaBenchmark {
    dataset_path: Option<PathBuf>,
}

impl GaiaBenchmark {
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
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("datasets/gaia_sample.jsonl")
    }

    pub fn load_from_jsonl(path: &std::path::Path) -> anyhow::Result<Vec<Box<dyn EvalTask>>> {
        let content = std::fs::read_to_string(path)?;
        let mut tasks: Vec<Box<dyn EvalTask>> = Vec::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let record: GaiaRecord = serde_json::from_str(line)?;
            tasks.push(Box::new(GaiaTask::new(record)));
        }

        Ok(tasks)
    }
}

impl ExternalBenchmark for GaiaBenchmark {
    fn name(&self) -> &str {
        "gaia"
    }

    fn description(&self) -> &str {
        "GAIA — General AI Assistants: multi-step reasoning + multi-tool evaluation (L1-L3)"
    }

    fn load_tasks(&self) -> anyhow::Result<Vec<Box<dyn EvalTask>>> {
        let path = self
            .dataset_path
            .clone()
            .unwrap_or_else(Self::default_dataset_path);

        if !path.exists() {
            anyhow::bail!(
                "GAIA dataset not found at {}. Download or create gaia_sample.jsonl.",
                path.display()
            );
        }

        Self::load_from_jsonl(&path)
    }

    fn custom_metrics(&self) -> Vec<MetricDefinition> {
        vec![
            MetricDefinition {
                name: "pass_rate_l1".into(),
                description: "Pass rate for Level 1 (easy) tasks".into(),
                unit: crate::benchmarks::MetricUnit::Percentage,
            },
            MetricDefinition {
                name: "pass_rate_l2".into(),
                description: "Pass rate for Level 2 (medium) tasks".into(),
                unit: crate::benchmarks::MetricUnit::Percentage,
            },
            MetricDefinition {
                name: "pass_rate_l3".into(),
                description: "Pass rate for Level 3 (hard) tasks".into(),
                unit: crate::benchmarks::MetricUnit::Percentage,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gaia_record_deserialize() {
        let json = r#"{"task_id":"gaia-L1-001","question":"How many studios?","final_answer":"3","level":1}"#;
        let record: GaiaRecord = serde_json::from_str(json).unwrap();
        assert_eq!(record.task_id, "gaia-L1-001");
        assert_eq!(record.level, 1);
        assert_eq!(record.final_answer, "3");
    }

    #[test]
    fn test_gaia_difficulty_classification() {
        assert_eq!(GaiaTask::classify_difficulty(1), Difficulty::Easy);
        assert_eq!(GaiaTask::classify_difficulty(2), Difficulty::Medium);
        assert_eq!(GaiaTask::classify_difficulty(3), Difficulty::Hard);
    }

    #[test]
    fn test_gaia_scoring() {
        let record = GaiaRecord {
            task_id: "test-001".into(),
            question: "What is 2+2?".into(),
            final_answer: "4".into(),
            level: 1,
            annotator_metadata: None,
        };
        let task = GaiaTask::new(record);

        // Pass case
        let output = AgentOutput {
            messages: vec![octo_types::ChatMessage::assistant("The answer is 4.")],
            ..Default::default()
        };
        let score = task.score(&output);
        assert!(score.passed);
        assert_eq!(score.score, 1.0);

        // Fail case
        let output_fail = AgentOutput {
            messages: vec![octo_types::ChatMessage::assistant("I don't know.")],
            ..Default::default()
        };
        let score_fail = task.score(&output_fail);
        assert!(!score_fail.passed);
        assert_eq!(score_fail.score, 0.0);
    }

    #[test]
    fn test_gaia_benchmark_trait() {
        let bm = GaiaBenchmark::new();
        assert_eq!(bm.name(), "gaia");
        assert!(!bm.requires_sandbox());
        assert!(bm.sandbox_available());
        assert!(bm.custom_verifier().is_none());
        assert_eq!(bm.custom_metrics().len(), 3);
    }
}
