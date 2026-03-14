use serde::{Deserialize, Serialize};

/// Evaluation score for a single task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalScore {
    pub passed: bool,
    pub score: f64, // 0.0 - 1.0
    pub details: ScoreDetails,
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
}

impl EvalScore {
    pub fn pass(score: f64, details: ScoreDetails) -> Self {
        Self {
            passed: true,
            score,
            details,
        }
    }

    pub fn fail(score: f64, details: ScoreDetails) -> Self {
        Self {
            passed: false,
            score,
            details,
        }
    }
}
