//! Benchmark aggregator — combines multi-suite comparison reports into a unified benchmark report.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::comparison::ComparisonReport;
use crate::model::ModelInfo;

/// Aggregated benchmark report across multiple evaluation suites.
#[derive(Debug, Serialize, Deserialize)]
pub struct BenchmarkReport {
    pub models: Vec<ModelBenchmark>,
    /// model_name → suite_name → pass_rate
    pub dimension_matrix: HashMap<String, HashMap<String, f64>>,
    pub cost_analysis: CostAnalysis,
    pub recommendations: Vec<Recommendation>,
}

/// Per-model benchmark summary.
#[derive(Debug, Serialize, Deserialize)]
pub struct ModelBenchmark {
    pub info: ModelInfo,
    pub overall_pass_rate: f64,
    pub overall_avg_score: f64,
    pub total_tokens: u64,
    pub estimated_cost: f64,
    pub per_suite: HashMap<String, SuiteResult>,
}

/// Result for a single suite evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuiteResult {
    pub total: usize,
    pub passed: usize,
    pub pass_rate: f64,
    pub avg_score: f64,
    pub tokens: u64,
    pub duration_ms: u64,
    pub estimated_cost: f64,
}

/// Cost analysis across all models.
#[derive(Debug, Serialize, Deserialize)]
pub struct CostAnalysis {
    pub cost_per_model: HashMap<String, f64>,
    /// pass_rate / cost — higher is better
    pub cost_effectiveness: HashMap<String, f64>,
    /// Model with pass_rate > 80% and lowest cost
    pub cheapest_acceptable: Option<String>,
}

/// Scenario-based recommendation.
#[derive(Debug, Serialize, Deserialize)]
pub struct Recommendation {
    pub scenario: String,
    pub recommended_model: String,
    pub reasoning: String,
}

/// Aggregates multiple suite ComparisonReports into a unified BenchmarkReport.
pub struct BenchmarkAggregator;

impl BenchmarkAggregator {
    /// Aggregate comparison reports from multiple suites.
    pub fn aggregate(suite_reports: Vec<(&str, &ComparisonReport)>) -> BenchmarkReport {
        // Collect all model names (preserving order from first report)
        let mut model_names: Vec<String> = Vec::new();
        let mut model_info_map: HashMap<String, ModelInfo> = HashMap::new();

        for (_, report) in &suite_reports {
            for (info, _) in &report.model_reports {
                if !model_info_map.contains_key(&info.name) {
                    model_names.push(info.name.clone());
                    model_info_map.insert(info.name.clone(), info.clone());
                }
            }
        }

        // Build per-model, per-suite results
        let mut model_suites: HashMap<String, HashMap<String, SuiteResult>> = HashMap::new();
        let mut dimension_matrix: HashMap<String, HashMap<String, f64>> = HashMap::new();

        for (suite_name, report) in &suite_reports {
            for (info, eval_report) in &report.model_reports {
                let suite_result = SuiteResult {
                    total: eval_report.total,
                    passed: eval_report.passed,
                    pass_rate: eval_report.pass_rate,
                    avg_score: eval_report.avg_score,
                    tokens: eval_report.total_tokens,
                    duration_ms: eval_report.total_duration_ms,
                    estimated_cost: eval_report.estimated_cost(),
                };

                model_suites
                    .entry(info.name.clone())
                    .or_default()
                    .insert(suite_name.to_string(), suite_result);

                dimension_matrix
                    .entry(info.name.clone())
                    .or_default()
                    .insert(suite_name.to_string(), eval_report.pass_rate);
            }
        }

        // Build ModelBenchmark entries
        let mut models: Vec<ModelBenchmark> = Vec::new();
        for name in &model_names {
            let info = model_info_map[name].clone();
            let suites = model_suites.remove(name).unwrap_or_default();

            let total_tasks: usize = suites.values().map(|s| s.total).sum();
            let total_passed: usize = suites.values().map(|s| s.passed).sum();
            let overall_pass_rate = if total_tasks > 0 {
                total_passed as f64 / total_tasks as f64
            } else {
                0.0
            };

            let suite_count = suites.len();
            let overall_avg_score = if suite_count > 0 {
                suites.values().map(|s| s.avg_score).sum::<f64>() / suite_count as f64
            } else {
                0.0
            };

            let total_tokens: u64 = suites.values().map(|s| s.tokens).sum();
            let estimated_cost: f64 = suites.values().map(|s| s.estimated_cost).sum();

            models.push(ModelBenchmark {
                info,
                overall_pass_rate,
                overall_avg_score,
                total_tokens,
                estimated_cost,
                per_suite: suites,
            });
        }

        // Cost analysis
        let cost_analysis = Self::build_cost_analysis(&models);

        // Recommendations
        let recommendations = Self::build_recommendations(&models);

        BenchmarkReport {
            models,
            dimension_matrix,
            cost_analysis,
            recommendations,
        }
    }

    fn build_cost_analysis(models: &[ModelBenchmark]) -> CostAnalysis {
        let mut cost_per_model = HashMap::new();
        let mut cost_effectiveness = HashMap::new();

        for m in models {
            cost_per_model.insert(m.info.name.clone(), m.estimated_cost);
            let effectiveness = if m.estimated_cost > 0.0 {
                m.overall_pass_rate / m.estimated_cost
            } else if m.overall_pass_rate > 0.0 {
                f64::INFINITY
            } else {
                0.0
            };
            cost_effectiveness.insert(m.info.name.clone(), effectiveness);
        }

        // Find cheapest model with pass_rate > 80%
        let cheapest_acceptable = models
            .iter()
            .filter(|m| m.overall_pass_rate >= 0.80)
            .min_by(|a, b| a.estimated_cost.partial_cmp(&b.estimated_cost).unwrap_or(std::cmp::Ordering::Equal))
            .map(|m| m.info.name.clone());

        CostAnalysis {
            cost_per_model,
            cost_effectiveness,
            cheapest_acceptable,
        }
    }

    fn build_recommendations(models: &[ModelBenchmark]) -> Vec<Recommendation> {
        let mut recs = Vec::new();

        if models.is_empty() {
            return recs;
        }

        // Cost-sensitive: cheapest with pass_rate > 70%
        if let Some(m) = models
            .iter()
            .filter(|m| m.overall_pass_rate >= 0.70)
            .min_by(|a, b| a.estimated_cost.partial_cmp(&b.estimated_cost).unwrap_or(std::cmp::Ordering::Equal))
        {
            recs.push(Recommendation {
                scenario: "cost_sensitive".into(),
                recommended_model: m.info.name.clone(),
                reasoning: format!(
                    "Pass rate {:.1}% at ${:.4} — lowest cost above 70% threshold",
                    m.overall_pass_rate * 100.0,
                    m.estimated_cost,
                ),
            });
        }

        // Balanced: best pass_rate / cost ratio
        if let Some(m) = models
            .iter()
            .filter(|m| m.overall_pass_rate > 0.0)
            .max_by(|a, b| {
                let ratio_a = if a.estimated_cost > 0.0 {
                    a.overall_pass_rate / a.estimated_cost
                } else {
                    f64::MAX
                };
                let ratio_b = if b.estimated_cost > 0.0 {
                    b.overall_pass_rate / b.estimated_cost
                } else {
                    f64::MAX
                };
                ratio_a.partial_cmp(&ratio_b).unwrap_or(std::cmp::Ordering::Equal)
            })
        {
            recs.push(Recommendation {
                scenario: "balanced".into(),
                recommended_model: m.info.name.clone(),
                reasoning: format!(
                    "Best cost-effectiveness ratio: {:.1}% pass rate at ${:.4}",
                    m.overall_pass_rate * 100.0,
                    m.estimated_cost,
                ),
            });
        }

        // Performance-first: highest overall pass_rate
        if let Some(m) = models
            .iter()
            .max_by(|a, b| a.overall_pass_rate.partial_cmp(&b.overall_pass_rate).unwrap_or(std::cmp::Ordering::Equal))
        {
            recs.push(Recommendation {
                scenario: "performance_first".into(),
                recommended_model: m.info.name.clone(),
                reasoning: format!(
                    "Highest pass rate: {:.1}% across all suites",
                    m.overall_pass_rate * 100.0,
                ),
            });
        }

        recs
    }

    /// Generate a comprehensive Markdown benchmark report.
    pub fn to_markdown(report: &BenchmarkReport) -> String {
        let mut md = String::new();

        md.push_str("# Octo Agent Benchmark Report\n\n");

        // Overview table
        md.push_str("## Overview\n\n");

        // Collect suite names from first model
        let suite_names: Vec<String> = if let Some(first) = report.models.first() {
            let mut names: Vec<String> = first.per_suite.keys().cloned().collect();
            names.sort();
            names
        } else {
            vec![]
        };

        // Header
        md.push_str("| Model | Tier |");
        for suite in &suite_names {
            md.push_str(&format!(" {} |", suite));
        }
        md.push_str(" Overall | Cost | Effectiveness |\n");

        md.push_str("|-------|------|");
        for _ in &suite_names {
            md.push_str("--------|");
        }
        md.push_str("---------|------|---------------|\n");

        // Rows
        for m in &report.models {
            md.push_str(&format!("| {} | {} |", m.info.name, m.info.tier));
            for suite in &suite_names {
                let rate = m
                    .per_suite
                    .get(suite)
                    .map(|s| format!("{:.1}%", s.pass_rate * 100.0))
                    .unwrap_or_else(|| "N/A".into());
                md.push_str(&format!(" {} |", rate));
            }
            let effectiveness = if m.estimated_cost > 0.0 {
                format!("{:.1}", m.overall_pass_rate / m.estimated_cost)
            } else if m.overall_pass_rate > 0.0 {
                "inf".into()
            } else {
                "0".into()
            };
            md.push_str(&format!(
                " {:.1}% | ${:.4} | {} |\n",
                m.overall_pass_rate * 100.0,
                m.estimated_cost,
                effectiveness,
            ));
        }
        md.push('\n');

        // Dimension sensitivity analysis
        if suite_names.len() >= 2 && report.models.len() >= 2 {
            md.push_str("## Dimension Sensitivity Analysis\n\n");
            md.push_str("Gap between cheapest and most expensive model per dimension:\n\n");
            md.push_str("| Dimension | Min Tier Rate | Max Tier Rate | Gap | Sensitivity |\n");
            md.push_str("|-----------|---------------|---------------|-----|-------------|\n");

            for suite in &suite_names {
                let rates: Vec<f64> = report
                    .models
                    .iter()
                    .filter_map(|m| m.per_suite.get(suite).map(|s| s.pass_rate))
                    .collect();
                if rates.len() >= 2 {
                    let min_rate = rates.iter().cloned().fold(f64::INFINITY, f64::min);
                    let max_rate = rates.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                    let gap = max_rate - min_rate;
                    let sensitivity = if gap > 0.3 {
                        "HIGH"
                    } else if gap > 0.15 {
                        "MEDIUM"
                    } else {
                        "LOW"
                    };
                    md.push_str(&format!(
                        "| {} | {:.1}% | {:.1}% | {:.1}% | {} |\n",
                        suite,
                        min_rate * 100.0,
                        max_rate * 100.0,
                        gap * 100.0,
                        sensitivity,
                    ));
                }
            }
            md.push('\n');
        }

        // Cost analysis
        md.push_str("## Cost Analysis\n\n");
        md.push_str("| Model | Total Cost | Cost/Task | Effectiveness (rate/$) |\n");
        md.push_str("|-------|-----------|-----------|------------------------|\n");

        for m in &report.models {
            let total_tasks: usize = m.per_suite.values().map(|s| s.total).sum();
            let cost_per_task = if total_tasks > 0 {
                m.estimated_cost / total_tasks as f64
            } else {
                0.0
            };
            let effectiveness = report
                .cost_analysis
                .cost_effectiveness
                .get(&m.info.name)
                .copied()
                .unwrap_or(0.0);
            let eff_str = if effectiveness.is_infinite() {
                "inf (free)".into()
            } else {
                format!("{:.1}", effectiveness)
            };
            md.push_str(&format!(
                "| {} | ${:.4} | ${:.6} | {} |\n",
                m.info.name, m.estimated_cost, cost_per_task, eff_str,
            ));
        }
        md.push('\n');

        if let Some(ref cheapest) = report.cost_analysis.cheapest_acceptable {
            md.push_str(&format!(
                "**Cheapest acceptable model (>80% pass rate):** {}\n\n",
                cheapest
            ));
        }

        // Recommendations
        md.push_str("## Recommendations\n\n");
        md.push_str("| Scenario | Model | Reasoning |\n");
        md.push_str("|----------|-------|-----------|\n");

        for rec in &report.recommendations {
            md.push_str(&format!(
                "| {} | {} | {} |\n",
                rec.scenario, rec.recommended_model, rec.reasoning,
            ));
        }

        md
    }

    /// Generate JSON string for the benchmark report.
    pub fn to_json(report: &BenchmarkReport) -> String {
        serde_json::to_string_pretty(report)
            .unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
    }

    /// Load a ComparisonReport from a JSON file (comparison.json format).
    pub fn load_comparison_json(
        path: &std::path::Path,
    ) -> anyhow::Result<(Vec<ModelInfo>, Vec<crate::runner::EvalReport>)> {
        let content = std::fs::read_to_string(path)?;
        let entries: Vec<ComparisonJsonEntry> = serde_json::from_str(&content)?;

        let mut models = Vec::new();
        let mut reports = Vec::new();

        for entry in entries {
            models.push(entry.model.clone());
            let report = crate::runner::EvalReport {
                model: Some(entry.model),
                results: vec![], // Task-level results not stored in comparison JSON
                total: entry.summary.total,
                passed: entry.summary.passed,
                pass_rate: entry.summary.pass_rate,
                avg_score: entry.summary.avg_score,
                total_tokens: entry.token_usage.total,
                total_duration_ms: entry.latency.total_ms,
            };
            reports.push(report);
        }

        Ok((models, reports))
    }
}

/// JSON entry for deserialization (mirrors comparison.rs ComparisonJsonEntry)
#[derive(Deserialize)]
struct ComparisonJsonEntry {
    model: ModelInfo,
    summary: crate::reporter::ReportSummary,
    #[allow(dead_code)]
    by_category: HashMap<String, crate::reporter::CategoryStats>,
    #[allow(dead_code)]
    by_difficulty: HashMap<String, crate::reporter::CategoryStats>,
    latency: crate::reporter::LatencyStats,
    token_usage: crate::reporter::TokenUsageStats,
    #[allow(dead_code)]
    task_results: Vec<crate::reporter::TaskResultSummary>,
    #[allow(dead_code)]
    estimated_cost_usd: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ModelTier;
    use crate::runner::EvalReport;

    fn make_report(passed: usize, total: usize, tokens: u64) -> EvalReport {
        EvalReport {
            model: None,
            results: vec![],
            total,
            passed,
            pass_rate: if total > 0 {
                passed as f64 / total as f64
            } else {
                0.0
            },
            avg_score: if total > 0 {
                passed as f64 / total as f64
            } else {
                0.0
            },
            total_tokens: tokens,
            total_duration_ms: 1000,
        }
    }

    fn make_model(name: &str, tier: ModelTier, cost_in: f64, cost_out: f64) -> ModelInfo {
        ModelInfo {
            name: name.into(),
            model_id: name.to_lowercase().replace(' ', "-"),
            provider: "test".into(),
            tier,
            cost_per_1m_input: cost_in,
            cost_per_1m_output: cost_out,
        }
    }

    #[test]
    fn test_aggregate_empty() {
        let report = BenchmarkAggregator::aggregate(vec![]);
        assert!(report.models.is_empty());
        assert!(report.recommendations.is_empty());
    }

    #[test]
    fn test_aggregate_single_suite() {
        let model_a = make_model("ModelA", ModelTier::Economy, 0.15, 0.75);
        let model_b = make_model("ModelB", ModelTier::Flagship, 3.0, 15.0);

        let report_a = make_report(8, 10, 5000);
        let report_b = make_report(10, 10, 8000);

        let comparison = ComparisonReport {
            model_reports: vec![
                (model_a.clone(), report_a),
                (model_b.clone(), report_b),
            ],
        };

        let benchmark = BenchmarkAggregator::aggregate(vec![("tool_call", &comparison)]);

        assert_eq!(benchmark.models.len(), 2);
        assert_eq!(benchmark.models[0].info.name, "ModelA");
        assert!((benchmark.models[0].overall_pass_rate - 0.8).abs() < 0.01);
        assert_eq!(benchmark.models[1].info.name, "ModelB");
        assert!((benchmark.models[1].overall_pass_rate - 1.0).abs() < 0.01);

        // Dimension matrix
        assert!(benchmark.dimension_matrix.contains_key("ModelA"));
        assert!(
            (benchmark.dimension_matrix["ModelA"]["tool_call"] - 0.8).abs() < 0.01
        );
    }

    #[test]
    fn test_aggregate_multiple_suites() {
        let model_a = make_model("ModelA", ModelTier::Economy, 0.15, 0.75);

        let tc_report = make_report(7, 10, 3000);
        let sec_report = make_report(9, 10, 4000);

        let tc_comparison = ComparisonReport {
            model_reports: vec![(model_a.clone(), tc_report)],
        };
        let sec_comparison = ComparisonReport {
            model_reports: vec![(model_a.clone(), sec_report)],
        };

        let benchmark = BenchmarkAggregator::aggregate(vec![
            ("tool_call", &tc_comparison),
            ("security", &sec_comparison),
        ]);

        assert_eq!(benchmark.models.len(), 1);
        // Overall: 16/20 = 80%
        assert!((benchmark.models[0].overall_pass_rate - 0.8).abs() < 0.01);
        assert_eq!(benchmark.models[0].per_suite.len(), 2);
    }

    #[test]
    fn test_recommendations_generated() {
        let model_cheap = make_model("Cheap", ModelTier::Economy, 0.0, 0.0);
        let model_exp = make_model("Expensive", ModelTier::Flagship, 3.0, 15.0);

        let cheap_report = make_report(8, 10, 5000);
        let exp_report = make_report(10, 10, 8000);

        let comparison = ComparisonReport {
            model_reports: vec![
                (model_cheap, cheap_report),
                (model_exp, exp_report),
            ],
        };

        let benchmark = BenchmarkAggregator::aggregate(vec![("tool_call", &comparison)]);

        assert!(!benchmark.recommendations.is_empty());
        let scenarios: Vec<&str> = benchmark
            .recommendations
            .iter()
            .map(|r| r.scenario.as_str())
            .collect();
        assert!(scenarios.contains(&"cost_sensitive"));
        assert!(scenarios.contains(&"balanced"));
        assert!(scenarios.contains(&"performance_first"));
    }

    #[test]
    fn test_markdown_generation() {
        let model = make_model("TestModel", ModelTier::Standard, 0.3, 1.2);
        let report = make_report(9, 10, 6000);

        let comparison = ComparisonReport {
            model_reports: vec![(model, report)],
        };

        let benchmark = BenchmarkAggregator::aggregate(vec![("tool_call", &comparison)]);
        let md = BenchmarkAggregator::to_markdown(&benchmark);

        assert!(md.contains("# Octo Agent Benchmark Report"));
        assert!(md.contains("TestModel"));
        assert!(md.contains("Recommendations"));
    }

    #[test]
    fn test_json_generation() {
        let model = make_model("TestModel", ModelTier::Standard, 0.3, 1.2);
        let report = make_report(9, 10, 6000);

        let comparison = ComparisonReport {
            model_reports: vec![(model, report)],
        };

        let benchmark = BenchmarkAggregator::aggregate(vec![("tool_call", &comparison)]);
        let json = BenchmarkAggregator::to_json(&benchmark);

        assert!(json.contains("TestModel"));
        assert!(json.contains("cost_analysis"));
        assert!(json.contains("recommendations"));
    }

    #[test]
    fn test_cost_analysis() {
        let model_free = make_model("Free", ModelTier::Free, 0.0, 0.0);
        let model_paid = make_model("Paid", ModelTier::Standard, 0.3, 1.2);

        let free_report = make_report(9, 10, 5000);
        let paid_report = make_report(10, 10, 8000);

        let comparison = ComparisonReport {
            model_reports: vec![
                (model_free, free_report),
                (model_paid, paid_report),
            ],
        };

        let benchmark = BenchmarkAggregator::aggregate(vec![("tool_call", &comparison)]);

        // Free model with 90% should be cheapest acceptable (>80%)
        assert_eq!(
            benchmark.cost_analysis.cheapest_acceptable,
            Some("Free".into())
        );
    }
}
