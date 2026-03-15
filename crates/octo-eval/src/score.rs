use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Evaluation score for a single task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalScore {
    pub passed: bool,
    pub score: f64, // 0.0 - 1.0
    pub details: ScoreDetails,
    #[serde(default)]
    pub dimensions: HashMap<String, f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_class: Option<crate::failure::FailureClass>,
}

/// Detailed scoring information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ScoreDetails {
    ExactMatch {
        expected: String,
        actual: String,
    },
    ToolCallMatch {
        expected_tool: String,
        actual_tool: Option<String>,
        arg_match_rate: f64,
    },
    SequenceMatch {
        expected_len: usize,
        matched: usize,
    },
    BehaviorCheck {
        expected_behavior: String,
        observed: bool,
    },
    Custom {
        message: String,
    },
    Timeout {
        elapsed_secs: u64,
    },
    LlmJudge {
        score: f64,
        reasoning: String,
        rubric: String,
    },
    PatchVerify {
        test_cmd: String,
        test_output: String,
        exit_code: i32,
    },
    FunctionCallMatch {
        expected_call: String,
        actual_tool: Option<String>,
        arg_match_rate: f64,
    },
    RegexMatch {
        pattern: String,
        matched: bool,
    },
    NotContains {
        forbidden: Vec<String>,
        found: Vec<String>,
    },
    SequenceWithArgsMatch {
        expected_len: usize,
        matched: usize,
        arg_match_rates: Vec<f64>,
    },
    ContainsAll {
        expected: Vec<String>,
        matched: usize,
        total: usize,
    },
    AstMatch {
        expected_tool: String,
        actual_tool: Option<String>,
        arg_match_rate: f64,
        mismatched_fields: Vec<String>,
    },
    /// SWE-bench Docker verification result
    SweVerify {
        instance_id: String,
        fail_to_pass_passed: bool,
        pass_to_pass_passed: bool,
        fail_to_pass_count: usize,
        pass_to_pass_count: usize,
        execution_time_ms: u64,
    },
    /// τ-bench pass^k consistency metric
    PassK {
        k: u32,
        passes: u32,
        pass_at_1: f64,
        pass_at_k: f64,
    },
    /// GAIA exact-match with level metadata
    GaiaMatch {
        expected: String,
        actual: String,
        level: u32,
    },
    /// Event sequence validation from agent execution traces
    EventSequence {
        expected_sequence: Vec<String>,
        actual_sequence: Vec<String>,
        sequence_correctness: f64,
        completion_ratio: f64,
    },
    /// Platform-level behavior verification
    PlatformBehavior {
        expected_behavior: String,
        observed: bool,
        evidence: String,
    },
    /// Terminal-Bench command orchestration scoring
    TerminalBench {
        command_match_rate: f64,
        output_match_rate: f64,
        forbidden_found: Vec<String>,
        level: u32,
    },
}

impl EvalScore {
    pub fn pass(score: f64, details: ScoreDetails) -> Self {
        Self {
            passed: true,
            score,
            details,
            dimensions: HashMap::new(),
            failure_class: None,
        }
    }

    pub fn fail(score: f64, details: ScoreDetails) -> Self {
        Self {
            passed: false,
            score,
            details,
            dimensions: HashMap::new(),
            failure_class: None,
        }
    }
}
