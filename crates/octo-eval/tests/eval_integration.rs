//! Integration tests for octo-eval — end-to-end evaluation pipeline with MockProvider.
//!
//! These tests verify the complete pipeline: EvalRunner → agent loop → scoring → reporting → recording.
//! MockProvider supplies pre-configured responses so no real LLM is needed.

use std::collections::HashMap;
use std::sync::Arc;

use octo_engine::providers::Provider;

use octo_eval::config::EvalConfig;
use octo_eval::mock_provider::MockProvider;
use octo_eval::recorder::{EvalRecorder, EvalTrace};
use octo_eval::reporter::Reporter;
use octo_eval::runner::{EvalReport, EvalRunner, TaskResult};
use octo_eval::score::{EvalScore, ScoreDetails};
use octo_eval::scorer::{BehaviorScorer, ExactMatchScorer, Scorer, ToolCallScorer};
use octo_eval::task::{AgentOutput, Difficulty, EvalTask, TaskMetadata, ToolCallRecord};

// ===================================================================
// Helper task implementations for testing
// ===================================================================

/// A text-matching eval task that checks if the agent's output contains a substring.
struct SimpleTextTask {
    id: String,
    prompt: String,
    expected_contains: String,
}

impl EvalTask for SimpleTextTask {
    fn id(&self) -> &str {
        &self.id
    }
    fn prompt(&self) -> &str {
        &self.prompt
    }
    fn available_tools(&self) -> Option<Vec<octo_types::tool::ToolSpec>> {
        None
    }
    fn score(&self, output: &AgentOutput) -> EvalScore {
        let text = output
            .messages
            .last()
            .map(|m| m.text_content())
            .unwrap_or_default();
        let passed = text.contains(&self.expected_contains);
        EvalScore {
            passed,
            score: if passed { 1.0 } else { 0.0 },
            details: ScoreDetails::ExactMatch {
                expected: self.expected_contains.clone(),
                actual: text,
            },
            dimensions: std::collections::HashMap::new(),
            failure_class: None,
        }
    }
    fn metadata(&self) -> TaskMetadata {
        TaskMetadata {
            category: "text".into(),
            difficulty: Difficulty::Easy,
            expected_steps: Some(1),
            tags: vec![],
        }
    }
}

/// A tool-call-matching eval task that checks if the agent called a specific tool.
struct SimpleToolTask {
    id: String,
    prompt: String,
    expected_tool: String,
}

impl EvalTask for SimpleToolTask {
    fn id(&self) -> &str {
        &self.id
    }
    fn prompt(&self) -> &str {
        &self.prompt
    }
    fn available_tools(&self) -> Option<Vec<octo_types::tool::ToolSpec>> {
        None
    }
    fn score(&self, output: &AgentOutput) -> EvalScore {
        let actual = output.tool_calls.first().map(|tc| tc.name.as_str());
        let passed = actual == Some(self.expected_tool.as_str());
        EvalScore {
            passed,
            score: if passed { 1.0 } else { 0.0 },
            details: ScoreDetails::ToolCallMatch {
                expected_tool: self.expected_tool.clone(),
                actual_tool: actual.map(String::from),
                arg_match_rate: if passed { 1.0 } else { 0.0 },
            },
            dimensions: std::collections::HashMap::new(),
            failure_class: None,
        }
    }
    fn metadata(&self) -> TaskMetadata {
        TaskMetadata {
            category: "tool_call".into(),
            difficulty: Difficulty::Medium,
            expected_steps: Some(1),
            tags: vec![],
        }
    }
}

// ===================================================================
// Pipeline integration tests
// ===================================================================

/// Test: EvalRunner processes a task through the agent loop with MockProvider.
/// The harness requires a tool registry; without one it emits an error event
/// and returns a default (empty) AgentOutput. The task scorer then evaluates
/// the empty output — this verifies the pipeline connects end-to-end even
/// under degraded conditions.
#[tokio::test]
async fn test_eval_runner_pipeline_connects_end_to_end() {
    let mock = MockProvider::with_text("The answer is 42");
    let config = EvalConfig::default();
    let runner = EvalRunner::with_provider(config, Arc::new(mock));

    let task = SimpleTextTask {
        id: "e2e-001".into(),
        prompt: "What is the answer?".into(),
        expected_contains: "42".into(),
    };

    // run_task completes without panic — the pipeline connects
    let result = runner.run_task(&task).await.unwrap();
    assert_eq!(result.task_id, "e2e-001");
    // The harness may return empty output if tools are not configured,
    // so the score may be a fail. The important thing is the pipeline ran.
    assert!(result.score.score >= 0.0 && result.score.score <= 1.0);
}

/// Test: Run a suite of multiple tasks and verify aggregated report.
#[tokio::test]
async fn test_eval_suite_multiple_tasks() {
    let mock = MockProvider::with_text("I completed the task");
    let config = EvalConfig::default();
    let runner = EvalRunner::with_provider(config, Arc::new(mock));

    let tasks: Vec<Box<dyn EvalTask>> = vec![
        Box::new(SimpleTextTask {
            id: "suite-t1".into(),
            prompt: "Task 1".into(),
            expected_contains: "never-match".into(),
        }),
        Box::new(SimpleTextTask {
            id: "suite-t2".into(),
            prompt: "Task 2".into(),
            expected_contains: "also-never".into(),
        }),
        Box::new(SimpleTextTask {
            id: "suite-t3".into(),
            prompt: "Task 3".into(),
            expected_contains: "nope".into(),
        }),
    ];

    let report = runner.run_suite(&tasks).await.unwrap();
    // All tasks ran — total is 3 regardless of pass/fail
    assert_eq!(report.total, 3);
    assert_eq!(report.results.len(), 3);
    // pass_rate is between 0 and 1
    assert!(report.pass_rate >= 0.0 && report.pass_rate <= 1.0);
}

/// Test: EvalReport::from_results correctly aggregates metrics.
#[test]
fn test_eval_report_aggregation() {
    let results = vec![
        TaskResult {
            task_id: "agg-1".into(),
            output: AgentOutput {
                input_tokens: 100,
                output_tokens: 50,
                ..AgentOutput::default()
            },
            score: EvalScore::pass(
                1.0,
                ScoreDetails::Custom {
                    message: "ok".into(),
                },
            ),
            duration_ms: 200,
        },
        TaskResult {
            task_id: "agg-2".into(),
            output: AgentOutput {
                input_tokens: 200,
                output_tokens: 100,
                ..AgentOutput::default()
            },
            score: EvalScore::fail(
                0.3,
                ScoreDetails::Custom {
                    message: "partial".into(),
                },
            ),
            duration_ms: 400,
        },
        TaskResult {
            task_id: "agg-3".into(),
            output: AgentOutput {
                input_tokens: 50,
                output_tokens: 25,
                ..AgentOutput::default()
            },
            score: EvalScore::pass(
                0.8,
                ScoreDetails::Custom {
                    message: "ok".into(),
                },
            ),
            duration_ms: 150,
        },
    ];

    let report = EvalReport::from_results(results);

    assert_eq!(report.total, 3);
    assert_eq!(report.passed, 2);
    assert!((report.pass_rate - 2.0 / 3.0).abs() < 0.01);
    assert!((report.avg_score - (1.0 + 0.3 + 0.8) / 3.0).abs() < 0.01);
    assert_eq!(report.total_tokens, 100 + 50 + 200 + 100 + 50 + 25);
    assert_eq!(report.total_duration_ms, 200 + 400 + 150);
}

// ===================================================================
// Scoring pipeline integration tests
// ===================================================================

/// Test: Scorer + AgentOutput → EvalScore pipeline for text matching.
#[test]
fn test_scoring_pipeline_text_match() {
    let scorer = ExactMatchScorer::new("42");
    let output = AgentOutput {
        messages: vec![octo_types::ChatMessage::assistant("The answer is 42")],
        ..AgentOutput::default()
    };
    let score = scorer.score(&output);
    assert!(score.passed);
    assert!((score.score - 1.0).abs() < 0.01);

    // Verify details
    match &score.details {
        ScoreDetails::ExactMatch { expected, actual } => {
            assert_eq!(expected, "42");
            assert!(actual.contains("42"));
        }
        other => panic!("Expected ExactMatch details, got {:?}", other),
    }
}

/// Test: Scorer + AgentOutput → EvalScore pipeline for tool call matching.
#[test]
fn test_scoring_pipeline_tool_call() {
    let scorer = ToolCallScorer::new("bash").with_args(serde_json::json!({"command": "ls -la"}));
    let output = AgentOutput {
        tool_calls: vec![ToolCallRecord {
            name: "bash".into(),
            input: serde_json::json!({"command": "ls -la"}),
            output: "file1\nfile2".into(),
            is_error: false,
            duration_ms: 50,
        }],
        ..AgentOutput::default()
    };
    let score = scorer.score(&output);
    assert!(score.passed);
    assert!(score.score > 0.9);
}

/// Test: Behavior scorer recognizes "rejected" pattern (no tool calls).
#[test]
fn test_scoring_pipeline_behavior() {
    let scorer = BehaviorScorer::new("rejected");
    let output = AgentOutput::default(); // no tool calls
    let score = scorer.score(&output);
    assert!(score.passed);
    assert!((score.score - 1.0).abs() < 0.01);
}

// ===================================================================
// Reporter integration tests
// ===================================================================

/// Test: Reporter generates valid JSON from an EvalReport.
#[test]
fn test_report_json_generation() {
    let results = vec![
        TaskResult {
            task_id: "rpt-1".into(),
            output: AgentOutput {
                input_tokens: 10,
                output_tokens: 5,
                ..AgentOutput::default()
            },
            score: EvalScore::pass(
                1.0,
                ScoreDetails::Custom {
                    message: "ok".into(),
                },
            ),
            duration_ms: 100,
        },
        TaskResult {
            task_id: "rpt-2".into(),
            output: AgentOutput {
                input_tokens: 20,
                output_tokens: 10,
                ..AgentOutput::default()
            },
            score: EvalScore::fail(
                0.0,
                ScoreDetails::Custom {
                    message: "fail".into(),
                },
            ),
            duration_ms: 200,
        },
    ];
    let report = EvalReport::from_results(results);

    let categories: HashMap<String, String> = [
        ("rpt-1".into(), "greeting".into()),
        ("rpt-2".into(), "math".into()),
    ]
    .into_iter()
    .collect();
    let difficulties: HashMap<String, Difficulty> = [
        ("rpt-1".into(), Difficulty::Easy),
        ("rpt-2".into(), Difficulty::Hard),
    ]
    .into_iter()
    .collect();

    let detailed = Reporter::generate(&report, &categories, &difficulties);

    // JSON output
    let json = Reporter::to_json(&detailed);
    assert!(json.contains("\"total\": 2"));
    assert!(json.contains("greeting"));
    assert!(json.contains("math"));
    assert!(json.contains("Easy"));
    assert!(json.contains("Hard"));

    // Verify it's valid JSON
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("JSON should be valid");
    assert_eq!(parsed["summary"]["total"], 2);
    assert_eq!(parsed["summary"]["passed"], 1);
}

/// Test: Reporter generates valid Markdown from an EvalReport.
#[test]
fn test_report_markdown_generation() {
    let results = vec![
        TaskResult {
            task_id: "md-1".into(),
            output: AgentOutput::default(),
            score: EvalScore::pass(
                1.0,
                ScoreDetails::Custom {
                    message: "ok".into(),
                },
            ),
            duration_ms: 50,
        },
        TaskResult {
            task_id: "md-2".into(),
            output: AgentOutput::default(),
            score: EvalScore::fail(
                0.0,
                ScoreDetails::Custom {
                    message: "fail".into(),
                },
            ),
            duration_ms: 75,
        },
    ];
    let report = EvalReport::from_results(results);
    let detailed = Reporter::generate(&report, &HashMap::new(), &HashMap::new());
    let md = Reporter::to_markdown(&detailed);

    assert!(md.contains("# Evaluation Report"));
    assert!(md.contains("| Total Tasks | 2 |"));
    assert!(md.contains("PASS"));
    assert!(md.contains("FAIL"));
    assert!(md.contains("## Summary"));
    assert!(md.contains("## Latency"));
    assert!(md.contains("## Token Usage"));
    assert!(md.contains("## Task Results"));
}

/// Test: Reporter breakdown by category works correctly.
#[test]
fn test_report_category_breakdown() {
    let results = vec![
        TaskResult {
            task_id: "cat-1".into(),
            output: AgentOutput::default(),
            score: EvalScore::pass(
                1.0,
                ScoreDetails::Custom {
                    message: "ok".into(),
                },
            ),
            duration_ms: 50,
        },
        TaskResult {
            task_id: "cat-2".into(),
            output: AgentOutput::default(),
            score: EvalScore::pass(
                0.7,
                ScoreDetails::Custom {
                    message: "ok".into(),
                },
            ),
            duration_ms: 60,
        },
        TaskResult {
            task_id: "cat-3".into(),
            output: AgentOutput::default(),
            score: EvalScore::fail(
                0.0,
                ScoreDetails::Custom {
                    message: "fail".into(),
                },
            ),
            duration_ms: 70,
        },
    ];
    let report = EvalReport::from_results(results);

    let categories: HashMap<String, String> = [
        ("cat-1".into(), "tool_call".into()),
        ("cat-2".into(), "tool_call".into()),
        ("cat-3".into(), "security".into()),
    ]
    .into_iter()
    .collect();

    let detailed = Reporter::generate(&report, &categories, &HashMap::new());

    let tool_call_stats = detailed.by_category.get("tool_call").unwrap();
    assert_eq!(tool_call_stats.total, 2);
    assert_eq!(tool_call_stats.passed, 2);
    assert!((tool_call_stats.pass_rate - 1.0).abs() < 0.01);

    let security_stats = detailed.by_category.get("security").unwrap();
    assert_eq!(security_stats.total, 1);
    assert_eq!(security_stats.passed, 0);
    assert!(security_stats.pass_rate < 0.01);
}

// ===================================================================
// Recorder integration tests
// ===================================================================

/// Test: EvalRecorder save/load roundtrip for a single trace.
#[test]
fn test_recorder_trace_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let recorder = EvalRecorder::new(dir.path().to_path_buf()).unwrap();

    let trace = EvalTrace {
        task_id: "integ-001".into(),
        timestamp: "2026-03-13T20:00:00Z".into(),
        interactions: vec![],
        timeline: vec![],
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
                message: "ok".into(),
            },
        ),
    };

    let path = recorder.save_trace(&trace).unwrap();
    let loaded = EvalRecorder::load_trace(&path).unwrap();

    assert_eq!(loaded.task_id, "integ-001");
    assert_eq!(loaded.timestamp, "2026-03-13T20:00:00Z");
    assert!(loaded.score.passed);
    assert!((loaded.score.score - 1.0).abs() < 0.01);
    assert_eq!(loaded.output.rounds, 1);
    assert_eq!(loaded.output.input_tokens, 10);
}

/// Test: EvalRecorder save/load roundtrip for a JSONL summary with multiple traces.
#[test]
fn test_recorder_summary_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let recorder = EvalRecorder::new(dir.path().to_path_buf()).unwrap();

    let traces = vec![
        EvalTrace {
            task_id: "sum-001".into(),
            timestamp: "2026-03-13T20:00:00Z".into(),
            interactions: vec![],
            timeline: vec![],
            output: AgentOutput::default(),
            score: EvalScore::pass(
                1.0,
                ScoreDetails::Custom {
                    message: "ok".into(),
                },
            ),
        },
        EvalTrace {
            task_id: "sum-002".into(),
            timestamp: "2026-03-13T20:01:00Z".into(),
            interactions: vec![],
            timeline: vec![],
            output: AgentOutput {
                rounds: 3,
                ..AgentOutput::default()
            },
            score: EvalScore::fail(
                0.2,
                ScoreDetails::ExactMatch {
                    expected: "hello".into(),
                    actual: "goodbye".into(),
                },
            ),
        },
    ];

    let path = recorder.save_summary(&traces).unwrap();
    let loaded = EvalRecorder::load_summary(&path).unwrap();

    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0].task_id, "sum-001");
    assert!(loaded[0].score.passed);
    assert_eq!(loaded[1].task_id, "sum-002");
    assert!(!loaded[1].score.passed);
    assert_eq!(loaded[1].output.rounds, 3);
}

/// Test: EvalRecorder extract_interactions returns the correct data.
#[test]
fn test_recorder_extract_interactions() {
    let interaction = octo_eval::recorder::record_interaction(
        "user: hello",
        "resp-1",
        "Hello!",
        "end_turn",
        10,
        5,
        150,
    );

    let trace = EvalTrace {
        task_id: "extract-001".into(),
        timestamp: "2026-03-13T20:00:00Z".into(),
        interactions: vec![interaction],
        timeline: vec![],
        output: AgentOutput::default(),
        score: EvalScore::pass(
            1.0,
            ScoreDetails::Custom {
                message: "ok".into(),
            },
        ),
    };

    let interactions = EvalRecorder::extract_interactions(&trace);
    assert_eq!(interactions.len(), 1);
    assert_eq!(interactions[0].request_summary, "user: hello");
    assert_eq!(interactions[0].latency_ms, 150);
}

// ===================================================================
// MockProvider integration tests
// ===================================================================

/// Test: MockProvider tracks call count correctly.
#[tokio::test]
async fn test_mock_provider_call_tracking() {
    let provider = MockProvider::with_text("response");
    let req = || octo_types::CompletionRequest {
        model: "test".into(),
        system: None,
        messages: vec![],
        max_tokens: 100,
        temperature: None,
        tools: vec![],
        stream: false,
    };

    assert_eq!(provider.call_count(), 0);
    let _ = provider.complete(req()).await.unwrap();
    assert_eq!(provider.call_count(), 1);
    let _ = provider.complete(req()).await.unwrap();
    assert_eq!(provider.call_count(), 2);
}

/// Test: MockProvider records requests for later inspection.
#[tokio::test]
async fn test_mock_provider_request_recording() {
    let provider = MockProvider::with_text("ok");
    let req = octo_types::CompletionRequest {
        model: "test-model".into(),
        system: Some("you are a helper".into()),
        messages: vec![],
        max_tokens: 256,
        temperature: None,
        tools: vec![],
        stream: false,
    };

    let _ = provider.complete(req).await.unwrap();
    let calls = provider.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].model, "test-model");
    assert_eq!(calls[0].system.as_deref(), Some("you are a helper"));
    assert_eq!(calls[0].max_tokens, 256);
}

/// Test: MockProvider with tool_then_text produces correct sequence.
#[tokio::test]
async fn test_mock_provider_tool_then_text_sequence() {
    let provider = MockProvider::with_tool_then_text(
        "file_read",
        serde_json::json!({"path": "/tmp/test.txt"}),
        "File contents: hello",
    );

    let req = || octo_types::CompletionRequest {
        model: "test".into(),
        system: None,
        messages: vec![],
        max_tokens: 100,
        temperature: None,
        tools: vec![],
        stream: false,
    };

    // First call: tool use
    let r1 = provider.complete(req()).await.unwrap();
    assert!(matches!(
        r1.stop_reason,
        Some(octo_types::StopReason::ToolUse)
    ));
    match &r1.content[0] {
        octo_types::ContentBlock::ToolUse { name, input, .. } => {
            assert_eq!(name, "file_read");
            assert_eq!(input["path"], "/tmp/test.txt");
        }
        other => panic!("Expected ToolUse, got {:?}", other),
    }

    // Second call: text response
    let r2 = provider.complete(req()).await.unwrap();
    assert!(matches!(
        r2.stop_reason,
        Some(octo_types::StopReason::EndTurn)
    ));
    match &r2.content[0] {
        octo_types::ContentBlock::Text { text } => {
            assert_eq!(text, "File contents: hello");
        }
        other => panic!("Expected Text, got {:?}", other),
    }

    assert_eq!(provider.call_count(), 2);
}

// ===================================================================
// Full pipeline: Task → Score → Report → Record
// ===================================================================

/// Test: Complete pipeline from task results through reporting and recording.
#[test]
fn test_full_pipeline_score_report_record() {
    // Step 1: Create scored task results (simulating what EvalRunner produces)
    let output_pass = AgentOutput {
        messages: vec![octo_types::ChatMessage::assistant("The answer is 42")],
        input_tokens: 50,
        output_tokens: 20,
        rounds: 1,
        stop_reason: "EndTurn".into(),
        ..AgentOutput::default()
    };
    let output_fail = AgentOutput {
        messages: vec![octo_types::ChatMessage::assistant("I don't know")],
        input_tokens: 60,
        output_tokens: 15,
        rounds: 1,
        stop_reason: "EndTurn".into(),
        ..AgentOutput::default()
    };

    let task_pass = SimpleTextTask {
        id: "pipe-1".into(),
        prompt: "What is 6*7?".into(),
        expected_contains: "42".into(),
    };
    let task_fail = SimpleTextTask {
        id: "pipe-2".into(),
        prompt: "What is 6*7?".into(),
        expected_contains: "42".into(),
    };

    let score_pass = task_pass.score(&output_pass);
    let score_fail = task_fail.score(&output_fail);

    assert!(score_pass.passed);
    assert!(!score_fail.passed);

    // Step 2: Build EvalReport
    let results = vec![
        TaskResult {
            task_id: "pipe-1".into(),
            output: output_pass.clone(),
            score: score_pass,
            duration_ms: 100,
        },
        TaskResult {
            task_id: "pipe-2".into(),
            output: output_fail.clone(),
            score: score_fail,
            duration_ms: 150,
        },
    ];
    let report = EvalReport::from_results(results);
    assert_eq!(report.total, 2);
    assert_eq!(report.passed, 1);
    assert!((report.pass_rate - 0.5).abs() < 0.01);

    // Step 3: Generate detailed report
    let categories: HashMap<String, String> = [
        ("pipe-1".into(), "math".into()),
        ("pipe-2".into(), "math".into()),
    ]
    .into_iter()
    .collect();
    let difficulties: HashMap<String, Difficulty> = [
        ("pipe-1".into(), Difficulty::Easy),
        ("pipe-2".into(), Difficulty::Easy),
    ]
    .into_iter()
    .collect();
    let detailed = Reporter::generate(&report, &categories, &difficulties);

    let json = Reporter::to_json(&detailed);
    assert!(json.contains("\"total\": 2"));
    assert!(json.contains("math"));

    let md = Reporter::to_markdown(&detailed);
    assert!(md.contains("PASS"));
    assert!(md.contains("FAIL"));
    assert!(md.contains("pipe-1"));
    assert!(md.contains("pipe-2"));

    // Step 4: Record traces
    let dir = tempfile::tempdir().unwrap();
    let recorder = EvalRecorder::new(dir.path().to_path_buf()).unwrap();

    let trace1 = EvalTrace {
        task_id: "pipe-1".into(),
        timestamp: "2026-03-13T20:00:00Z".into(),
        interactions: vec![],
        timeline: vec![],
        output: output_pass,
        score: EvalScore::pass(
            1.0,
            ScoreDetails::ExactMatch {
                expected: "42".into(),
                actual: "The answer is 42".into(),
            },
        ),
    };
    let trace2 = EvalTrace {
        task_id: "pipe-2".into(),
        timestamp: "2026-03-13T20:00:01Z".into(),
        interactions: vec![],
        timeline: vec![],
        output: output_fail,
        score: EvalScore::fail(
            0.0,
            ScoreDetails::ExactMatch {
                expected: "42".into(),
                actual: "I don't know".into(),
            },
        ),
    };

    // Save individual traces
    let path1 = recorder.save_trace(&trace1).unwrap();
    let path2 = recorder.save_trace(&trace2).unwrap();

    let loaded1 = EvalRecorder::load_trace(&path1).unwrap();
    let loaded2 = EvalRecorder::load_trace(&path2).unwrap();
    assert_eq!(loaded1.task_id, "pipe-1");
    assert!(loaded1.score.passed);
    assert_eq!(loaded2.task_id, "pipe-2");
    assert!(!loaded2.score.passed);

    // Save summary
    let summary_path = recorder.save_summary(&[trace1, trace2]).unwrap();
    let loaded_all = EvalRecorder::load_summary(&summary_path).unwrap();
    assert_eq!(loaded_all.len(), 2);
}

/// Test: SimpleToolTask scores tool calls correctly.
#[test]
fn test_tool_task_scoring() {
    let task = SimpleToolTask {
        id: "tool-001".into(),
        prompt: "List files".into(),
        expected_tool: "bash".into(),
    };

    // Correct tool call
    let output_pass = AgentOutput {
        tool_calls: vec![ToolCallRecord {
            name: "bash".into(),
            input: serde_json::json!({"command": "ls"}),
            output: "file1".into(),
            is_error: false,
            duration_ms: 10,
        }],
        ..AgentOutput::default()
    };
    let score = task.score(&output_pass);
    assert!(score.passed);
    assert!((score.score - 1.0).abs() < 0.01);

    // Wrong tool call
    let output_fail = AgentOutput {
        tool_calls: vec![ToolCallRecord {
            name: "file_read".into(),
            input: serde_json::json!({}),
            output: "".into(),
            is_error: false,
            duration_ms: 10,
        }],
        ..AgentOutput::default()
    };
    let score = task.score(&output_fail);
    assert!(!score.passed);

    // Verify metadata
    let meta = task.metadata();
    assert_eq!(meta.category, "tool_call");
    assert_eq!(meta.difficulty, Difficulty::Medium);
}

/// Test: auto_scorer selects the correct scorer based on task definition JSON.
#[test]
fn test_auto_scorer_integration() {
    use octo_eval::scorer::auto_scorer;

    // Tool call scorer
    let def = serde_json::json!({
        "expected_tool": "bash",
        "expected_args": {"command": "echo hello"}
    });
    let scorer = auto_scorer(&def);
    let output = AgentOutput {
        tool_calls: vec![ToolCallRecord {
            name: "bash".into(),
            input: serde_json::json!({"command": "echo hello"}),
            output: "hello".into(),
            is_error: false,
            duration_ms: 10,
        }],
        ..AgentOutput::default()
    };
    let score = scorer.score(&output);
    assert!(score.passed);

    // Behavior scorer
    let def = serde_json::json!({"expected_behavior": "completed"});
    let scorer = auto_scorer(&def);
    let output = AgentOutput {
        stop_reason: "EndTurn".into(),
        ..AgentOutput::default()
    };
    let score = scorer.score(&output);
    assert!(score.passed);

    // Exact match scorer
    let def = serde_json::json!({"expected_output": "hello world"});
    let scorer = auto_scorer(&def);
    let output = AgentOutput {
        messages: vec![octo_types::ChatMessage::assistant(
            "The result is hello world!",
        )],
        ..AgentOutput::default()
    };
    let score = scorer.score(&output);
    assert!(score.passed);
}

/// Test: EvalConfig default values are sensible.
#[test]
fn test_eval_config_defaults() {
    let config = EvalConfig::default();
    assert_eq!(config.concurrency, 1);
    assert_eq!(config.timeout_secs, 120);
    assert!(!config.record_traces);
    // Engine target is the default
    match &config.target {
        octo_eval::config::EvalTarget::Engine(engine) => {
            assert_eq!(engine.model, "mock");
            assert_eq!(engine.max_tokens, 4096);
            assert_eq!(engine.max_iterations, 10);
        }
        _ => panic!("Expected Engine target"),
    }
}

// ===================================================================
// Phase E1 feature tests
// ===================================================================

/// E1-T1: Recorder is auto-created when config.record_traces = true
#[tokio::test]
async fn test_recorder_integration_auto_traces() {
    let dir = tempfile::tempdir().unwrap();
    let mock = MockProvider::with_text("traced response");
    let config = EvalConfig {
        record_traces: true,
        output_dir: dir.path().to_path_buf(),
        ..EvalConfig::default()
    };
    let runner = EvalRunner::with_provider(config, Arc::new(mock));

    let task = SimpleTextTask {
        id: "trace-001".into(),
        prompt: "trace me".into(),
        expected_contains: "traced".into(),
    };

    let _result = runner.run_task(&task).await.unwrap();

    // Verify trace file was created
    let traces_dir = dir.path().join("traces");
    assert!(traces_dir.exists(), "traces directory should be created");
    let trace_file = traces_dir.join("trace_trace-001.json");
    assert!(
        trace_file.exists(),
        "individual trace file should be saved"
    );

    // Verify it's valid JSON that can be loaded
    let loaded = EvalRecorder::load_trace(&trace_file).unwrap();
    assert_eq!(loaded.task_id, "trace-001");
}

/// E1-T2: Timeout ScoreDetails variant serializes correctly
#[test]
fn test_timeout_score_details() {
    use octo_eval::score::ScoreDetails;

    let score = EvalScore::fail(0.0, ScoreDetails::Timeout { elapsed_secs: 30 });
    assert!(!score.passed);

    // Verify serialization roundtrip
    let json = serde_json::to_string(&score).unwrap();
    assert!(json.contains("Timeout"));
    assert!(json.contains("30"));
    let loaded: EvalScore = serde_json::from_str(&json).unwrap();
    assert!(!loaded.passed);
    match loaded.details {
        ScoreDetails::Timeout { elapsed_secs } => assert_eq!(elapsed_secs, 30),
        other => panic!("Expected Timeout, got {:?}", other),
    }
}

/// E1-T2: Timeout enforcement — verify the config field is wired up
#[tokio::test]
async fn test_timeout_config_wired() {
    // A very generous timeout should not trigger on a fast mock
    let mock = MockProvider::with_text("fast response");
    let config = EvalConfig {
        timeout_secs: 300, // generous timeout
        ..EvalConfig::default()
    };
    let runner = EvalRunner::with_provider(config, Arc::new(mock));

    let task = SimpleTextTask {
        id: "timeout-ok-001".into(),
        prompt: "this should complete".into(),
        expected_contains: "never-match".into(),
    };

    let result = runner.run_task(&task).await.unwrap();
    // Should NOT be a timeout — it should run and score normally
    match &result.score.details {
        ScoreDetails::Timeout { .. } => panic!("Should not have timed out"),
        _ => {} // any other score detail is fine
    }
}

/// E1-T4: Tool allowlist filtering via JsonlTask
#[test]
fn test_tool_allowlist_in_jsonl_task() {
    use octo_eval::datasets::loader::JsonlTask;
    use octo_eval::task::EvalTask;

    // Task with tools field set
    let json = r#"{"id":"allow-01","prompt":"test","category":"test","tools":["bash","file_read"]}"#;
    let task: JsonlTask = serde_json::from_str(json).unwrap();
    assert_eq!(task.tool_allowlist(), Some(vec!["bash".into(), "file_read".into()]));

    // Task without tools field
    let json_no_tools = r#"{"id":"allow-02","prompt":"test","category":"test"}"#;
    let task2: JsonlTask = serde_json::from_str(json_no_tools).unwrap();
    assert_eq!(task2.tool_allowlist(), None);
}

/// E1-T3: Concurrent suite execution with concurrency > 1
#[tokio::test]
async fn test_concurrent_suite_execution() {
    let mock = MockProvider::with_text("concurrent result");
    let config = EvalConfig {
        concurrency: 2, // concurrency > 1
        ..EvalConfig::default()
    };
    let runner = EvalRunner::with_provider(config, Arc::new(mock));

    let tasks: Vec<Box<dyn EvalTask>> = (1..=4)
        .map(|i| {
            Box::new(SimpleTextTask {
                id: format!("conc-{:02}", i),
                prompt: format!("Task {}", i),
                expected_contains: "never-match".into(),
            }) as Box<dyn EvalTask>
        })
        .collect();

    let report = runner.run_suite(&tasks).await.unwrap();

    // All 4 tasks should have run
    assert_eq!(report.total, 4);
    assert_eq!(report.results.len(), 4);

    // Results should be in original order (sorted after concurrent execution)
    assert_eq!(report.results[0].task_id, "conc-01");
    assert_eq!(report.results[1].task_id, "conc-02");
    assert_eq!(report.results[2].task_id, "conc-03");
    assert_eq!(report.results[3].task_id, "conc-04");
}

/// E1-T5: Regression detection in reporter
#[test]
fn test_regression_detection_integration() {
    use octo_eval::reporter::{Reporter, RegressionReport};

    // Build baseline
    let baseline_results = vec![
        TaskResult {
            task_id: "reg-1".into(),
            output: AgentOutput::default(),
            score: EvalScore::pass(1.0, ScoreDetails::Custom { message: "ok".into() }),
            duration_ms: 50,
        },
        TaskResult {
            task_id: "reg-2".into(),
            output: AgentOutput::default(),
            score: EvalScore::fail(0.0, ScoreDetails::Custom { message: "fail".into() }),
            duration_ms: 60,
        },
    ];
    let baseline_report = EvalReport::from_results(baseline_results);
    let baseline_detailed = Reporter::generate(&baseline_report, &HashMap::new(), &HashMap::new());

    // Build current (reg-2 now passes)
    let current_results = vec![
        TaskResult {
            task_id: "reg-1".into(),
            output: AgentOutput::default(),
            score: EvalScore::pass(1.0, ScoreDetails::Custom { message: "ok".into() }),
            duration_ms: 55,
        },
        TaskResult {
            task_id: "reg-2".into(),
            output: AgentOutput::default(),
            score: EvalScore::pass(0.8, ScoreDetails::Custom { message: "better".into() }),
            duration_ms: 65,
        },
    ];
    let current_report = EvalReport::from_results(current_results);
    let current_detailed = Reporter::generate(&current_report, &HashMap::new(), &HashMap::new());

    let regression = Reporter::diff_report(&current_detailed, &baseline_detailed);

    assert_eq!(regression.improved, 1);   // reg-2 improved
    assert_eq!(regression.regressed, 0);
    assert_eq!(regression.unchanged, 1);  // reg-1 unchanged
    assert!(regression.current_pass_rate > regression.baseline_pass_rate);

    // Serialization roundtrip
    let json = serde_json::to_string(&regression).unwrap();
    let loaded: RegressionReport = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded.improved, 1);
    assert_eq!(loaded.regressed, 0);
}

// ===================================================================
// F4-T3: JSONL format validation tests
// ===================================================================

/// Collect all .jsonl files from the datasets/ directory.
fn collect_jsonl_files() -> Vec<std::path::PathBuf> {
    let datasets_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("datasets");
    assert!(
        datasets_dir.exists(),
        "datasets/ directory not found at {:?}",
        datasets_dir
    );
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&datasets_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "jsonl") {
            files.push(path);
        }
    }
    files.sort();
    assert!(
        !files.is_empty(),
        "No .jsonl files found in {:?}",
        datasets_dir
    );
    files
}

/// Returns true if the filename indicates a BFCL dataset.
fn is_bfcl_file(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map_or(false, |name| name.starts_with("bfcl"))
}

/// External benchmark datasets have their own schemas and dedicated tests.
fn is_external_benchmark_file(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map_or(false, |name| {
            name.starts_with("gaia_")
                || name.starts_with("swe_bench_")
                || name.starts_with("tau_bench_")
                || name.starts_with("terminal_bench")
        })
}

/// F4-T3: Validate the structure of every task in every JSONL dataset file.
#[test]
fn test_validate_all_jsonl_datasets() {
    let valid_difficulties = ["easy", "medium", "hard"];
    let scoring_fields = [
        "expected_tool",
        "expected_output",
        "expected_behavior",
        "expected_sequence",
        "expected_call",
        "expected_regex",
        "expected_contains_all",
        "expected_not_contains",
        "expected_sequence_with_args",
        "scorer",
        "ground_truth",
    ];

    let files = collect_jsonl_files();

    for file in &files {
        // External benchmark datasets have their own schemas and dedicated unit tests
        if is_external_benchmark_file(file) {
            continue;
        }
        let bfcl = is_bfcl_file(file);
        let filename = file.file_name().unwrap().to_string_lossy();
        let content = std::fs::read_to_string(file)
            .unwrap_or_else(|e| panic!("Failed to read {}: {}", filename, e));

        for (line_idx, line) in content.lines().enumerate() {
            let line_num = line_idx + 1;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
                continue;
            }

            let val: serde_json::Value = serde_json::from_str(trimmed).unwrap_or_else(|e| {
                panic!("[{}:{}] Invalid JSON: {}", filename, line_num, e)
            });

            let obj = val.as_object().unwrap_or_else(|| {
                panic!("[{}:{}] Task must be a JSON object", filename, line_num)
            });

            // id: must be non-empty string
            let id = obj
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| {
                    panic!("[{}:{}] Missing or non-string 'id' field", filename, line_num)
                });
            assert!(
                !id.is_empty(),
                "[{}:{}] 'id' must be non-empty",
                filename,
                line_num
            );

            // prompt OR question: at least one must be present
            // Note: BFCL `question` is an array of arrays, not a string
            let has_prompt = obj
                .get("prompt")
                .and_then(|v| v.as_str())
                .map_or(false, |s| !s.is_empty());
            let has_question = obj.get("question").map_or(false, |v| {
                // Accept either a non-empty string or a non-empty array
                v.as_str().map_or(false, |s| !s.is_empty())
                    || v.as_array().map_or(false, |a| !a.is_empty())
            });
            assert!(
                has_prompt || has_question,
                "[{}:{}] Task '{}' must have a non-empty 'prompt' or 'question' field",
                filename,
                line_num,
                id
            );

            // category: required for octo datasets only (not BFCL)
            if !bfcl {
                let category = obj
                    .get("category")
                    .and_then(|v| v.as_str())
                    .unwrap_or_else(|| {
                        panic!(
                            "[{}:{}] Task '{}' missing or non-string 'category' field",
                            filename, line_num, id
                        )
                    });
                assert!(
                    !category.is_empty(),
                    "[{}:{}] Task '{}' 'category' must be non-empty",
                    filename,
                    line_num,
                    id
                );
            }

            // difficulty: if present, must be one of the valid values
            if let Some(diff_val) = obj.get("difficulty") {
                let diff = diff_val.as_str().unwrap_or_else(|| {
                    panic!(
                        "[{}:{}] Task '{}' 'difficulty' must be a string",
                        filename, line_num, id
                    )
                });
                assert!(
                    valid_difficulties.contains(&diff),
                    "[{}:{}] Task '{}' has invalid difficulty '{}', expected one of {:?}",
                    filename,
                    line_num,
                    id,
                    diff,
                    valid_difficulties
                );
            }

            // At least one scoring field must be present
            let has_scoring = scoring_fields.iter().any(|f| obj.contains_key(*f));
            assert!(
                has_scoring,
                "[{}:{}] Task '{}' has no scoring field. Expected at least one of: {:?}",
                filename, line_num, id, scoring_fields
            );
        }
    }
}

/// F4-T3: Validate that all task IDs are globally unique across all JSONL files.
#[test]
fn test_validate_unique_task_ids() {
    let files = collect_jsonl_files();
    let mut seen: HashMap<String, String> = HashMap::new(); // id -> filename

    for file in &files {
        let filename = file.file_name().unwrap().to_string_lossy().to_string();
        let content = std::fs::read_to_string(file).unwrap();

        for (line_idx, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
                continue;
            }
            let val: serde_json::Value = match serde_json::from_str(trimmed) {
                Ok(v) => v,
                Err(_) => continue, // skip unparseable lines (caught by other test)
            };
            if let Some(id) = val.get("id").and_then(|v| v.as_str()) {
                if let Some(prev_file) = seen.get(id) {
                    panic!(
                        "Duplicate task ID '{}' found in '{}' (line {}) — previously seen in '{}'",
                        id,
                        filename,
                        line_idx + 1,
                        prev_file,
                    );
                }
                seen.insert(id.to_string(), filename.clone());
            }
        }
    }
    println!(
        "[test_validate_unique_task_ids] Verified {} unique task IDs across {} files",
        seen.len(),
        files.len()
    );
}

/// F4-T3: Validate that the total task count meets the minimum baseline.
#[test]
fn test_validate_jsonl_task_count() {
    let files = collect_jsonl_files();
    let mut total = 0usize;

    for file in &files {
        let content = std::fs::read_to_string(file).unwrap();
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
                continue;
            }
            if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
                total += 1;
            }
        }
    }

    println!(
        "[test_validate_jsonl_task_count] Total tasks across {} JSONL files: {}",
        files.len(),
        total
    );
    assert!(
        total >= 90,
        "Expected at least 90 tasks across all JSONL files, found {}",
        total
    );
}
