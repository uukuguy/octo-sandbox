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

use crate::config::{CliConfig, EvalConfig, EvalTarget, ServerConfig};
use crate::model::ModelInfo;
use crate::recorder::{EvalRecorder, EvalTrace};
use crate::score::{EvalScore, ScoreDetails};
use crate::task::{AgentOutput, EvalTask, ToolCallRecord};

/// JSON output format produced by `octo ask --output json`
#[derive(Debug, Deserialize)]
struct CliJsonOutput {
    text: String,
    tool_calls: Vec<CliToolCall>,
    rounds: u32,
    input_tokens: u64,
    output_tokens: u64,
    duration_ms: u64,
    stop_reason: String,
}

#[derive(Debug, Deserialize)]
struct CliToolCall {
    name: String,
    args: serde_json::Value,
    result: String,
    success: bool,
}

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
            EvalTarget::Cli(_) | EvalTarget::Server(_) => {
                // CLI/Server modes don't use a provider directly — use a dummy mock
                Ok(Arc::from(create_provider("openai", String::new(), None)))
            }
        }
    }

    /// Run a single evaluation task with timeout enforcement
    pub async fn run_task(&self, task: &dyn EvalTask) -> Result<TaskResult> {
        // Dispatch based on target mode
        match &self.config.target {
            EvalTarget::Cli(cli_config) => return self.run_task_cli(task, cli_config).await,
            EvalTarget::Server(server_config) => {
                return self.run_task_server(task, server_config).await
            }
            EvalTarget::Engine(_) => {} // fall through to engine path below
        }

        let start = Instant::now();
        let task_id = task.id().to_string();
        let timeout_secs = self.config.timeout_secs;

        info!(task_id = %task_id, "Starting evaluation task");

        let engine_config = match &self.config.target {
            EvalTarget::Engine(c) => c,
            _ => unreachable!(),
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
                _ => unreachable!(), // CLI dispatches early
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

    /// Run a single evaluation task via CLI subprocess (`octo ask --output json`)
    async fn run_task_cli(
        &self,
        task: &dyn EvalTask,
        cli_config: &CliConfig,
    ) -> Result<TaskResult> {
        let start = Instant::now();
        let task_id = task.id().to_string();
        let timeout_secs = cli_config.timeout_secs;

        info!(task_id = %task_id, "Starting CLI evaluation task");

        let mut cmd = tokio::process::Command::new(&cli_config.binary_path);
        cmd.arg("ask").arg("--output").arg("json");

        // Append extra CLI arguments
        for arg in &cli_config.extra_args {
            cmd.arg(arg);
        }

        // The prompt is the final positional argument
        cmd.arg(&task.prompt());

        // Inject environment variables
        for (k, v) in &cli_config.env {
            cmd.env(k, v);
        }

        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        // Spawn with timeout
        let timeout_duration = Duration::from_secs(timeout_secs);
        let child_result = match tokio::time::timeout(timeout_duration, async {
            let child = cmd.spawn()?;
            child.wait_with_output().await
        })
        .await
        {
            Ok(result) => result?,
            Err(_elapsed) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                warn!(task_id = %task_id, timeout_secs, "CLI task timed out");
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

        if !child_result.status.success() {
            let stderr = String::from_utf8_lossy(&child_result.stderr);
            warn!(task_id = %task_id, stderr = %stderr, "CLI subprocess failed");
            return Ok(TaskResult {
                task_id,
                output: AgentOutput::default(),
                score: EvalScore::fail(
                    0.0,
                    ScoreDetails::Custom {
                        message: format!("CLI exited with {}: {}", child_result.status, stderr),
                    },
                ),
                duration_ms,
            });
        }

        // Parse JSON output from stdout
        let stdout = String::from_utf8_lossy(&child_result.stdout);
        let cli_output: CliJsonOutput = serde_json::from_str(&stdout).map_err(|e| {
            anyhow::anyhow!("Failed to parse CLI JSON output: {} — raw: {}", e, stdout)
        })?;

        // Convert to AgentOutput
        let output = AgentOutput {
            rounds: cli_output.rounds,
            input_tokens: cli_output.input_tokens,
            output_tokens: cli_output.output_tokens,
            stop_reason: cli_output.stop_reason,
            tool_calls: cli_output
                .tool_calls
                .into_iter()
                .map(|tc| ToolCallRecord {
                    name: tc.name,
                    input: tc.args,
                    output: tc.result,
                    is_error: !tc.success,
                    duration_ms: 0,
                })
                .collect(),
            messages: vec![octo_types::ChatMessage::assistant(&cli_output.text)],
            duration_ms: cli_output.duration_ms,
        };

        let score = task.score(&output);

        info!(
            task_id = %task_id,
            passed = score.passed,
            score = score.score,
            duration_ms = duration_ms,
            "CLI task evaluation complete"
        );

        Ok(TaskResult {
            task_id,
            output,
            score,
            duration_ms,
        })
    }

    /// Run a single evaluation task via HTTP against a running octo-server
    async fn run_task_server(
        &self,
        task: &dyn EvalTask,
        server_config: &ServerConfig,
    ) -> Result<TaskResult> {
        let start = Instant::now();
        let task_id = task.id().to_string();
        let timeout_secs = server_config.timeout_secs;

        info!(task_id = %task_id, "Starting Server HTTP evaluation task");

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()?;

        let base_url = server_config.base_url.trim_end_matches('/');

        // Build common headers
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(ref key) = server_config.api_key {
            headers.insert(
                "Authorization",
                reqwest::header::HeaderValue::from_str(&format!("Bearer {}", key))
                    .unwrap_or_else(|_| reqwest::header::HeaderValue::from_static("")),
            );
        }

        // Step 1: Create session
        let create_resp = client
            .post(format!("{}/api/eval/sessions", base_url))
            .headers(headers.clone())
            .json(&serde_json::json!({}))
            .send()
            .await;

        let session_id = match create_resp {
            Ok(resp) if resp.status().is_success() => {
                let body: serde_json::Value = resp.json().await?;
                body["session_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string()
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Ok(TaskResult {
                    task_id,
                    output: AgentOutput::default(),
                    score: EvalScore::fail(
                        0.0,
                        ScoreDetails::Custom {
                            message: format!("Server session create failed ({}): {}", status, body),
                        },
                    ),
                    duration_ms: start.elapsed().as_millis() as u64,
                });
            }
            Err(e) => {
                return Ok(TaskResult {
                    task_id,
                    output: AgentOutput::default(),
                    score: EvalScore::fail(
                        0.0,
                        ScoreDetails::Custom {
                            message: format!("Server connection failed: {}", e),
                        },
                    ),
                    duration_ms: start.elapsed().as_millis() as u64,
                });
            }
        };

        // Step 2: Send message and wait for complete response
        let msg_resp = client
            .post(format!(
                "{}/api/eval/sessions/{}/messages",
                base_url, session_id
            ))
            .headers(headers.clone())
            .json(&serde_json::json!({ "content": task.prompt() }))
            .send()
            .await;

        let output = match msg_resp {
            Ok(resp) if resp.status().is_success() => {
                let body: serde_json::Value = resp.json().await?;
                AgentOutput {
                    rounds: body["rounds"].as_u64().unwrap_or(1) as u32,
                    input_tokens: body["input_tokens"].as_u64().unwrap_or(0),
                    output_tokens: body["output_tokens"].as_u64().unwrap_or(0),
                    stop_reason: body["stop_reason"]
                        .as_str()
                        .unwrap_or("unknown")
                        .to_string(),
                    tool_calls: body["tool_calls"]
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .map(|tc| ToolCallRecord {
                                    name: tc["name"]
                                        .as_str()
                                        .unwrap_or_default()
                                        .to_string(),
                                    input: tc["args"].clone(),
                                    output: tc["result"]
                                        .as_str()
                                        .unwrap_or_default()
                                        .to_string(),
                                    is_error: !tc["success"].as_bool().unwrap_or(true),
                                    duration_ms: 0,
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    messages: vec![octo_types::ChatMessage::assistant(
                        body["text"].as_str().unwrap_or_default(),
                    )],
                    duration_ms: body["duration_ms"].as_u64().unwrap_or(0),
                }
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                AgentOutput {
                    messages: vec![octo_types::ChatMessage::assistant(&format!(
                        "Server error ({}): {}",
                        status, body
                    ))],
                    ..AgentOutput::default()
                }
            }
            Err(e) if e.is_timeout() => {
                let duration_ms = start.elapsed().as_millis() as u64;
                warn!(task_id = %task_id, timeout_secs, "Server task timed out");
                // Cleanup session on timeout
                let _ = client
                    .delete(format!("{}/api/eval/sessions/{}", base_url, session_id))
                    .headers(headers)
                    .send()
                    .await;
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
            Err(e) => {
                return Ok(TaskResult {
                    task_id,
                    output: AgentOutput::default(),
                    score: EvalScore::fail(
                        0.0,
                        ScoreDetails::Custom {
                            message: format!("Server request failed: {}", e),
                        },
                    ),
                    duration_ms: start.elapsed().as_millis() as u64,
                });
            }
        };

        // Step 3: Cleanup session
        let _ = client
            .delete(format!("{}/api/eval/sessions/{}", base_url, session_id))
            .headers(headers)
            .send()
            .await;

        let duration_ms = start.elapsed().as_millis() as u64;

        // Score the output
        let score = if let Some(judge_config) = task.llm_judge_config() {
            let judge = crate::scorer::LlmJudgeScorer::new(
                judge_config.rubric,
                judge_config.pass_threshold,
            );
            let engine_config = crate::config::EngineConfig::default();
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
            "Server task evaluation complete"
        );

        Ok(TaskResult {
            task_id,
            output,
            score,
            duration_ms,
        })
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
