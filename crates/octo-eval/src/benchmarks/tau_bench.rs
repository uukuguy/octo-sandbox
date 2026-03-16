//! τ-bench benchmark adapter — multi-turn tool-use consistency evaluation.
//!
//! τ-bench measures agent behavioral reliability through pass^k metrics.
//! A task that passes 80% of individual runs may only have 25% pass^8 consistency.

use std::path::PathBuf;

use std::collections::HashMap;

use serde::Deserialize;

use crate::benchmarks::{ExternalBenchmark, MetricDefinition};
use crate::score::{EvalScore, ScoreDetails};
use crate::task::{AgentOutput, Difficulty, EvalTask, TaskMetadata};

/// A single τ-bench evaluation record parsed from JSONL
#[derive(Debug, Clone, Deserialize)]
pub struct TauBenchRecord {
    pub task_id: String,
    pub domain: String,
    pub user_instruction: String,
    #[serde(default)]
    pub policy_rules: Vec<String>,
    #[serde(default)]
    pub available_tools: Vec<String>,
    #[serde(default)]
    pub expected_actions: Vec<TauExpectedAction>,
    #[serde(default)]
    pub expected_db_state: serde_json::Value,
    #[serde(default = "default_k")]
    pub k: u32,
}

fn default_k() -> u32 {
    8
}

#[derive(Debug, Clone, Deserialize)]
pub struct TauExpectedAction {
    pub tool: String,
    #[serde(default)]
    pub args: serde_json::Value,
}

/// EvalTask implementation for a single τ-bench task
pub struct TauBenchTask {
    record: TauBenchRecord,
}

impl TauBenchTask {
    pub fn new(record: TauBenchRecord) -> Self {
        Self { record }
    }
}

impl EvalTask for TauBenchTask {
    fn id(&self) -> &str {
        &self.record.task_id
    }

    fn prompt(&self) -> &str {
        &self.record.user_instruction
    }

    fn available_tools(&self) -> Option<Vec<octo_types::tool::ToolSpec>> {
        if self.record.available_tools.is_empty() {
            return None;
        }
        // Build ToolSpec for each business tool declared in the task record.
        // These are injected as EvalMockTools by the runner so the agent can actually call them.
        let specs = self.record.available_tools.iter().map(|tool_name| {
            let (description, schema) = tau_tool_spec(tool_name);
            octo_types::tool::ToolSpec {
                name: tool_name.clone(),
                description: description.to_string(),
                input_schema: schema,
            }
        }).collect();
        Some(specs)
    }

    fn tool_allowlist(&self) -> Option<Vec<String>> {
        if self.record.available_tools.is_empty() {
            None
        } else {
            Some(self.record.available_tools.clone())
        }
    }

    fn score(&self, output: &AgentOutput) -> EvalScore {
        // Check if the agent called the expected tools in order
        let expected_tools: Vec<&str> = self
            .record
            .expected_actions
            .iter()
            .map(|a| a.tool.as_str())
            .collect();
        let actual_tools: Vec<&str> = output.tool_calls.iter().map(|tc| tc.name.as_str()).collect();

        let mut matched = 0;
        let mut actual_idx = 0;
        for expected in &expected_tools {
            while actual_idx < actual_tools.len() {
                if actual_tools[actual_idx] == *expected {
                    matched += 1;
                    actual_idx += 1;
                    break;
                }
                actual_idx += 1;
            }
        }

        let total = expected_tools.len();
        let match_rate = if total > 0 {
            matched as f64 / total as f64
        } else {
            1.0
        };
        let passed = matched == total;

        // Single-run score; pass^k requires multiple runs (handled by TauVerifier)
        EvalScore {
            passed,
            score: match_rate,
            details: ScoreDetails::PassK {
                k: 1,
                passes: if passed { 1 } else { 0 },
                pass_at_1: if passed { 1.0 } else { 0.0 },
                pass_at_k: if passed { 1.0 } else { 0.0 },
            },
            dimensions: HashMap::new(),
            failure_class: None,
        }
    }

    fn scoring_data(&self) -> serde_json::Value {
        let expected_tools: Vec<&str> = self.record.expected_actions.iter()
            .map(|a| a.tool.as_str()).collect();
        serde_json::json!({
            "benchmark": "tau_bench",
            "expected_tools": expected_tools,
            "domain": self.record.domain,
        })
    }

    fn metadata(&self) -> TaskMetadata {
        let difficulty = match self.record.expected_actions.len() {
            0..=2 => Difficulty::Easy,
            3..=4 => Difficulty::Medium,
            _ => Difficulty::Hard,
        };

        TaskMetadata {
            category: format!("tau-bench:{}", self.record.domain),
            difficulty,
            expected_steps: Some(self.record.expected_actions.len() as u32),
            tags: vec!["external".into(), "tau_bench".into()],
        }
    }
}

/// Return (description, input_schema) for known τ-bench retail business tools.
fn tau_tool_spec(tool_name: &str) -> (&'static str, serde_json::Value) {
    match tool_name {
        "lookup_order" => (
            "Look up order details by order ID. Returns order status, items, purchase date, and payment info.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "order_id": {"type": "string", "description": "The order ID to look up"}
                },
                "required": ["order_id"]
            }),
        ),
        "check_return_eligibility" => (
            "Check if an order is eligible for return based on policy rules. Returns eligibility status and reason.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "order_id": {"type": "string", "description": "The order ID to check"},
                    "reason": {"type": "string", "description": "Reason for return (e.g. incorrect_fit, defective, not_as_expected)"},
                    "has_receipt": {"type": "boolean", "description": "Whether the customer has a receipt"},
                    "defect_description": {"type": "string", "description": "Description of defect if applicable"}
                },
                "required": ["order_id", "reason"]
            }),
        ),
        "process_return" => (
            "Process a return or refund for an order. Returns confirmation and refund details.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "order_id": {"type": "string", "description": "The order ID to process return for"},
                    "refund_type": {"type": "string", "enum": ["full", "store_credit", "replacement"], "description": "Type of refund"},
                    "refund_method": {"type": "string", "description": "Refund method (original_payment, store_credit)"},
                    "items": {"type": "array", "items": {"type": "string"}, "description": "Specific items to return for partial returns"},
                    "restocking_fee_percent": {"type": "number", "description": "Restocking fee percentage if applicable"},
                    "free_return_shipping": {"type": "boolean", "description": "Whether return shipping is free"}
                },
                "required": ["order_id", "refund_type"]
            }),
        ),
        "send_confirmation" => (
            "Send a confirmation notification to the customer about their return or refund.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "order_id": {"type": "string", "description": "The order ID"},
                    "type": {"type": "string", "description": "Type of confirmation (return_initiated, partial_return_initiated, refund_processed)"},
                    "channel": {"type": "string", "enum": ["email", "sms", "push"], "description": "Notification channel"}
                },
                "required": ["order_id", "type"]
            }),
        ),
        "update_inventory" => (
            "Update inventory records after a return or replacement is processed.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": ["restock", "reserve_replacement", "mark_defective"], "description": "Inventory action to perform"},
                    "order_id": {"type": "string", "description": "The related order ID"},
                    "item_sku": {"type": "string", "description": "SKU of the item to update"}
                },
                "required": ["action"]
            }),
        ),
        _ => (
            "Business tool for retail customer service operations.",
            serde_json::json!({"type": "object", "properties": {}}),
        ),
    }
}

/// pass^k calculator — measures consistency across k independent runs
pub struct PassKCalculator;

impl PassKCalculator {
    /// Calculate pass^k: probability that ALL k runs pass.
    /// Given individual pass results, pass^k = (passes/total)^k
    pub fn calculate(results: &[bool], k: u32) -> (f64, f64) {
        if results.is_empty() {
            return (0.0, 0.0);
        }
        let passes = results.iter().filter(|&&r| r).count();
        let pass_at_1 = passes as f64 / results.len() as f64;
        let pass_at_k = pass_at_1.powi(k as i32);
        (pass_at_1, pass_at_k)
    }
}

/// τ-bench benchmark adapter
pub struct TauBenchmark {
    dataset_path: Option<PathBuf>,
}

impl TauBenchmark {
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
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("datasets/tau_bench_retail.jsonl")
    }

    pub fn load_from_jsonl(path: &std::path::Path) -> anyhow::Result<Vec<Box<dyn EvalTask>>> {
        let content = std::fs::read_to_string(path)?;
        let mut tasks: Vec<Box<dyn EvalTask>> = Vec::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let record: TauBenchRecord = serde_json::from_str(line)?;
            tasks.push(Box::new(TauBenchTask::new(record)));
        }

        Ok(tasks)
    }
}

impl ExternalBenchmark for TauBenchmark {
    fn name(&self) -> &str {
        "tau_bench"
    }

    fn description(&self) -> &str {
        "τ-bench — multi-turn tool-use consistency evaluation with pass^k metrics"
    }

    fn load_tasks(&self) -> anyhow::Result<Vec<Box<dyn EvalTask>>> {
        let path = self
            .dataset_path
            .clone()
            .unwrap_or_else(Self::default_dataset_path);

        if !path.exists() {
            anyhow::bail!(
                "τ-bench dataset not found at {}. Download or create tau_bench_retail.jsonl.",
                path.display()
            );
        }

        Self::load_from_jsonl(&path)
    }

    fn custom_metrics(&self) -> Vec<MetricDefinition> {
        vec![
            MetricDefinition {
                name: "pass_at_1".into(),
                description: "Single-run pass rate".into(),
                unit: crate::benchmarks::MetricUnit::Percentage,
            },
            MetricDefinition {
                name: "pass_at_k".into(),
                description: "Consistency: probability all k runs pass".into(),
                unit: crate::benchmarks::MetricUnit::Percentage,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tau_record_deserialize() {
        let json = r#"{"task_id":"tau-001","domain":"retail","user_instruction":"Return order #123","expected_actions":[{"tool":"lookup_order","args":{"id":"123"}}]}"#;
        let record: TauBenchRecord = serde_json::from_str(json).unwrap();
        assert_eq!(record.task_id, "tau-001");
        assert_eq!(record.domain, "retail");
        assert_eq!(record.k, 8); // default
        assert_eq!(record.expected_actions.len(), 1);
    }

    #[test]
    fn test_pass_k_calculator() {
        // All pass: pass^8 = 1.0
        let results = vec![true, true, true, true, true];
        let (p1, pk) = PassKCalculator::calculate(&results, 8);
        assert_eq!(p1, 1.0);
        assert_eq!(pk, 1.0);

        // 80% pass rate: pass^8 = 0.8^8 ≈ 0.168
        let results = vec![true, true, true, true, false];
        let (p1, pk) = PassKCalculator::calculate(&results, 8);
        assert!((p1 - 0.8).abs() < 0.001);
        assert!((pk - 0.8_f64.powi(8)).abs() < 0.001);

        // Empty: 0.0
        let (p1, pk) = PassKCalculator::calculate(&[], 8);
        assert_eq!(p1, 0.0);
        assert_eq!(pk, 0.0);
    }

    #[test]
    fn test_tau_benchmark_trait() {
        let bm = TauBenchmark::new();
        assert_eq!(bm.name(), "tau_bench");
        assert!(!bm.requires_sandbox());
        assert!(bm.custom_verifier().is_none());
        assert_eq!(bm.custom_metrics().len(), 2);
    }
}
