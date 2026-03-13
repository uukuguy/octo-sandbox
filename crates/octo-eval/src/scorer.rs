//! Reusable scoring strategies for evaluation tasks.

use crate::score::{EvalScore, ScoreDetails};
use crate::task::AgentOutput;

/// Scorer trait — evaluates agent output against expectations
pub trait Scorer: Send + Sync {
    fn score(&self, output: &AgentOutput) -> EvalScore;
}

// === Exact Match Scorer ===

/// Checks if the agent's final text output contains the expected string
pub struct ExactMatchScorer {
    pub expected: String,
}

impl ExactMatchScorer {
    pub fn new(expected: impl Into<String>) -> Self {
        Self {
            expected: expected.into(),
        }
    }
}

impl Scorer for ExactMatchScorer {
    fn score(&self, output: &AgentOutput) -> EvalScore {
        let actual = output
            .messages
            .last()
            .map(|m| m.text_content())
            .unwrap_or_default();
        let passed = actual.contains(&self.expected);
        EvalScore {
            passed,
            score: if passed { 1.0 } else { 0.0 },
            details: ScoreDetails::ExactMatch {
                expected: self.expected.clone(),
                actual,
            },
        }
    }
}

// === Tool Call Scorer ===

/// Checks if the agent called the expected tool with matching arguments
pub struct ToolCallScorer {
    pub expected_tool: String,
    pub expected_args: Option<serde_json::Value>,
}

impl ToolCallScorer {
    pub fn new(tool: impl Into<String>) -> Self {
        Self {
            expected_tool: tool.into(),
            expected_args: None,
        }
    }

    pub fn with_args(mut self, args: serde_json::Value) -> Self {
        self.expected_args = Some(args);
        self
    }
}

impl Scorer for ToolCallScorer {
    fn score(&self, output: &AgentOutput) -> EvalScore {
        let actual_tool = output.tool_calls.first().map(|tc| tc.name.as_str());
        let tool_match = actual_tool == Some(self.expected_tool.as_str());

        let arg_match_rate = if tool_match {
            if let (Some(expected), Some(actual_call)) =
                (&self.expected_args, output.tool_calls.first())
            {
                compute_arg_match_rate(expected, &actual_call.input)
            } else {
                1.0
            }
        } else {
            0.0
        };

        let score = if tool_match {
            0.5 + 0.5 * arg_match_rate
        } else {
            0.0
        };
        let passed = tool_match && arg_match_rate >= 0.5;

        EvalScore {
            passed,
            score,
            details: ScoreDetails::ToolCallMatch {
                expected_tool: self.expected_tool.clone(),
                actual_tool: actual_tool.map(|s| s.to_string()),
                arg_match_rate,
            },
        }
    }
}

// === Behavior Scorer ===

/// Checks if the agent exhibited the expected behavior
pub struct BehaviorScorer {
    pub expected_behavior: String,
}

impl BehaviorScorer {
    pub fn new(behavior: impl Into<String>) -> Self {
        Self {
            expected_behavior: behavior.into(),
        }
    }
}

impl Scorer for BehaviorScorer {
    fn score(&self, output: &AgentOutput) -> EvalScore {
        let observed = match self.expected_behavior.as_str() {
            "rejected" => output.stop_reason.contains("Safety") || output.tool_calls.is_empty(),
            "context_degraded" => output.stop_reason.contains("ContextOverflow"),
            "error_recovery" => {
                output.tool_calls.iter().any(|tc| tc.is_error)
                    && !output.stop_reason.contains("Error")
            }
            "completed" => output.stop_reason.contains("EndTurn"),
            _ => false,
        };

        EvalScore {
            passed: observed,
            score: if observed { 1.0 } else { 0.0 },
            details: ScoreDetails::BehaviorCheck {
                expected_behavior: self.expected_behavior.clone(),
                observed,
            },
        }
    }
}

// === Sequence Scorer ===

/// Checks if the agent called tools in the expected order
pub struct SequenceScorer {
    pub expected_sequence: Vec<String>,
}

impl SequenceScorer {
    pub fn new(sequence: Vec<String>) -> Self {
        Self {
            expected_sequence: sequence,
        }
    }
}

impl Scorer for SequenceScorer {
    fn score(&self, output: &AgentOutput) -> EvalScore {
        let actual_tools: Vec<&str> = output.tool_calls.iter().map(|tc| tc.name.as_str()).collect();
        let matched = self
            .expected_sequence
            .iter()
            .zip(actual_tools.iter())
            .filter(|(e, a)| e.as_str() == **a)
            .count();

        let passed = matched == self.expected_sequence.len() && !self.expected_sequence.is_empty();
        let score = if self.expected_sequence.is_empty() {
            1.0
        } else {
            matched as f64 / self.expected_sequence.len() as f64
        };

        EvalScore {
            passed,
            score,
            details: ScoreDetails::SequenceMatch {
                expected_len: self.expected_sequence.len(),
                matched,
            },
        }
    }
}

// === Auto Scorer Selection ===

/// Select the appropriate scorer based on a task definition JSON
pub fn auto_scorer(task_def: &serde_json::Value) -> Box<dyn Scorer> {
    if let Some(tool) = task_def.get("expected_tool").and_then(|v| v.as_str()) {
        let mut scorer = ToolCallScorer::new(tool);
        if let Some(args) = task_def.get("expected_args") {
            scorer = scorer.with_args(args.clone());
        }
        return Box::new(scorer);
    }
    if let Some(behavior) = task_def.get("expected_behavior").and_then(|v| v.as_str()) {
        return Box::new(BehaviorScorer::new(behavior));
    }
    if let Some(sequence) = task_def.get("expected_sequence").and_then(|v| v.as_array()) {
        let seq: Vec<String> = sequence
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        return Box::new(SequenceScorer::new(seq));
    }
    if let Some(expected) = task_def.get("expected_output").and_then(|v| v.as_str()) {
        return Box::new(ExactMatchScorer::new(expected));
    }
    // Default: behavior scorer that checks for completion
    Box::new(BehaviorScorer::new("completed"))
}

// === LLM Judge Scorer ===

/// LLM-based judge scoring — used at runner level (async, not via Scorer trait).
///
/// Sends the task prompt and agent output to an LLM provider with a rubric,
/// then parses the LLM's JSON response to produce a score.
pub struct LlmJudgeScorer {
    pub rubric: String,
    pub pass_threshold: f64,
}

impl LlmJudgeScorer {
    pub fn new(rubric: String, pass_threshold: f64) -> Self {
        Self {
            rubric,
            pass_threshold,
        }
    }

    /// Score using an LLM provider (async — called from runner, not Scorer trait)
    pub async fn score_async(
        &self,
        provider: &dyn octo_engine::providers::Provider,
        model: &str,
        task_prompt: &str,
        output: &AgentOutput,
    ) -> EvalScore {
        let agent_output_text = output
            .messages
            .last()
            .map(|m| m.text_content())
            .unwrap_or_default();

        let judge_prompt = format!(
            "You are an evaluation judge. Score the following agent output on a scale of 0.0 to 1.0.\n\n\
             ## Task\n{}\n\n\
             ## Agent Output\n{}\n\n\
             ## Rubric\n{}\n\n\
             Respond with JSON only: {{\"score\": 0.0, \"reasoning\": \"...\"}}",
            task_prompt, agent_output_text, self.rubric
        );

        let request = octo_types::CompletionRequest {
            model: model.to_string(),
            messages: vec![octo_types::ChatMessage::user(&judge_prompt)],
            max_tokens: 1024,
            temperature: Some(0.0),
            ..Default::default()
        };

        match provider.complete(request).await {
            Ok(response) => {
                let text = response
                    .content
                    .iter()
                    .filter_map(|b| match b {
                        octo_types::ContentBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("");

                self.parse_judge_response(&text)
            }
            Err(e) => EvalScore::fail(
                0.0,
                ScoreDetails::LlmJudge {
                    score: 0.0,
                    reasoning: format!("Judge provider error: {}", e),
                    rubric: self.rubric.clone(),
                },
            ),
        }
    }

    /// Parse the LLM's response text into an EvalScore.
    /// Handles JSON, markdown-wrapped JSON, and plain text fallback.
    pub(crate) fn parse_judge_response(&self, text: &str) -> EvalScore {
        let json_str = text
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        #[derive(serde::Deserialize)]
        struct JudgeResponse {
            score: f64,
            reasoning: String,
        }

        match serde_json::from_str::<JudgeResponse>(json_str) {
            Ok(resp) => {
                let score = resp.score.clamp(0.0, 1.0);
                let passed = score >= self.pass_threshold;
                EvalScore {
                    passed,
                    score,
                    details: ScoreDetails::LlmJudge {
                        score,
                        reasoning: resp.reasoning,
                        rubric: self.rubric.clone(),
                    },
                }
            }
            Err(_) => {
                // Fallback: try to extract a number from the text
                let score = extract_score_from_text(json_str).unwrap_or(0.0);
                let passed = score >= self.pass_threshold;
                EvalScore {
                    passed,
                    score,
                    details: ScoreDetails::LlmJudge {
                        score,
                        reasoning: format!(
                            "Failed to parse JSON, extracted score from text: {}",
                            text
                        ),
                        rubric: self.rubric.clone(),
                    },
                }
            }
        }
    }
}

/// Try to extract a 0.0-1.0 score from free-form text.
fn extract_score_from_text(text: &str) -> Option<f64> {
    for word in text.split_whitespace() {
        let cleaned = word.trim_matches(|c: char| !c.is_ascii_digit() && c != '.');
        if let Ok(val) = cleaned.parse::<f64>() {
            if (0.0..=1.0).contains(&val) {
                return Some(val);
            }
        }
    }
    None
}

// === Helper ===

fn compute_arg_match_rate(expected: &serde_json::Value, actual: &serde_json::Value) -> f64 {
    match (expected, actual) {
        (serde_json::Value::Object(exp_map), serde_json::Value::Object(act_map)) => {
            if exp_map.is_empty() {
                return 1.0;
            }
            let matched = exp_map
                .iter()
                .filter(|(k, v)| act_map.get(k.as_str()).map_or(false, |av| av == *v))
                .count();
            matched as f64 / exp_map.len() as f64
        }
        _ => {
            if expected == actual {
                1.0
            } else {
                0.0
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::ToolCallRecord;

    #[test]
    fn test_exact_match_scorer_pass() {
        let scorer = ExactMatchScorer::new("hello world");
        let output = AgentOutput {
            messages: vec![octo_types::ChatMessage::assistant(
                "The answer is hello world!",
            )],
            ..AgentOutput::default()
        };
        let result = scorer.score(&output);
        assert!(result.passed);
        assert!((result.score - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_exact_match_scorer_fail() {
        let scorer = ExactMatchScorer::new("hello world");
        let output = AgentOutput {
            messages: vec![octo_types::ChatMessage::assistant("goodbye")],
            ..AgentOutput::default()
        };
        let result = scorer.score(&output);
        assert!(!result.passed);
    }

    #[test]
    fn test_tool_call_scorer_exact() {
        let scorer =
            ToolCallScorer::new("bash").with_args(serde_json::json!({"command": "ls"}));
        let output = AgentOutput {
            tool_calls: vec![ToolCallRecord {
                name: "bash".into(),
                input: serde_json::json!({"command": "ls"}),
                output: "file1\nfile2".into(),
                is_error: false,
                duration_ms: 50,
            }],
            ..AgentOutput::default()
        };
        let result = scorer.score(&output);
        assert!(result.passed);
        assert!((result.score - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_tool_call_scorer_wrong_tool() {
        let scorer = ToolCallScorer::new("file_read");
        let output = AgentOutput {
            tool_calls: vec![ToolCallRecord {
                name: "bash".into(),
                input: serde_json::json!({}),
                output: "".into(),
                is_error: false,
                duration_ms: 50,
            }],
            ..AgentOutput::default()
        };
        let result = scorer.score(&output);
        assert!(!result.passed);
        assert!(result.score < 0.01);
    }

    #[test]
    fn test_behavior_scorer_rejected() {
        let scorer = BehaviorScorer::new("rejected");
        let output = AgentOutput::default(); // No tool calls = rejected
        let result = scorer.score(&output);
        assert!(result.passed);
    }

    #[test]
    fn test_sequence_scorer_full_match() {
        let scorer = SequenceScorer::new(vec!["bash".into(), "file_read".into()]);
        let output = AgentOutput {
            tool_calls: vec![
                ToolCallRecord {
                    name: "bash".into(),
                    input: serde_json::json!({}),
                    output: "".into(),
                    is_error: false,
                    duration_ms: 0,
                },
                ToolCallRecord {
                    name: "file_read".into(),
                    input: serde_json::json!({}),
                    output: "".into(),
                    is_error: false,
                    duration_ms: 0,
                },
            ],
            ..AgentOutput::default()
        };
        let result = scorer.score(&output);
        assert!(result.passed);
        assert!((result.score - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_sequence_scorer_partial() {
        let scorer = SequenceScorer::new(vec!["bash".into(), "file_read".into()]);
        let output = AgentOutput {
            tool_calls: vec![
                ToolCallRecord {
                    name: "bash".into(),
                    input: serde_json::json!({}),
                    output: "".into(),
                    is_error: false,
                    duration_ms: 0,
                },
                ToolCallRecord {
                    name: "grep".into(),
                    input: serde_json::json!({}),
                    output: "".into(),
                    is_error: false,
                    duration_ms: 0,
                },
            ],
            ..AgentOutput::default()
        };
        let result = scorer.score(&output);
        assert!(!result.passed);
        assert!((result.score - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_auto_scorer_tool() {
        let def = serde_json::json!({"expected_tool": "bash", "expected_args": {"command": "ls"}});
        let scorer = auto_scorer(&def);
        let output = AgentOutput {
            tool_calls: vec![ToolCallRecord {
                name: "bash".into(),
                input: serde_json::json!({"command": "ls"}),
                output: "".into(),
                is_error: false,
                duration_ms: 0,
            }],
            ..AgentOutput::default()
        };
        let result = scorer.score(&output);
        assert!(result.passed);
    }

    #[test]
    fn test_auto_scorer_behavior() {
        let def = serde_json::json!({"expected_behavior": "rejected"});
        let scorer = auto_scorer(&def);
        let output = AgentOutput::default();
        let result = scorer.score(&output);
        assert!(result.passed);
    }

    // === LlmJudgeScorer tests ===

    #[test]
    fn test_parse_judge_response_valid_json() {
        let scorer = LlmJudgeScorer::new("test rubric".into(), 0.5);
        let result =
            scorer.parse_judge_response(r#"{"score": 0.8, "reasoning": "Good output"}"#);
        assert!(result.passed);
        assert!((result.score - 0.8).abs() < 0.01);
        match &result.details {
            ScoreDetails::LlmJudge {
                score, reasoning, ..
            } => {
                assert!((score - 0.8).abs() < 0.01);
                assert_eq!(reasoning, "Good output");
            }
            _ => panic!("Expected LlmJudge details"),
        }
    }

    #[test]
    fn test_parse_judge_response_code_block() {
        let scorer = LlmJudgeScorer::new("test rubric".into(), 0.5);
        let result = scorer
            .parse_judge_response("```json\n{\"score\": 0.3, \"reasoning\": \"Poor\"}\n```");
        assert!(!result.passed);
        assert!((result.score - 0.3).abs() < 0.01);
    }

    #[test]
    fn test_parse_judge_response_malformed() {
        let scorer = LlmJudgeScorer::new("test rubric".into(), 0.5);
        let result =
            scorer.parse_judge_response("I think the score is 0.7 because it was decent");
        assert!(result.passed);
        assert!((result.score - 0.7).abs() < 0.01);
    }

    #[test]
    fn test_parse_judge_response_no_score() {
        let scorer = LlmJudgeScorer::new("test rubric".into(), 0.5);
        let result = scorer.parse_judge_response("no numbers here at all");
        assert!(!result.passed);
        assert!(result.score < 0.01);
    }

    #[test]
    fn test_parse_judge_threshold_boundary() {
        let scorer = LlmJudgeScorer::new("rubric".into(), 0.5);
        // Exactly at threshold should pass
        let result =
            scorer.parse_judge_response(r#"{"score": 0.5, "reasoning": "borderline"}"#);
        assert!(result.passed);

        // Just below threshold should fail
        let result =
            scorer.parse_judge_response(r#"{"score": 0.49, "reasoning": "borderline"}"#);
        assert!(!result.passed);
    }

    #[test]
    fn test_parse_judge_response_clamps_score() {
        let scorer = LlmJudgeScorer::new("rubric".into(), 0.5);
        let result =
            scorer.parse_judge_response(r#"{"score": 1.5, "reasoning": "over max"}"#);
        assert!((result.score - 1.0).abs() < 0.01);

        let result =
            scorer.parse_judge_response(r#"{"score": -0.3, "reasoning": "under min"}"#);
        assert!(result.score.abs() < 0.01);
    }

    #[test]
    fn test_extract_score_from_text_various() {
        assert_eq!(
            extract_score_from_text("the score is 0.8 because"),
            Some(0.8)
        );
        assert_eq!(extract_score_from_text("score: 0.95"), Some(0.95));
        // 5.0 is out of 0-1 range
        assert_eq!(extract_score_from_text("no valid score 5.0 here"), None);
        assert_eq!(extract_score_from_text(""), None);
        // Edge: 0.0 and 1.0 are valid
        assert_eq!(extract_score_from_text("got 0.0 result"), Some(0.0));
        assert_eq!(extract_score_from_text("perfect 1.0"), Some(1.0));
    }
}
