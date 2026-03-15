//! Terminal-Bench benchmark adapter — terminal command orchestration evaluation.
//!
//! Evaluates agent capability in terminal operations: command composition,
//! file manipulation, system administration, and multi-step shell workflows.
//! Level 1: single command, Level 2: pipeline/composition, Level 3: multi-step orchestration.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::Deserialize;

use crate::benchmarks::{ExternalBenchmark, MetricDefinition};
use crate::score::{EvalScore, ScoreDetails};
use crate::task::{AgentOutput, Difficulty, EvalTask, TaskMetadata};

/// A single Terminal-Bench evaluation record parsed from JSONL
#[derive(Debug, Clone, Deserialize)]
pub struct TerminalBenchRecord {
    pub task_id: String,
    pub instruction: String,
    pub category: String,
    pub level: u32,
    #[serde(default)]
    pub expected_commands: Vec<String>,
    #[serde(default)]
    pub expected_output_contains: Vec<String>,
    #[serde(default)]
    pub forbidden_patterns: Vec<String>,
}

/// EvalTask implementation for a single Terminal-Bench task
pub struct TerminalBenchTask {
    record: TerminalBenchRecord,
}

impl TerminalBenchTask {
    pub fn new(record: TerminalBenchRecord) -> Self {
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

impl EvalTask for TerminalBenchTask {
    fn id(&self) -> &str {
        &self.record.task_id
    }

    fn prompt(&self) -> &str {
        &self.record.instruction
    }

    fn available_tools(&self) -> Option<Vec<octo_types::tool::ToolSpec>> {
        None
    }

    fn score(&self, output: &AgentOutput) -> EvalScore {
        let actual_text = output
            .messages
            .last()
            .map(|m| m.text_content())
            .unwrap_or_default();
        let actual_lower = actual_text.to_lowercase();

        let actual_tools: Vec<&str> = output.tool_calls.iter().map(|tc| tc.name.as_str()).collect();

        // Score component 1: command sequence matching (subsequence match)
        let cmd_score = if self.record.expected_commands.is_empty() {
            1.0
        } else {
            let mut matched = 0usize;
            let mut search_start = 0usize;
            for expected_cmd in &self.record.expected_commands {
                let expected_lower = expected_cmd.to_lowercase();
                // Check tool calls first
                let found_in_tools = actual_tools.iter().skip(search_start).enumerate().any(|(i, tc)| {
                    if tc.to_lowercase().contains(&expected_lower) || expected_lower.contains(&tc.to_lowercase()) {
                        search_start = search_start + i + 1;
                        true
                    } else {
                        false
                    }
                });
                // Also check text output for command references
                let found_in_text = actual_lower.contains(&expected_lower);
                if found_in_tools || found_in_text {
                    matched += 1;
                }
            }
            matched as f64 / self.record.expected_commands.len() as f64
        };

        // Score component 2: output content verification
        let output_score = if self.record.expected_output_contains.is_empty() {
            1.0
        } else {
            let matched = self
                .record
                .expected_output_contains
                .iter()
                .filter(|pat| actual_lower.contains(&pat.to_lowercase()))
                .count();
            matched as f64 / self.record.expected_output_contains.len() as f64
        };

        // Score component 3: forbidden pattern check (penalty)
        let forbidden_found: Vec<String> = self
            .record
            .forbidden_patterns
            .iter()
            .filter(|pat| actual_lower.contains(&pat.to_lowercase()))
            .cloned()
            .collect();
        let forbidden_penalty = if self.record.forbidden_patterns.is_empty() {
            0.0
        } else {
            forbidden_found.len() as f64 / self.record.forbidden_patterns.len() as f64
        };

        // Combined score: weighted average minus penalty
        let raw_score = (cmd_score * 0.6 + output_score * 0.4) * (1.0 - forbidden_penalty * 0.5);
        let score = raw_score.clamp(0.0, 1.0);
        let passed = score >= 0.5;

        EvalScore {
            passed,
            score,
            details: ScoreDetails::TerminalBench {
                command_match_rate: cmd_score,
                output_match_rate: output_score,
                forbidden_found,
                level: self.record.level,
            },
            dimensions: HashMap::new(),
            failure_class: None,
        }
    }

    fn metadata(&self) -> TaskMetadata {
        TaskMetadata {
            category: format!("terminal-L{}", self.record.level),
            difficulty: Self::classify_difficulty(self.record.level),
            expected_steps: Some(self.record.expected_commands.len().max(1) as u32),
            tags: vec!["external".into(), "terminal_bench".into()],
        }
    }
}

/// Terminal-Bench benchmark adapter
pub struct TerminalBenchmark {
    dataset_path: Option<PathBuf>,
}

impl TerminalBenchmark {
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
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("datasets/terminal_bench.jsonl")
    }

    pub fn load_from_jsonl(path: &std::path::Path) -> anyhow::Result<Vec<Box<dyn EvalTask>>> {
        let content = std::fs::read_to_string(path)?;
        let mut tasks: Vec<Box<dyn EvalTask>> = Vec::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let record: TerminalBenchRecord = serde_json::from_str(line)?;
            tasks.push(Box::new(TerminalBenchTask::new(record)));
        }

        Ok(tasks)
    }
}

impl ExternalBenchmark for TerminalBenchmark {
    fn name(&self) -> &str {
        "terminal_bench"
    }

    fn description(&self) -> &str {
        "Terminal-Bench — terminal command orchestration evaluation (L1-L3)"
    }

    fn load_tasks(&self) -> anyhow::Result<Vec<Box<dyn EvalTask>>> {
        let path = self
            .dataset_path
            .clone()
            .unwrap_or_else(Self::default_dataset_path);

        if !path.exists() {
            anyhow::bail!(
                "Terminal-Bench dataset not found at {}. Create terminal_bench.jsonl.",
                path.display()
            );
        }

        Self::load_from_jsonl(&path)
    }

    fn custom_metrics(&self) -> Vec<MetricDefinition> {
        vec![
            MetricDefinition {
                name: "pass_rate_l1".into(),
                description: "Pass rate for Level 1 (single command) tasks".into(),
                unit: crate::benchmarks::MetricUnit::Percentage,
            },
            MetricDefinition {
                name: "pass_rate_l2".into(),
                description: "Pass rate for Level 2 (pipeline/composition) tasks".into(),
                unit: crate::benchmarks::MetricUnit::Percentage,
            },
            MetricDefinition {
                name: "pass_rate_l3".into(),
                description: "Pass rate for Level 3 (multi-step orchestration) tasks".into(),
                unit: crate::benchmarks::MetricUnit::Percentage,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_record_deserialize() {
        let json = r#"{"task_id":"tb-L1-001","instruction":"List all .rs files","category":"file_ops","level":1,"expected_commands":["find"],"expected_output_contains":[".rs"]}"#;
        let record: TerminalBenchRecord = serde_json::from_str(json).unwrap();
        assert_eq!(record.task_id, "tb-L1-001");
        assert_eq!(record.level, 1);
        assert_eq!(record.expected_commands.len(), 1);
        assert!(record.forbidden_patterns.is_empty());
    }

    #[test]
    fn test_terminal_difficulty_classification() {
        assert_eq!(TerminalBenchTask::classify_difficulty(1), Difficulty::Easy);
        assert_eq!(TerminalBenchTask::classify_difficulty(2), Difficulty::Medium);
        assert_eq!(TerminalBenchTask::classify_difficulty(3), Difficulty::Hard);
    }

    #[test]
    fn test_terminal_scoring_pass() {
        let record = TerminalBenchRecord {
            task_id: "test-001".into(),
            instruction: "Find all Python files".into(),
            category: "file_ops".into(),
            level: 1,
            expected_commands: vec!["find".into()],
            expected_output_contains: vec![".py".into()],
            forbidden_patterns: vec![],
        };
        let task = TerminalBenchTask::new(record);

        let output = AgentOutput {
            messages: vec![octo_types::ChatMessage::assistant(
                "I used find to locate .py files:\nfind . -name '*.py'\nfound main.py, utils.py",
            )],
            ..Default::default()
        };
        let score = task.score(&output);
        assert!(score.passed);
        assert!(score.score > 0.5);
    }

    #[test]
    fn test_terminal_scoring_forbidden() {
        let record = TerminalBenchRecord {
            task_id: "test-002".into(),
            instruction: "Delete temp files safely".into(),
            category: "file_ops".into(),
            level: 2,
            expected_commands: vec!["find".into(), "rm".into()],
            expected_output_contains: vec![],
            forbidden_patterns: vec!["rm -rf /".into()],
        };
        let task = TerminalBenchTask::new(record);

        let output = AgentOutput {
            messages: vec![octo_types::ChatMessage::assistant(
                "rm -rf / is dangerous, instead use find /tmp -name '*.tmp' -delete",
            )],
            ..Default::default()
        };
        let score = task.score(&output);
        // Score penalized due to forbidden pattern found
        assert!(score.score < 1.0);
    }

    #[test]
    fn test_terminal_benchmark_trait() {
        let bm = TerminalBenchmark::new();
        assert_eq!(bm.name(), "terminal_bench");
        assert!(!bm.requires_sandbox());
        assert!(bm.sandbox_available());
        assert!(bm.custom_verifier().is_none());
        assert_eq!(bm.custom_metrics().len(), 3);
    }
}
