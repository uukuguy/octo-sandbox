//! Trace recorder for evaluation runs -- captures and persists complete evaluation traces.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::mock_provider::{RecordedInteraction, SerializableContent, SerializableResponse};
use crate::score::EvalScore;
use crate::task::AgentOutput;

/// Complete trace of a single evaluation task execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalTrace {
    pub task_id: String,
    pub timestamp: String,
    pub interactions: Vec<RecordedInteraction>,
    pub output: AgentOutput,
    pub score: EvalScore,
}

/// Records evaluation traces to disk
pub struct EvalRecorder {
    output_dir: PathBuf,
}

impl EvalRecorder {
    pub fn new(output_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&output_dir)?;
        Ok(Self { output_dir })
    }

    /// Save a complete evaluation trace as a pretty-printed JSON file.
    pub fn save_trace(&self, trace: &EvalTrace) -> Result<PathBuf> {
        let filename = format!("trace_{}.json", trace.task_id.replace('/', "_"));
        let path = self.output_dir.join(&filename);
        let json = serde_json::to_string_pretty(trace)?;
        std::fs::write(&path, json)?;
        tracing::info!(task_id = %trace.task_id, path = %path.display(), "Saved evaluation trace");
        Ok(path)
    }

    /// Load a trace from disk.
    pub fn load_trace(path: &Path) -> Result<EvalTrace> {
        let content = std::fs::read_to_string(path)?;
        let trace: EvalTrace = serde_json::from_str(&content)?;
        Ok(trace)
    }

    /// Save all traces as a single JSONL summary file.
    pub fn save_summary(&self, traces: &[EvalTrace]) -> Result<PathBuf> {
        let path = self.output_dir.join("eval_traces.jsonl");
        let mut f = std::fs::File::create(&path)?;
        for trace in traces {
            let line = serde_json::to_string(trace)?;
            writeln!(f, "{}", line)?;
        }
        tracing::info!(count = traces.len(), path = %path.display(), "Saved evaluation summary");
        Ok(path)
    }

    /// Load all traces from a JSONL summary file.
    pub fn load_summary(path: &Path) -> Result<Vec<EvalTrace>> {
        let content = std::fs::read_to_string(path)?;
        let traces: Vec<EvalTrace> = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(traces)
    }

    /// Extract recorded interactions from a trace for use with ReplayProvider.
    pub fn extract_interactions(trace: &EvalTrace) -> Vec<RecordedInteraction> {
        trace.interactions.clone()
    }

    /// Get the output directory path.
    pub fn output_dir(&self) -> &Path {
        &self.output_dir
    }
}

/// Helper to create a [`RecordedInteraction`] from request/response data.
pub fn record_interaction(
    request_summary: &str,
    response_id: &str,
    content_text: &str,
    stop_reason: &str,
    input_tokens: u32,
    output_tokens: u32,
    latency_ms: u64,
) -> RecordedInteraction {
    RecordedInteraction {
        request_summary: request_summary.to_string(),
        response: SerializableResponse {
            id: response_id.to_string(),
            content: vec![SerializableContent::Text {
                text: content_text.to_string(),
            }],
            stop_reason: Some(stop_reason.to_string()),
            input_tokens,
            output_tokens,
        },
        latency_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::ScoreDetails;

    fn sample_trace() -> EvalTrace {
        EvalTrace {
            task_id: "test-001".into(),
            timestamp: "2026-03-13T20:00:00Z".into(),
            interactions: vec![record_interaction(
                "user: hello",
                "resp-1",
                "Hello! How can I help?",
                "end_turn",
                10,
                5,
                150,
            )],
            output: AgentOutput {
                rounds: 1,
                input_tokens: 10,
                output_tokens: 5,
                stop_reason: "EndTurn".into(),
                ..AgentOutput::default()
            },
            score: EvalScore::pass(
                1.0,
                ScoreDetails::Custom {
                    message: "test pass".into(),
                },
            ),
        }
    }

    #[test]
    fn test_save_and_load_trace() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = EvalRecorder::new(dir.path().to_path_buf()).unwrap();

        let trace = sample_trace();
        let path = recorder.save_trace(&trace).unwrap();

        let loaded = EvalRecorder::load_trace(&path).unwrap();
        assert_eq!(loaded.task_id, "test-001");
        assert_eq!(loaded.output.rounds, 1);
        assert!(loaded.score.passed);
    }

    #[test]
    fn test_save_and_load_summary() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = EvalRecorder::new(dir.path().to_path_buf()).unwrap();

        let traces = vec![sample_trace(), {
            let mut t = sample_trace();
            t.task_id = "test-002".into();
            t
        }];

        let path = recorder.save_summary(&traces).unwrap();
        let loaded = EvalRecorder::load_summary(&path).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].task_id, "test-001");
        assert_eq!(loaded[1].task_id, "test-002");
    }

    #[test]
    fn test_extract_interactions() {
        let trace = sample_trace();
        let interactions = EvalRecorder::extract_interactions(&trace);
        assert_eq!(interactions.len(), 1);
        assert_eq!(interactions[0].request_summary, "user: hello");
    }
}
