//! Multi-model comparison runner and report generator.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::config::MultiModelConfig;
use crate::model::ModelInfo;
use crate::reporter::{CategoryStats, Reporter};
use crate::runner::{EvalReport, EvalRunner};
use crate::task::{Difficulty, EvalTask};

/// Result of comparing multiple models on the same task set.
#[derive(Debug)]
pub struct ComparisonReport {
    pub model_reports: Vec<(ModelInfo, EvalReport)>,
}

impl ComparisonReport {
    /// Number of models in the comparison.
    pub fn model_count(&self) -> usize {
        self.model_reports.len()
    }

    /// Generate a comparison Markdown report.
    pub fn to_markdown(
        &self,
        categories: &HashMap<String, String>,
        difficulties: &HashMap<String, Difficulty>,
    ) -> String {
        let mut md = String::new();

        md.push_str("# Multi-Model Comparison Report\n\n");

        // Summary table
        md.push_str("## Model Summary\n\n");
        md.push_str("| Model | Tier | Tasks | Passed | Pass Rate | Avg Score | Tokens | Est. Cost | Duration |\n");
        md.push_str("|-------|------|-------|--------|-----------|-----------|--------|-----------|----------|\n");

        for (info, report) in &self.model_reports {
            let cost = report.estimated_cost();
            md.push_str(&format!(
                "| {} | {} | {} | {} | {:.1}% | {:.3} | {} | ${:.4} | {}ms |\n",
                info.name,
                info.tier,
                report.total,
                report.passed,
                report.pass_rate * 100.0,
                report.avg_score,
                report.total_tokens,
                cost,
                report.total_duration_ms,
            ));
        }
        md.push('\n');

        // Per-category comparison
        let all_categories: Vec<String> = {
            let mut cats: Vec<String> = categories.values().cloned().collect();
            cats.sort();
            cats.dedup();
            cats
        };

        if !all_categories.is_empty() {
            md.push_str("## By Category\n\n");
            md.push_str("| Category |");
            for (info, _) in &self.model_reports {
                md.push_str(&format!(" {} |", info.name));
            }
            md.push('\n');

            md.push_str("|----------|");
            for _ in &self.model_reports {
                md.push_str("----------|");
            }
            md.push('\n');

            for cat in &all_categories {
                md.push_str(&format!("| {} |", cat));
                for (_, report) in &self.model_reports {
                    let detailed = Reporter::generate(report, categories, difficulties);
                    let stats = detailed
                        .by_category
                        .get(cat)
                        .cloned()
                        .unwrap_or_default();
                    md.push_str(&format!(" {:.1}% ({}/{}) |", stats.pass_rate * 100.0, stats.passed, stats.total));
                }
                md.push('\n');
            }
            md.push('\n');
        }

        // Cost-effectiveness analysis
        md.push_str("## Cost-Effectiveness\n\n");
        md.push_str("| Model | Cost/Task | Score/Dollar | Tier |\n");
        md.push_str("|-------|-----------|-------------|------|\n");

        for (info, report) in &self.model_reports {
            let cost = report.estimated_cost();
            let cost_per_task = if report.total > 0 {
                cost / report.total as f64
            } else {
                0.0
            };
            let score_per_dollar = if cost > 0.0 {
                report.avg_score / cost
            } else {
                f64::INFINITY
            };
            let score_str = if score_per_dollar.is_infinite() {
                "∞ (free)".to_string()
            } else {
                format!("{:.1}", score_per_dollar)
            };
            md.push_str(&format!(
                "| {} | ${:.6} | {} | {} |\n",
                info.name, cost_per_task, score_str, info.tier,
            ));
        }

        md
    }

    /// Generate a comparison JSON report.
    pub fn to_json(
        &self,
        categories: &HashMap<String, String>,
        difficulties: &HashMap<String, Difficulty>,
    ) -> String {
        let entries: Vec<ComparisonJsonEntry> = self
            .model_reports
            .iter()
            .map(|(info, report)| {
                let detailed = Reporter::generate(report, categories, difficulties);
                ComparisonJsonEntry {
                    model: info.clone(),
                    summary: detailed.summary,
                    by_category: detailed.by_category,
                    by_difficulty: detailed.by_difficulty,
                    latency: detailed.latency,
                    token_usage: detailed.token_usage,
                    estimated_cost_usd: report.estimated_cost(),
                }
            })
            .collect();

        serde_json::to_string_pretty(&entries).unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
    }
}

#[derive(Serialize, Deserialize)]
struct ComparisonJsonEntry {
    model: ModelInfo,
    summary: crate::reporter::ReportSummary,
    by_category: HashMap<String, CategoryStats>,
    by_difficulty: HashMap<String, CategoryStats>,
    latency: crate::reporter::LatencyStats,
    token_usage: crate::reporter::TokenUsageStats,
    estimated_cost_usd: f64,
}

/// Runs the same task set against multiple models and produces a comparison.
pub struct ComparisonRunner {
    config: MultiModelConfig,
}

impl ComparisonRunner {
    pub fn new(config: MultiModelConfig) -> Self {
        Self { config }
    }

    /// Run all models against the same task set.
    pub async fn run_comparison(
        &self,
        tasks: &[Box<dyn EvalTask>],
    ) -> Result<ComparisonReport> {
        let mut model_reports = Vec::new();

        for entry in &self.config.models {
            let eval_config = self.config.to_eval_config(entry);
            info!(model = %entry.info.name, tier = %entry.info.tier, "Starting model evaluation");

            let runner = EvalRunner::new(eval_config)?;
            let report = runner.run_suite(tasks).await?;
            let report = report.with_model(entry.info.clone());

            info!(
                model = %entry.info.name,
                passed = report.passed,
                total = report.total,
                pass_rate = format!("{:.1}%", report.pass_rate * 100.0),
                "Model evaluation complete"
            );

            model_reports.push((entry.info.clone(), report));
        }

        Ok(ComparisonReport { model_reports })
    }

    /// Run comparison with explicit providers (useful for testing with MockProvider).
    pub async fn run_comparison_with_providers(
        models: Vec<(ModelInfo, Arc<dyn octo_engine::providers::Provider>)>,
        tasks: &[Box<dyn EvalTask>],
        config: &MultiModelConfig,
    ) -> Result<ComparisonReport> {
        let mut model_reports = Vec::new();

        for (info, provider) in models {
            let eval_config = EvalConfig {
                target: crate::config::EvalTarget::Engine(crate::config::EngineConfig::default()),
                concurrency: config.concurrency,
                timeout_secs: config.timeout_secs,
                record_traces: config.record_traces,
                output_dir: config.output_dir.clone(),
            };

            let runner = EvalRunner::with_provider(eval_config, provider);
            let report = runner.run_suite(tasks).await?;
            let report = report.with_model(info.clone());

            model_reports.push((info, report));
        }

        Ok(ComparisonReport { model_reports })
    }
}

// Re-export EvalConfig for use in run_comparison_with_providers
use crate::config::EvalConfig;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_provider::MockProvider;
    use crate::model::ModelTier;
    use crate::score::{EvalScore, ScoreDetails};
    use crate::task::{AgentOutput, TaskMetadata};

    struct SimpleTask {
        id: String,
        prompt: String,
    }

    impl EvalTask for SimpleTask {
        fn id(&self) -> &str {
            &self.id
        }
        fn prompt(&self) -> &str {
            &self.prompt
        }
        fn available_tools(&self) -> Option<Vec<octo_types::tool::ToolSpec>> {
            None
        }
        fn score(&self, _output: &AgentOutput) -> EvalScore {
            EvalScore::pass(
                1.0,
                ScoreDetails::Custom {
                    message: "auto-pass".into(),
                },
            )
        }
        fn metadata(&self) -> TaskMetadata {
            TaskMetadata {
                category: "test".into(),
                difficulty: Difficulty::Easy,
                expected_steps: None,
                tags: vec![],
            }
        }
    }

    #[tokio::test]
    async fn test_comparison_with_mock_providers() {
        let model_a = ModelInfo {
            name: "Model-A".into(),
            model_id: "model-a".into(),
            provider: "mock".into(),
            tier: ModelTier::Economy,
            cost_per_1m_input: 0.15,
            cost_per_1m_output: 0.75,
        };

        let model_b = ModelInfo {
            name: "Model-B".into(),
            model_id: "model-b".into(),
            provider: "mock".into(),
            tier: ModelTier::Flagship,
            cost_per_1m_input: 3.0,
            cost_per_1m_output: 15.0,
        };

        let provider_a = Arc::new(MockProvider::with_text("answer from A"));
        let provider_b = Arc::new(MockProvider::with_text("answer from B"));

        let tasks: Vec<Box<dyn EvalTask>> = vec![
            Box::new(SimpleTask {
                id: "t1".into(),
                prompt: "test question".into(),
            }),
        ];

        let config = MultiModelConfig::default();
        let report = ComparisonRunner::run_comparison_with_providers(
            vec![(model_a, provider_a), (model_b, provider_b)],
            &tasks,
            &config,
        )
        .await
        .unwrap();

        assert_eq!(report.model_count(), 2);
        assert_eq!(report.model_reports[0].0.name, "Model-A");
        assert_eq!(report.model_reports[1].0.name, "Model-B");

        // Both should pass since SimpleTask auto-passes
        assert_eq!(report.model_reports[0].1.passed, 1);
        assert_eq!(report.model_reports[1].1.passed, 1);
    }

    #[tokio::test]
    async fn test_comparison_markdown_report() {
        let model_a = ModelInfo {
            name: "Cheap".into(),
            model_id: "cheap".into(),
            provider: "mock".into(),
            tier: ModelTier::Economy,
            cost_per_1m_input: 0.1,
            cost_per_1m_output: 0.5,
        };

        let provider_a = Arc::new(MockProvider::with_text("ok"));
        let tasks: Vec<Box<dyn EvalTask>> = vec![
            Box::new(SimpleTask {
                id: "t1".into(),
                prompt: "hello".into(),
            }),
        ];

        let config = MultiModelConfig::default();
        let report = ComparisonRunner::run_comparison_with_providers(
            vec![(model_a, provider_a)],
            &tasks,
            &config,
        )
        .await
        .unwrap();

        let categories: HashMap<String, String> = [("t1".into(), "tool_call".into())].into();
        let difficulties: HashMap<String, Difficulty> = [("t1".into(), Difficulty::Easy)].into();

        let md = report.to_markdown(&categories, &difficulties);
        assert!(md.contains("# Multi-Model Comparison Report"));
        assert!(md.contains("Cheap"));
        assert!(md.contains("T1-Economy"));
    }

    #[tokio::test]
    async fn test_comparison_json_report() {
        let model = ModelInfo {
            name: "TestModel".into(),
            model_id: "test".into(),
            provider: "mock".into(),
            tier: ModelTier::Standard,
            cost_per_1m_input: 0.0,
            cost_per_1m_output: 0.0,
        };

        let provider = Arc::new(MockProvider::with_text("done"));
        let tasks: Vec<Box<dyn EvalTask>> = vec![
            Box::new(SimpleTask {
                id: "t1".into(),
                prompt: "q".into(),
            }),
        ];

        let config = MultiModelConfig::default();
        let report = ComparisonRunner::run_comparison_with_providers(
            vec![(model, provider)],
            &tasks,
            &config,
        )
        .await
        .unwrap();

        let json = report.to_json(&HashMap::new(), &HashMap::new());
        assert!(json.contains("TestModel"));
        assert!(json.contains("estimated_cost_usd"));
    }

    #[test]
    fn test_empty_comparison_report() {
        let report = ComparisonReport {
            model_reports: vec![],
        };
        assert_eq!(report.model_count(), 0);

        let md = report.to_markdown(&HashMap::new(), &HashMap::new());
        assert!(md.contains("# Multi-Model Comparison Report"));
    }
}
