//! Evaluation runner — drives agent loop execution for eval tasks.

use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use futures_util::StreamExt;
use tracing::{info, warn};

use octo_engine::agent::{run_agent_loop, AgentEvent, AgentLoopConfig};
use octo_engine::providers::{create_provider, Provider};
use octo_engine::tools::ToolRegistry;
use octo_types::ChatMessage;

use crate::config::{EvalConfig, EvalTarget};
use crate::model::ModelInfo;
use crate::score::{EvalScore, ScoreDetails};
use crate::task::{AgentOutput, EvalTask, ToolCallRecord};

/// Result of running a single evaluation task
#[derive(Debug)]
pub struct TaskResult {
    pub task_id: String,
    pub output: AgentOutput,
    pub score: EvalScore,
    pub duration_ms: u64,
}

/// Aggregated evaluation report
#[derive(Debug, Default)]
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
}

impl EvalRunner {
    pub fn new(config: EvalConfig) -> Result<Self> {
        let provider = Self::create_provider_from_config(&config)?;
        Ok(Self { config, provider })
    }

    /// Create with an explicit provider (useful for MockProvider in tests)
    pub fn with_provider(config: EvalConfig, provider: Arc<dyn Provider>) -> Self {
        Self { config, provider }
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

    /// Run a single evaluation task
    pub async fn run_task(&self, task: &dyn EvalTask) -> Result<TaskResult> {
        let start = Instant::now();
        let task_id = task.id().to_string();

        info!(task_id = %task_id, "Starting evaluation task");

        let engine_config = match &self.config.target {
            EvalTarget::Engine(c) => c,
        };

        // Build AgentLoopConfig
        let loop_config = AgentLoopConfig::builder()
            .provider(self.provider.clone())
            .model(engine_config.model.clone())
            .max_tokens(engine_config.max_tokens)
            .max_iterations(engine_config.max_iterations)
            .build();

        // Create the initial user message from the task prompt
        let messages = vec![ChatMessage::user(task.prompt())];

        // Run agent loop and collect events into AgentOutput
        let output = Self::collect_events(loop_config, messages).await;

        let duration_ms = start.elapsed().as_millis() as u64;

        // Score the output using the task's scoring function
        let score = task.score(&output);

        info!(
            task_id = %task_id,
            passed = score.passed,
            score = score.score,
            duration_ms = duration_ms,
            "Task evaluation complete"
        );

        Ok(TaskResult {
            task_id,
            output,
            score,
            duration_ms,
        })
    }

    /// Run all tasks and generate an aggregated report
    pub async fn run_suite(&self, tasks: &[Box<dyn EvalTask>]) -> Result<EvalReport> {
        let mut results = Vec::with_capacity(tasks.len());

        for task in tasks {
            match self.run_task(task.as_ref()).await {
                Ok(result) => results.push(result),
                Err(e) => {
                    warn!(task_id = task.id(), error = %e, "Task failed with error");
                    // Record as a failed result rather than aborting the suite
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

        Ok(EvalReport::from_results(results))
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

// Suppress unused import warning — ToolRegistry will be used when tasks
// specify tool subsets (Phase D).
const _: () = {
    fn _assert_tool_registry_imported() {
        let _ = std::mem::size_of::<ToolRegistry>();
    }
};
