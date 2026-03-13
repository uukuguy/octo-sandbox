//! Evaluation runner — drives agent loop execution for eval tasks.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use futures_util::stream::StreamExt;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use octo_engine::agent::{run_agent_loop, AgentEvent, AgentLoopConfig};
use octo_engine::providers::{create_provider, Provider};
use octo_types::ChatMessage;

use crate::config::{EvalConfig, EvalTarget};
use crate::model::ModelInfo;
use crate::recorder::{EvalRecorder, EvalTrace};
use crate::score::{EvalScore, ScoreDetails};
use crate::task::{AgentOutput, EvalTask, ToolCallRecord};

/// Result of running a single evaluation task
#[derive(Debug, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: String,
    pub output: AgentOutput,
    pub score: EvalScore,
    pub duration_ms: u64,
}

/// Aggregated evaluation report
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct EvalReport {
    pub model: Option<ModelInfo>,
    pub results: Vec<TaskResult>,
    pub total: usize,
    pub passed: usize,
    pub pass_rate: f64,
    pub avg_score: f64,
    pub total_tokens: u64,
    pub total_duration_ms: u64,
}

impl EvalReport {
    pub fn from_results(results: Vec<TaskResult>) -> Self {
        let total = results.len();
        let passed = results.iter().filter(|r| r.score.passed).count();
        let avg_score = if total > 0 {
            results.iter().map(|r| r.score.score).sum::<f64>() / total as f64
        } else {
            0.0
        };
        let total_tokens: u64 = results
            .iter()
            .map(|r| r.output.input_tokens + r.output.output_tokens)
            .sum();
        let total_duration_ms: u64 = results.iter().map(|r| r.duration_ms).sum();
        let pass_rate = if total > 0 {
            passed as f64 / total as f64
        } else {
            0.0
        };

        Self {
            model: None,
            results,
            total,
            passed,
            pass_rate,
            avg_score,
            total_tokens,
            total_duration_ms,
        }
    }

    /// Attach model info to this report.
    pub fn with_model(mut self, model: ModelInfo) -> Self {
        self.model = Some(model);
        self
    }

    /// Estimated cost in USD based on model pricing and token usage.
    pub fn estimated_cost(&self) -> f64 {
        match &self.model {
            Some(info) => {
                let total_input: u64 = self.results.iter().map(|r| r.output.input_tokens).sum();
                let total_output: u64 = self.results.iter().map(|r| r.output.output_tokens).sum();
                info.estimate_cost(total_input, total_output)
            }
            None => 0.0,
        }
    }
}

/// Evaluation runner — drives agent loop execution for eval tasks
pub struct EvalRunner {
    config: EvalConfig,
    provider: Arc<dyn Provider>,
    recorder: Option<EvalRecorder>,
}

impl EvalRunner {
    pub fn new(config: EvalConfig) -> Result<Self> {
        let provider = Self::create_provider_from_config(&config)?;
        let recorder = if config.record_traces {
            Some(EvalRecorder::new(config.output_dir.join("traces"))?)
        } else {
            None
        };
        Ok(Self {
            config,
            provider,
            recorder,
        })
    }

    /// Create with an explicit provider (useful for MockProvider in tests)
    pub fn with_provider(config: EvalConfig, provider: Arc<dyn Provider>) -> Self {
        let recorder = if config.record_traces {
            EvalRecorder::new(config.output_dir.join("traces")).ok()
        } else {
            None
        };
        Self {
            config,
            provider,
            recorder,
        }
    }

    /// Returns a reference to the runner configuration.
    pub fn config(&self) -> &EvalConfig {
        &self.config
    }

    fn create_provider_from_config(config: &EvalConfig) -> Result<Arc<dyn Provider>> {
        match &config.target {
            EvalTarget::Engine(engine_config) => {
                let api_key = engine_config.api_key.clone().unwrap_or_default();
                let provider = create_provider(
                    &engine_config.provider_name,
                    api_key,
                    engine_config.base_url.clone(),
                );
                Ok(Arc::from(provider))
            }
        }
    }

    /// Run a single evaluation task with timeout enforcement
    pub async fn run_task(&self, task: &dyn EvalTask) -> Result<TaskResult> {
        let start = Instant::now();
        let task_id = task.id().to_string();
        let timeout_secs = self.config.timeout_secs;

        info!(task_id = %task_id, "Starting evaluation task");

        let engine_config = match &self.config.target {
            EvalTarget::Engine(c) => c,
        };

        // Build tool registry — apply per-task allowlist if specified
        let base_registry = octo_engine::tools::default_tools();
        let tool_registry = if let Some(ref tool_names) = task.tool_allowlist() {
            Arc::new(base_registry.snapshot_filtered(tool_names))
        } else {
            Arc::new(base_registry)
        };

        // Build AgentLoopConfig
        let loop_config = AgentLoopConfig::builder()
            .provider(self.provider.clone())
            .model(engine_config.model.clone())
            .max_tokens(engine_config.max_tokens)
            .max_iterations(engine_config.max_iterations)
            .tools(tool_registry)
            .build();

        // Create the initial user message from the task prompt
        let messages = vec![ChatMessage::user(task.prompt())];

        // Run agent loop with timeout wrapping collect_events
        let timeout_duration = Duration::from_secs(timeout_secs);
        let output = match tokio::time::timeout(
            timeout_duration,
            Self::collect_events(loop_config, messages),
        )
        .await
        {
            Ok(output) => output,
            Err(_elapsed) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                warn!(task_id = %task_id, timeout_secs, "Task timed out");
                return Ok(TaskResult {
                    task_id,
                    output: AgentOutput::default(),
                    score: EvalScore::fail(
                        0.0,
                        ScoreDetails::Timeout {
                            elapsed_secs: timeout_secs,
                        },
                    ),
                    duration_ms,
                });
            }
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        // Score the output using the task's scoring function,
        // then re-score with LlmJudge if the task requests it.
        let score = if let Some(judge_config) = task.llm_judge_config() {
            let judge = crate::scorer::LlmJudgeScorer::new(
                judge_config.rubric,
                judge_config.pass_threshold,
            );
            let engine_config = match &self.config.target {
                EvalTarget::Engine(c) => c,
            };
            judge
                .score_async(
                    self.provider.as_ref(),
                    &engine_config.model,
                    task.prompt(),
                    &output,
                )
                .await
        } else {
            task.score(&output)
        };

        info!(
            task_id = %task_id,
            passed = score.passed,
            score = score.score,
            duration_ms = duration_ms,
            "Task evaluation complete"
        );

        let result = TaskResult {
            task_id,
            output,
            score,
            duration_ms,
        };

        // Record trace if recorder is enabled
        if let Some(ref recorder) = self.recorder {
            let trace = EvalTrace {
                task_id: result.task_id.clone(),
                timestamp: chrono::Utc::now().to_rfc3339(),
                interactions: vec![], // populated only when using MockProvider
                output: result.output.clone(),
                score: result.score.clone(),
            };
            if let Err(e) = recorder.save_trace(&trace) {
                warn!(error = %e, "Failed to save evaluation trace");
            }
        }

        Ok(result)
    }

    /// Run all tasks (sequentially or concurrently) and generate an aggregated report
    pub async fn run_suite(&self, tasks: &[Box<dyn EvalTask>]) -> Result<EvalReport> {
        let total = tasks.len();
        let concurrency = self.config.concurrency.max(1);

        let results = if concurrency <= 1 {
            // Sequential mode (default, preserves ordering)
            self.run_suite_sequential(tasks).await?
        } else {
            // Concurrent mode
            self.run_suite_concurrent(tasks, concurrency).await?
        };

        eprintln!(
            "Suite complete: {}/{} passed",
            results.iter().filter(|r| r.score.passed).count(),
            total
        );

        // Save summary trace if recorder is enabled
        if let Some(ref recorder) = self.recorder {
            let traces: Vec<EvalTrace> = results
                .iter()
                .map(|r| EvalTrace {
                    task_id: r.task_id.clone(),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    interactions: vec![],
                    output: r.output.clone(),
                    score: r.score.clone(),
                })
                .collect();
            if let Err(e) = recorder.save_summary(&traces) {
                warn!(error = %e, "Failed to save evaluation summary");
            }
        }

        Ok(EvalReport::from_results(results))
    }

    /// Sequential task execution (concurrency = 1)
    async fn run_suite_sequential(
        &self,
        tasks: &[Box<dyn EvalTask>],
    ) -> Result<Vec<TaskResult>> {
        let total = tasks.len();
        let mut results = Vec::with_capacity(total);

        for (i, task) in tasks.iter().enumerate() {
            let idx = i + 1;
            eprintln!("[{}/{}] Running task: {} ...", idx, total, task.id());

            match self.run_task(task.as_ref()).await {
                Ok(result) => {
                    let status = if result.score.passed { "PASS" } else { "FAIL" };
                    eprintln!(
                        "[{}/{}] {} {} (score={:.2}, {}ms)",
                        idx, total, status, result.task_id, result.score.score, result.duration_ms
                    );
                    results.push(result);
                }
                Err(e) => {
                    eprintln!("[{}/{}] ERROR {}: {}", idx, total, task.id(), e);
                    results.push(TaskResult {
                        task_id: task.id().to_string(),
                        output: AgentOutput::default(),
                        score: EvalScore::fail(
                            0.0,
                            ScoreDetails::Custom {
                                message: format!("Execution error: {}", e),
                            },
                        ),
                        duration_ms: 0,
                    });
                }
            }
        }

        Ok(results)
    }

    /// Concurrent task execution (concurrency > 1)
    async fn run_suite_concurrent(
        &self,
        tasks: &[Box<dyn EvalTask>],
        concurrency: usize,
    ) -> Result<Vec<TaskResult>> {
        let total = tasks.len();
        let provider = self.provider.clone();
        let config = self.config.clone();

        // Create futures for each task — each clones the Arc<Provider> and config
        let futures = tasks.iter().enumerate().map(|(i, task)| {
            let provider = provider.clone();
            let config = config.clone();
            let task_id = task.id().to_string();
            let idx = i + 1;

            async move {
                eprintln!("[{}/{}] Running task: {} ...", idx, total, task_id);

                let runner = EvalRunner {
                    config,
                    provider,
                    recorder: None, // traces saved at suite level
                };

                match runner.run_task(task.as_ref()).await {
                    Ok(result) => {
                        let status = if result.score.passed { "PASS" } else { "FAIL" };
                        eprintln!(
                            "[{}/{}] {} {} (score={:.2}, {}ms)",
                            idx, total, status, result.task_id, result.score.score,
                            result.duration_ms
                        );
                        (i, result)
                    }
                    Err(e) => {
                        eprintln!("[{}/{}] ERROR {}: {}", idx, total, task_id, e);
                        (
                            i,
                            TaskResult {
                                task_id: task_id.clone(),
                                output: AgentOutput::default(),
                                score: EvalScore::fail(
                                    0.0,
                                    ScoreDetails::Custom {
                                        message: format!("Execution error: {}", e),
                                    },
                                ),
                                duration_ms: 0,
                            },
                        )
                    }
                }
            }
        });

        // Run with bounded concurrency and collect results
        let mut indexed_results: Vec<(usize, TaskResult)> =
            futures_util::stream::iter(futures)
                .buffer_unordered(concurrency)
                .collect()
                .await;

        // Sort by original index to maintain stable ordering
        indexed_results.sort_by_key(|(i, _)| *i);
        Ok(indexed_results.into_iter().map(|(_, r)| r).collect())
    }

    /// Collect AgentEvents from the stream into an AgentOutput
    async fn collect_events(
        config: AgentLoopConfig,
        messages: Vec<ChatMessage>,
    ) -> AgentOutput {
        let stream = run_agent_loop(config, messages);
        futures_util::pin_mut!(stream);

        let mut output = AgentOutput::default();
        let mut pending_tools: std::collections::HashMap<
            String,
            (String, serde_json::Value, Instant),
        > = std::collections::HashMap::new();

        while let Some(event) = stream.next().await {
            match event {
                AgentEvent::ToolStart {
                    tool_id,
                    tool_name,
                    input,
                } => {
                    pending_tools.insert(tool_id, (tool_name, input, Instant::now()));
                }
                AgentEvent::ToolResult {
                    tool_id,
                    output: tool_output,
                    success,
                } => {
                    if let Some((name, input, tool_start)) = pending_tools.remove(&tool_id) {
                        output.tool_calls.push(ToolCallRecord {
                            name,
                            input,
                            output: tool_output,
                            is_error: !success,
                            duration_ms: tool_start.elapsed().as_millis() as u64,
                        });
                    }
                }
                AgentEvent::Completed(result) => {
                    output.rounds = result.rounds;
                    output.input_tokens = result.input_tokens;
                    output.output_tokens = result.output_tokens;
                    output.stop_reason = format!("{:?}", result.stop_reason);
                    output.messages = result.final_messages;
                }
                AgentEvent::Error { message } => {
                    warn!(error = %message, "Agent loop error during evaluation");
                }
                _ => {}
            }
        }

        output
    }
}
