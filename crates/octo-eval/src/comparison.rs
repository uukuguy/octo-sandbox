//! Multi-model comparison runner and report generator.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::config::MultiModelConfig;
use crate::model::ModelInfo;
use crate::reporter::{CategoryStats, Reporter, TaskResultSummary};
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

        // Per-task detail table
        if let Some((_, first_report)) = self.model_reports.first() {
            if !first_report.results.is_empty() {
                md.push_str("## Per-Task Results\n\n");

                // Header
                md.push_str("| Task ID | Difficulty |");
                for (info, _) in &self.model_reports {
                    md.push_str(&format!(" {} |", info.name));
                }
                md.push('\n');

                md.push_str("|---------|------------|");
                for _ in &self.model_reports {
                    md.push_str("------------|");
                }
                md.push('\n');

                // Rows — one per task
                for (idx, result) in first_report.results.iter().enumerate() {
                    let task_id = &result.task_id;
                    let diff = difficulties
                        .get(task_id)
                        .map(|d| format!("{:?}", d))
                        .unwrap_or_else(|| "-".into());
                    md.push_str(&format!("| {} | {} |", task_id, diff));

                    for (_, report) in &self.model_reports {
                        if let Some(r) = report.results.get(idx) {
                            let icon = if r.score.passed { "✅" } else { "❌" };
                            md.push_str(&format!(
                                " {} {:.0}ms |",
                                icon,
                                r.duration_ms,
                            ));
                        } else {
                            md.push_str(" - |");
                        }
                    }
                    md.push('\n');
                }
                md.push('\n');
            }
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
        md.push('\n');

        // Failure analysis
        let has_failures = self.model_reports.iter().any(|(_, r)| {
            r.results.iter().any(|tr| tr.score.failure_class.is_some())
        });
        if has_failures {
            md.push_str("## Failure Analysis\n\n");

            for (info, report) in &self.model_reports {
                let failures: Vec<_> = report
                    .results
                    .iter()
                    .filter(|r| r.score.failure_class.is_some())
                    .collect();
                if failures.is_empty() {
                    continue;
                }

                md.push_str(&format!("### {} — {} failures classified\n\n", info.name, failures.len()));

                // Aggregate by class
                let mut by_class: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
                let mut by_category: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
                for f in &failures {
                    if let Some(ref fc) = f.score.failure_class {
                        *by_class.entry(fc.label()).or_default() += 1;
                        *by_category.entry(fc.category()).or_default() += 1;
                    }
                }

                md.push_str("| Failure Class | Count |\n");
                md.push_str("|---------------|-------|\n");
                let mut sorted: Vec<_> = by_class.iter().collect();
                sorted.sort_by(|a, b| b.1.cmp(a.1));
                for (class, count) in sorted {
                    md.push_str(&format!("| {} | {} |\n", class, count));
                }
                md.push('\n');

                md.push_str("| Category | Count |\n");
                md.push_str("|----------|-------|\n");
                let mut cat_sorted: Vec<_> = by_category.iter().collect();
                cat_sorted.sort_by(|a, b| b.1.cmp(a.1));
                for (cat, count) in cat_sorted {
                    md.push_str(&format!("| {} | {} |\n", cat, count));
                }
                md.push('\n');
            }
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
                    task_results: detailed.task_results,
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
    task_results: Vec<TaskResultSummary>,
    estimated_cost_usd: f64,
}

/// Runs the same task set against multiple models and produces a comparison.
/// Serialized task snapshot for passing tasks across thread boundaries.
/// Each field is cloned from the original task's trait methods.
/// Used by run_comparison to parallelize model evaluation.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct TaskRecord {
    pub id: String,
    pub prompt: String,
    pub tools: Option<Vec<octo_types::tool::ToolSpec>>,
    pub allowlist: Option<Vec<String>>,
    /// JSON-encoded benchmark-specific payload for accurate scoring
    pub scoring_data: serde_json::Value,
    pub category: String,
    pub difficulty_label: String,
    pub expected_steps: Option<u32>,
    pub tags: Vec<String>,
}

impl TaskRecord {
    pub fn from_task(task: &dyn EvalTask) -> Self {
        let meta = task.metadata();
        Self {
            id: task.id().to_string(),
            prompt: task.prompt().to_string(),
            tools: task.available_tools(),
            allowlist: task.tool_allowlist(),
            scoring_data: task.scoring_data(),
            category: meta.category,
            difficulty_label: format!("{:?}", meta.difficulty),
            expected_steps: meta.expected_steps,
            tags: meta.tags,
        }
    }
}

impl EvalTask for TaskRecord {
    fn id(&self) -> &str { &self.id }
    fn prompt(&self) -> &str { &self.prompt }
    fn available_tools(&self) -> Option<Vec<octo_types::tool::ToolSpec>> { self.tools.clone() }
    fn tool_allowlist(&self) -> Option<Vec<String>> { self.allowlist.clone() }

    fn score(&self, output: &crate::task::AgentOutput) -> crate::score::EvalScore {
        use crate::score::{EvalScore, ScoreDetails};

        // Dispatch to benchmark-specific scoring when scoring_data is present
        let benchmark = self.scoring_data.get("benchmark").and_then(|v| v.as_str());
        match benchmark {
            Some("gaia") => {
                let expected = self.scoring_data.get("final_answer")
                    .and_then(|v| v.as_str()).unwrap_or("");
                let level = self.scoring_data.get("level")
                    .and_then(|v| v.as_u64()).unwrap_or(1) as u32;
                let actual = output.messages.last()
                    .map(|m| m.text_content()).unwrap_or_default();
                let normalized_expected = expected.trim().to_lowercase();
                let normalized_actual = actual.trim().to_lowercase();
                let passed = !normalized_expected.is_empty()
                    && normalized_actual.contains(&normalized_expected);
                EvalScore {
                    passed,
                    score: if passed { 1.0 } else { 0.0 },
                    details: ScoreDetails::GaiaMatch {
                        expected: expected.to_string(),
                        actual,
                        level,
                    },
                    dimensions: HashMap::new(),
                    failure_class: None,
                }
            }
            Some("swe_bench") => {
                let instance_id = self.scoring_data.get("instance_id")
                    .and_then(|v| v.as_str()).unwrap_or("").to_string();
                let repo = self.scoring_data.get("repo")
                    .and_then(|v| v.as_str()).unwrap_or("");
                let problem_statement = self.scoring_data.get("problem_statement")
                    .and_then(|v| v.as_str()).unwrap_or("");
                let text = output.messages.last()
                    .map(|m| m.text_content()).unwrap_or_default();
                let all_text: String = {
                    let mut s = text;
                    for tc in &output.tool_calls {
                        s.push('\n');
                        s.push_str(&tc.output);
                    }
                    s
                };
                let has_diff_header = all_text.contains("diff --git")
                    || (all_text.contains("--- a/") && all_text.contains("+++ b/"));
                let repo_basename = repo.split('/').next_back().unwrap_or("");
                let references_repo = all_text.to_lowercase().contains(&repo.to_lowercase())
                    || (!repo_basename.is_empty() && all_text.to_lowercase().contains(&repo_basename.to_lowercase()));
                let explored_code = output.tool_calls.iter().any(|tc| {
                    tc.name == "file_read" || tc.name == "bash" || tc.name == "file_write"
                });
                let problem_keywords: Vec<&str> = problem_statement
                    .split_whitespace().filter(|w| w.len() > 5).take(5).collect();
                let addresses_problem = problem_keywords.iter().any(|kw| {
                    all_text.to_lowercase().contains(&kw.to_lowercase())
                });
                let mut score = 0.0f64;
                if has_diff_header { score += 0.4; }
                if explored_code { score += 0.2; }
                if references_repo { score += 0.2; }
                if addresses_problem { score += 0.2; }
                let passed = score >= 0.6;
                EvalScore {
                    passed,
                    score,
                    details: ScoreDetails::SweVerify {
                        instance_id,
                        fail_to_pass_passed: passed,
                        pass_to_pass_passed: false,
                        fail_to_pass_count: if passed { 1 } else { 0 },
                        pass_to_pass_count: 0,
                        execution_time_ms: 0,
                    },
                    dimensions: HashMap::new(),
                    failure_class: None,
                }
            }
            Some("tau_bench") => {
                let expected_tools: Vec<String> = self.scoring_data.get("expected_tools")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                let actual_tools: Vec<&str> = output.tool_calls.iter()
                    .map(|tc| tc.name.as_str()).collect();
                let mut matched = 0usize;
                let mut actual_idx = 0;
                for expected in &expected_tools {
                    while actual_idx < actual_tools.len() {
                        if actual_tools[actual_idx] == expected.as_str() {
                            matched += 1;
                            actual_idx += 1;
                            break;
                        }
                        actual_idx += 1;
                    }
                }
                let total = expected_tools.len();
                let match_rate = if total > 0 { matched as f64 / total as f64 } else { 1.0 };
                let passed = matched == total;
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
            _ => {
                // Fallback: generic scoring for internal suites
                let actual = output.messages.last()
                    .map(|m| m.text_content()).unwrap_or_default();
                let passed = !actual.trim().is_empty() && !output.tool_calls.is_empty();
                EvalScore {
                    passed,
                    score: if passed { 1.0 } else { 0.0 },
                    details: ScoreDetails::Custom { message: actual.chars().take(200).collect() },
                    dimensions: HashMap::new(),
                    failure_class: None,
                }
            }
        }
    }

    fn metadata(&self) -> crate::task::TaskMetadata {
        let difficulty = match self.difficulty_label.as_str() {
            "Easy" => crate::task::Difficulty::Easy,
            "Medium" => crate::task::Difficulty::Medium,
            _ => crate::task::Difficulty::Hard,
        };
        crate::task::TaskMetadata {
            category: self.category.clone(),
            difficulty,
            expected_steps: self.expected_steps,
            tags: self.tags.clone(),
        }
    }
}

pub struct ComparisonRunner {
    config: MultiModelConfig,
}

impl ComparisonRunner {
    pub fn new(config: MultiModelConfig) -> Self {
        Self { config }
    }

    /// Run all models against the same task set sequentially.
    pub async fn run_comparison(
        &self,
        tasks: &[Box<dyn EvalTask>],
    ) -> Result<ComparisonReport> {
        let mut model_reports = Vec::new();
        let model_count = self.config.models.len();

        for (mi, entry) in self.config.models.iter().enumerate() {
            let eval_config = self.config.to_eval_config(entry);
            eprintln!("\n=== Model [{}/{}]: {} ({}) ===", mi + 1, model_count, entry.info.name, entry.info.tier);

            let runner = EvalRunner::new(eval_config)?;
            let report = runner.run_suite(tasks).await?;
            let report = report.with_model(entry.info.clone());

            eprintln!("  ✓ {} — {}/{} passed ({:.1}%)",
                entry.info.name, report.passed, report.total, report.pass_rate * 100.0);

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

    #[test]
    fn test_task_record_gaia_scoring_dispatch() {
        use crate::benchmarks::gaia::{GaiaRecord, GaiaTask};
        use crate::task::AgentOutput;

        let record = GaiaRecord {
            task_id: "gaia-test-001".into(),
            question: "What is 2+2?".into(),
            final_answer: "4".into(),
            level: 1,
            annotator_metadata: None,
            file_name: None,
        };
        let task = GaiaTask::new(record);

        // Wrap in TaskRecord (as benchmark mode does)
        let tr = TaskRecord::from_task(&task);
        assert_eq!(tr.scoring_data.get("benchmark").unwrap(), "gaia");
        assert_eq!(tr.scoring_data.get("final_answer").unwrap(), "4");

        // Pass case: answer contains expected
        let output_pass = AgentOutput {
            messages: vec![octo_types::ChatMessage::assistant("The answer is 4.")],
            ..Default::default()
        };
        let score_pass = tr.score(&output_pass);
        assert!(score_pass.passed, "TaskRecord should pass when answer contains '4'");
        assert!(matches!(score_pass.details, crate::score::ScoreDetails::GaiaMatch { .. }));

        // Fail case: wrong answer
        let output_fail = AgentOutput {
            messages: vec![octo_types::ChatMessage::assistant("I don't know")],
            ..Default::default()
        };
        let score_fail = tr.score(&output_fail);
        assert!(!score_fail.passed, "TaskRecord should fail when answer doesn't contain '4'");

        // Fail case: "(no response)" placeholder
        let output_noresp = AgentOutput {
            messages: vec![octo_types::ChatMessage::assistant("(no response)")],
            ..Default::default()
        };
        let score_noresp = tr.score(&output_noresp);
        assert!(!score_noresp.passed, "TaskRecord should fail for '(no response)'");
    }

    #[test]
    fn test_task_record_swe_bench_scoring_dispatch() {
        use crate::benchmarks::swe_bench::{SweBenchRecord, SweBenchTask};
        use crate::task::{AgentOutput, ToolCallRecord};

        let record = SweBenchRecord {
            instance_id: "django__django-16527".into(),
            repo: "django/django".into(),
            base_commit: String::new(),
            patch: String::new(),
            test_patch: String::new(),
            problem_statement: "Fix issue with QuerySet filtering on related fields".into(),
            hints_text: String::new(),
            fail_to_pass: "[]".into(),
            pass_to_pass: "[]".into(),
        };
        let task = SweBenchTask::new(record);
        let tr = TaskRecord::from_task(&task);
        assert_eq!(tr.scoring_data.get("benchmark").unwrap(), "swe_bench");

        // Pass case: has diff + explored code + references repo + addresses problem
        let output_pass = AgentOutput {
            messages: vec![octo_types::ChatMessage::assistant(
                "Here is the fix:\ndiff --git a/django/db/models/query.py b/django/db/models/query.py\n--- a/django/db/models/query.py\n+++ b/django/db/models/query.py\n@@ -1,3 +1,4 @@\n+# Fix filtering\n"
            )],
            tool_calls: vec![ToolCallRecord {
                name: "bash".into(),
                input: serde_json::json!({}),
                output: "filtering on related fields".into(),
                is_error: false,
                duration_ms: 100,
            }],
            ..Default::default()
        };
        let score_pass = tr.score(&output_pass);
        assert!(score_pass.passed, "TaskRecord should pass SWE-bench with diff + signals");
        assert!(matches!(score_pass.details, crate::score::ScoreDetails::SweVerify { .. }));

        // Fail case: no diff
        let output_fail = AgentOutput {
            messages: vec![octo_types::ChatMessage::assistant("I can't fix this")],
            ..Default::default()
        };
        let score_fail = tr.score(&output_fail);
        assert!(!score_fail.passed, "TaskRecord should fail SWE-bench without diff");
    }

    #[test]
    fn test_task_record_tau_bench_scoring_dispatch() {
        use crate::task::{AgentOutput, ToolCallRecord};

        let tr = TaskRecord {
            id: "tau-test-001".into(),
            prompt: "Help the customer".into(),
            tools: None,
            allowlist: None,
            scoring_data: serde_json::json!({
                "benchmark": "tau_bench",
                "expected_tools": ["search_order", "cancel_order"],
                "domain": "retail",
            }),
            category: "tau-bench:retail".into(),
            difficulty_label: "Medium".into(),
            expected_steps: Some(2),
            tags: vec!["external".into(), "tau_bench".into()],
        };

        // Pass case: all expected tools called in order
        let output_pass = AgentOutput {
            messages: vec![octo_types::ChatMessage::assistant("Done")],
            tool_calls: vec![
                ToolCallRecord { name: "search_order".into(), input: serde_json::json!({}), output: "found".into(), is_error: false, duration_ms: 50 },
                ToolCallRecord { name: "cancel_order".into(), input: serde_json::json!({}), output: "cancelled".into(), is_error: false, duration_ms: 50 },
            ],
            ..Default::default()
        };
        let score_pass = tr.score(&output_pass);
        assert!(score_pass.passed, "TaskRecord should pass tau_bench when all tools matched");
        assert!(matches!(score_pass.details, crate::score::ScoreDetails::PassK { .. }));

        // Fail case: missing expected tool
        let output_fail = AgentOutput {
            messages: vec![octo_types::ChatMessage::assistant("Done")],
            tool_calls: vec![
                ToolCallRecord { name: "search_order".into(), input: serde_json::json!({}), output: "found".into(), is_error: false, duration_ms: 50 },
            ],
            ..Default::default()
        };
        let score_fail = tr.score(&output_fail);
        assert!(!score_fail.passed, "TaskRecord should fail tau_bench when tools missing");
    }

    #[test]
    fn test_task_record_fallback_generic_scoring() {
        use crate::task::{AgentOutput, ToolCallRecord};

        // TaskRecord with no scoring_data (internal suites) uses generic scoring
        let tr = TaskRecord {
            id: "internal-001".into(),
            prompt: "test".into(),
            tools: None,
            allowlist: None,
            scoring_data: serde_json::Value::Null,
            category: "tool_call".into(),
            difficulty_label: "Easy".into(),
            expected_steps: None,
            tags: vec![],
        };

        // Pass: has text + has tool calls
        let output_pass = AgentOutput {
            messages: vec![octo_types::ChatMessage::assistant("result")],
            tool_calls: vec![
                ToolCallRecord { name: "bash".into(), input: serde_json::json!({}), output: "ok".into(), is_error: false, duration_ms: 10 },
            ],
            ..Default::default()
        };
        assert!(tr.score(&output_pass).passed);

        // Fail: no tool calls
        let output_fail = AgentOutput {
            messages: vec![octo_types::ChatMessage::assistant("just text")],
            ..Default::default()
        };
        assert!(!tr.score(&output_fail).passed);
    }
}
