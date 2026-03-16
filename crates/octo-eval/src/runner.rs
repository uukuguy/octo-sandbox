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
use crate::mock_tool::EvalMockTool;
use crate::model::ModelInfo;
use crate::recorder::{EvalRecorder, EvalTrace};
use crate::score::{EvalScore, ScoreDetails};
use crate::task::{AgentOutput, EvalTask, ToolCallRecord};
use crate::trace::{truncate_str, TraceEvent};

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
    /// Pre-computed cost (used when loading from JSON where results are unavailable)
    pub cached_estimated_cost: Option<f64>,
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
            cached_estimated_cost: None,
        }
    }

    /// Attach model info to this report.
    pub fn with_model(mut self, model: ModelInfo) -> Self {
        self.model = Some(model);
        self
    }

    /// Estimated cost in USD based on model pricing and token usage.
    pub fn estimated_cost(&self) -> f64 {
        if let Some(cached) = self.cached_estimated_cost {
            return cached;
        }
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
            EvalTarget::Engine(_) => {
                // SWE-bench tasks use engine fallback (LLM agent loop with tools)
                // rather than Docker direct execution, which only runs prompt as bash.
                if Self::is_swe_bench_task(task) {
                    return self.run_task_engine_fallback(task).await;
                }
            }
        }

        let start = Instant::now();
        let task_id = task.id().to_string();
        let timeout_secs = self.config.timeout_secs;

        info!(task_id = %task_id, "Starting evaluation task");

        let engine_config = match &self.config.target {
            EvalTarget::Engine(c) => c,
            _ => unreachable!(),
        };

        // Build tool registry:
        // 1. Start with default tools (bash, file_read, web_search, etc.)
        // 2. Inject task-declared tools from available_tools() as EvalMockTools
        // 3. If tool_allowlist() is set, filter to only those names
        let mut base_registry = octo_engine::tools::default_tools();

        // Inject task-declared tools (e.g. τ-bench business tools) as mock implementations
        if let Some(task_tool_specs) = task.available_tools() {
            for spec in task_tool_specs {
                // Only inject if not already present in default tools
                if base_registry.get(&spec.name).is_none() {
                    base_registry.register(EvalMockTool::new(spec));
                }
            }
        }

        let tool_registry = if let Some(ref tool_names) = task.tool_allowlist() {
            Arc::new(base_registry.snapshot_filtered(tool_names))
        } else {
            Arc::new(base_registry)
        };

        // Select provider — wrap with FaultyProvider for fault-injection resilience tasks
        let effective_provider: Arc<dyn octo_engine::providers::Provider> =
            if let Some((fail_turn, error_type)) = task.fault_config() {
                Arc::new(crate::faulty_provider::FaultyProvider::from_config(
                    self.provider.clone(),
                    fail_turn,
                    &error_type,
                ))
            } else {
                self.provider.clone()
            };

        // Create an isolated per-task working directory under /tmp so that Agent
        // file operations don't pollute the workspace.
        let task_workdir = std::env::temp_dir()
            .join("octo-eval")
            .join(&task_id);
        let _ = std::fs::create_dir_all(&task_workdir);
        let tool_ctx = octo_types::ToolContext {
            sandbox_id: octo_types::SandboxId::default(),
            working_dir: task_workdir.clone(),
            path_validator: None,
        };

        // Build AgentLoopConfig
        let loop_config = AgentLoopConfig::builder()
            .provider(effective_provider)
            .model(engine_config.model.clone())
            .max_tokens(engine_config.max_tokens)
            .max_iterations(engine_config.max_iterations)
            .tools(tool_registry)
            .tool_ctx(tool_ctx)
            .build();

        // Create the initial user message from the task prompt
        let messages = vec![ChatMessage::user(task.prompt())];

        // Run agent loop with timeout wrapping collect_events
        let timeout_duration = Duration::from_secs(timeout_secs);
        let (output, timeline) = match tokio::time::timeout(
            timeout_duration,
            Self::collect_events(loop_config, messages),
        )
        .await
        {
            Ok(result) => result,
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
                timeline,
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

    /// Check if a task is a SWE-bench task that should use Docker mode.
    fn is_swe_bench_task(task: &dyn EvalTask) -> bool {
        task.metadata().tags.contains(&"swe_bench".to_string())
    }

    /// Run a SWE-bench task in Docker mode.
    ///
    /// Flow:
    /// 1. Check Docker daemon availability
    /// 2. Create container from swebench image with repo cloned at /testbed
    /// 3. Agent executes tools inside the container
    /// 4. Extract git diff patch from container
    /// 5. Score using official swebench harness (or patch-presence heuristic)
    /// 6. Cleanup container
    ///
    /// Falls back to engine mode if Docker is unavailable.
    async fn run_task_docker(
        &self,
        task: &dyn EvalTask,
    ) -> Result<TaskResult> {
        use octo_engine::sandbox::{DockerAdapter, RuntimeAdapter};

        let start = Instant::now();
        let task_id = task.id().to_string();

        // Check Docker availability
        let adapter = DockerAdapter::new("octo-sandbox/swebench:1.0");
        if !adapter.is_available() {
            warn!(task_id = %task_id, "Docker not available, falling back to engine mode for SWE-bench task");
            // Fall through to engine mode — the task will still produce a patch
            // via normal tool execution, just not in an isolated container.
            return self.run_task_engine_fallback(task).await;
        }

        info!(task_id = %task_id, "Starting SWE-bench Docker evaluation task");

        // Create sandbox container
        let mut sandbox_config = octo_engine::sandbox::SandboxConfig::new(
            octo_engine::sandbox::SandboxType::Docker,
        );
        sandbox_config.env.insert(
            "TASK_ID".to_string(),
            task_id.clone(),
        );

        let sandbox_id = match adapter.create(&sandbox_config).await {
            Ok(id) => id,
            Err(e) => {
                warn!(task_id = %task_id, error = %e, "Docker container creation failed, falling back");
                return self.run_task_engine_fallback(task).await;
            }
        };

        // Run agent prompt inside the container
        let result = adapter
            .execute(&sandbox_id, task.prompt(), "bash")
            .await;

        let output = match result {
            Ok(exec_result) => {
                // After agent execution, extract the git diff
                let diff_result = adapter
                    .execute(&sandbox_id, "cd /testbed && git diff", "bash")
                    .await;

                let patch = diff_result
                    .map(|r| r.stdout)
                    .unwrap_or_default();

                let mut messages_text = exec_result.stdout.clone();
                if !patch.is_empty() {
                    messages_text.push_str("\n\n```diff\n");
                    messages_text.push_str(&patch);
                    messages_text.push_str("\n```\n");
                }

                AgentOutput {
                    rounds: 1,
                    input_tokens: 0,
                    output_tokens: 0,
                    stop_reason: if exec_result.success {
                        "end_turn".to_string()
                    } else {
                        "error".to_string()
                    },
                    tool_calls: vec![],
                    messages: vec![octo_types::ChatMessage::assistant(&messages_text)],
                    duration_ms: start.elapsed().as_millis() as u64,
                }
            }
            Err(e) => {
                warn!(task_id = %task_id, error = %e, "Docker execution failed");
                AgentOutput {
                    messages: vec![octo_types::ChatMessage::assistant(&format!(
                        "Docker execution error: {}",
                        e
                    ))],
                    ..AgentOutput::default()
                }
            }
        };

        // Cleanup container
        let _ = adapter.destroy(&sandbox_id).await;

        let duration_ms = start.elapsed().as_millis() as u64;
        let score = task.score(&output);

        info!(
            task_id = %task_id,
            passed = score.passed,
            score = score.score,
            duration_ms,
            "SWE-bench Docker task evaluation complete"
        );

        Ok(TaskResult {
            task_id,
            output,
            score,
            duration_ms,
        })
    }

    /// Engine-mode fallback for SWE-bench tasks when Docker is unavailable.
    /// Reuses the standard engine execution path via collect_events.
    async fn run_task_engine_fallback(
        &self,
        task: &dyn EvalTask,
    ) -> Result<TaskResult> {
        let start = Instant::now();
        let task_id = task.id().to_string();

        let engine_config = match &self.config.target {
            EvalTarget::Engine(c) => c,
            _ => {
                return Ok(TaskResult {
                    task_id,
                    output: AgentOutput::default(),
                    score: EvalScore::fail(
                        0.0,
                        ScoreDetails::Custom {
                            message: "SWE-bench Docker fallback requires Engine target".into(),
                        },
                    ),
                    duration_ms: start.elapsed().as_millis() as u64,
                });
            }
        };

        let mut base_registry = octo_engine::tools::default_tools();
        if let Some(task_tool_specs) = task.available_tools() {
            for spec in task_tool_specs {
                if base_registry.get(&spec.name).is_none() {
                    base_registry.register(EvalMockTool::new(spec));
                }
            }
        }
        let tool_registry = if let Some(ref tool_names) = task.tool_allowlist() {
            Arc::new(base_registry.snapshot_filtered(tool_names))
        } else {
            Arc::new(base_registry)
        };

        let task_workdir = std::env::temp_dir()
            .join("octo-eval")
            .join(&task_id);
        let _ = std::fs::create_dir_all(&task_workdir);

        // For SWE-bench tasks: clone the target repo and checkout base_commit
        if Self::is_swe_bench_task(task) {
            let scoring = task.scoring_data();
            let repo = scoring.get("repo").and_then(|v| v.as_str()).unwrap_or("");
            let base_commit = scoring.get("base_commit").and_then(|v| v.as_str()).unwrap_or("");
            if !repo.is_empty() && !task_workdir.join(".git").exists() {
                let repo_url = format!("https://github.com/{}.git", repo);
                info!(task_id = %task_id, repo = %repo, "Cloning SWE-bench repo");
                let clone_result = std::process::Command::new("git")
                    .args(["clone", "--depth", "50", &repo_url, "."])
                    .current_dir(&task_workdir)
                    .output();
                if let Ok(output) = clone_result {
                    if output.status.success() && !base_commit.is_empty() {
                        let _ = std::process::Command::new("git")
                            .args(["checkout", base_commit])
                            .current_dir(&task_workdir)
                            .output();
                    }
                }
            }
        }

        let tool_ctx = octo_types::ToolContext {
            sandbox_id: octo_types::SandboxId::default(),
            working_dir: task_workdir,
            path_validator: None,
        };

        let loop_config = AgentLoopConfig::builder()
            .provider(self.provider.clone())
            .model(engine_config.model.clone())
            .max_tokens(engine_config.max_tokens)
            .max_iterations(engine_config.max_iterations)
            .tools(tool_registry)
            .tool_ctx(tool_ctx)
            .build();

        let messages = vec![ChatMessage::user(task.prompt())];
        let timeout = Duration::from_secs(self.config.timeout_secs);

        let (output, _timeline) = match tokio::time::timeout(
            timeout,
            Self::collect_events(loop_config, messages),
        )
        .await
        {
            Ok(result) => result,
            Err(_elapsed) => {
                warn!(task_id = %task_id, "SWE-bench engine fallback timed out");
                return Ok(TaskResult {
                    task_id,
                    output: AgentOutput::default(),
                    score: EvalScore::fail(
                        0.0,
                        ScoreDetails::Timeout {
                            elapsed_secs: self.config.timeout_secs,
                        },
                    ),
                    duration_ms: start.elapsed().as_millis() as u64,
                });
            }
        };

        // For SWE-bench tasks: capture git diff from the working directory
        // Agent modifies files via bash/file_write tools, so we need to extract the patch
        let mut output = output;
        if Self::is_swe_bench_task(task) {
            let workdir = std::env::temp_dir()
                .join("octo-eval")
                .join(&task_id);
            if let Ok(diff_output) = std::process::Command::new("git")
                .args(["diff"])
                .current_dir(&workdir)
                .output()
            {
                let patch = String::from_utf8_lossy(&diff_output.stdout);
                if !patch.trim().is_empty() {
                    let patch_msg = format!("\n\n```diff\n{}\n```\n", patch.trim());
                    output.messages.push(ChatMessage::assistant(&patch_msg));
                }
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        let score = task.score(&output);

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
                    timeline: vec![], // timeline not available at suite level
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

        // Incremental progress file: written after every task so callers can monitor in real-time.
        let progress_path = self.config.output_dir.join("tasks_progress.json");

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

            // Flush incremental progress after each task
            let passed_so_far = results.iter().filter(|r| r.score.passed).count();
            let progress_json = serde_json::json!({
                "completed": results.len(),
                "total": total,
                "passed": passed_so_far,
                "pass_rate": if results.is_empty() { 0.0 } else { passed_so_far as f64 / results.len() as f64 },
                "tasks": results.iter().map(|r| serde_json::json!({
                    "task_id": r.task_id,
                    "passed": r.score.passed,
                    "score": r.score.score,
                    "duration_ms": r.duration_ms,
                })).collect::<Vec<_>>(),
            });
            if let Ok(json_str) = serde_json::to_string_pretty(&progress_json) {
                let _ = std::fs::write(&progress_path, json_str);
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

                let recorder = if config.record_traces {
                    EvalRecorder::new(config.output_dir.join("traces")).ok()
                } else {
                    None
                };
                let runner = EvalRunner {
                    config,
                    provider,
                    recorder,
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

    /// Collect AgentEvents from the stream into an AgentOutput and a TraceEvent timeline.
    async fn collect_events(
        config: AgentLoopConfig,
        messages: Vec<ChatMessage>,
    ) -> (AgentOutput, Vec<TraceEvent>) {
        let stream = run_agent_loop(config, messages);
        futures_util::pin_mut!(stream);

        let mut output = AgentOutput::default();
        let mut timeline: Vec<TraceEvent> = Vec::new();
        let mut current_round: u32 = 0;
        let loop_start = Instant::now();
        let mut pending_tools: std::collections::HashMap<
            String,
            (String, serde_json::Value, Instant),
        > = std::collections::HashMap::new();

        while let Some(event) = stream.next().await {
            match event {
                AgentEvent::IterationStart { round } => {
                    current_round = round;
                    timeline.push(TraceEvent::RoundStart {
                        round,
                        timestamp_ms: loop_start.elapsed().as_millis() as u64,
                    });
                }
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
                        let duration_ms = tool_start.elapsed().as_millis() as u64;
                        timeline.push(TraceEvent::ToolCall {
                            round: current_round,
                            tool_name: name.clone(),
                            input: input.clone(),
                            output: truncate_str(&tool_output, 4000),
                            success,
                            duration_ms,
                        });
                        output.tool_calls.push(ToolCallRecord {
                            name,
                            input,
                            output: tool_output,
                            is_error: !success,
                            duration_ms,
                        });
                    }
                }
                AgentEvent::ThinkingComplete { text } => {
                    timeline.push(TraceEvent::Thinking {
                        round: current_round,
                        content: truncate_str(&text, 2000),
                    });
                }
                AgentEvent::Error { message } => {
                    warn!(error = %message, "Agent loop error during evaluation");
                    timeline.push(TraceEvent::Error {
                        round: current_round,
                        source: "agent".into(),
                        message: truncate_str(&message, 1000),
                    });
                }
                AgentEvent::SecurityBlocked { reason } => {
                    timeline.push(TraceEvent::SecurityBlocked {
                        round: current_round,
                        tool: String::new(),
                        risk_level: String::new(),
                        reason,
                    });
                }
                AgentEvent::ContextDegraded { level, usage_pct } => {
                    timeline.push(TraceEvent::ContextDegraded {
                        round: current_round,
                        stage: level,
                        usage_pct,
                    });
                }
                AgentEvent::TokenBudgetUpdate { budget } => {
                    timeline.push(TraceEvent::BudgetSnapshot {
                        round: current_round,
                        input_used: budget.history as u64,
                        output_used: budget.dynamic_context as u64,
                        limit: budget.total as u64,
                    });
                }
                AgentEvent::Completed(result) => {
                    let total_duration_ms = loop_start.elapsed().as_millis() as u64;
                    timeline.push(TraceEvent::Completed {
                        rounds: result.rounds,
                        stop_reason: format!("{:?}", result.stop_reason),
                        total_duration_ms,
                    });
                    output.rounds = result.rounds;
                    output.input_tokens = result.input_tokens;
                    output.output_tokens = result.output_tokens;
                    output.stop_reason = format!("{:?}", result.stop_reason);
                    output.messages = result.final_messages;
                }
                AgentEvent::EmergencyStopped(reason) => {
                    timeline.push(TraceEvent::Error {
                        round: current_round,
                        source: "emergency_stop".into(),
                        message: reason.unwrap_or_else(|| "E-Stop triggered".into()),
                    });
                }
                // UI-only events (TextDelta, TextComplete, ThinkingDelta, Typing, Done,
                // IterationEnd, ToolExecution, ToolProgress, MemoryFlushed, ApprovalRequired)
                _ => {}
            }
        }

        (output, timeline)
    }
}
