//! Report generator — produces JSON and Markdown evaluation reports.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::runner::{EvalReport, TaskResult};
use crate::task::Difficulty;

/// Detailed report with category and difficulty breakdowns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedReport {
    pub summary: ReportSummary,
    pub by_category: HashMap<String, CategoryStats>,
    pub by_difficulty: HashMap<String, CategoryStats>,
    pub latency: LatencyStats,
    pub token_usage: TokenUsageStats,
    pub task_results: Vec<TaskResultSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub pass_rate: f64,
    pub avg_score: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CategoryStats {
    pub total: usize,
    pub passed: usize,
    pub pass_rate: f64,
    pub avg_score: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LatencyStats {
    pub min_ms: u64,
    pub max_ms: u64,
    pub avg_ms: u64,
    pub p95_ms: u64,
    pub total_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsageStats {
    pub total_input: u64,
    pub total_output: u64,
    pub total: u64,
    pub avg_per_task: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResultSummary {
    pub task_id: String,
    pub passed: bool,
    pub score: f64,
    pub duration_ms: u64,
    pub tokens: u64,
}

/// Reporter generates reports from evaluation results.
pub struct Reporter;

impl Reporter {
    /// Generate a detailed report from eval results.
    ///
    /// `categories` maps task_id -> category string.
    /// `difficulties` maps task_id -> difficulty.
    pub fn generate(
        report: &EvalReport,
        categories: &HashMap<String, String>,
        difficulties: &HashMap<String, Difficulty>,
    ) -> DetailedReport {
        let summary = ReportSummary {
            total: report.total,
            passed: report.passed,
            failed: report.total - report.passed,
            pass_rate: report.pass_rate,
            avg_score: report.avg_score,
        };

        let by_category = build_breakdown(&report.results, |r| {
            categories
                .get(&r.task_id)
                .cloned()
                .unwrap_or_else(|| "uncategorized".into())
        });

        let by_difficulty = build_breakdown(&report.results, |r| {
            difficulties
                .get(&r.task_id)
                .map(|d| format!("{:?}", d))
                .unwrap_or_else(|| "Unknown".into())
        });

        let latency = compute_latency_stats(&report.results);
        let token_usage = compute_token_stats(&report.results);

        let task_results = report
            .results
            .iter()
            .map(|r| TaskResultSummary {
                task_id: r.task_id.clone(),
                passed: r.score.passed,
                score: r.score.score,
                duration_ms: r.duration_ms,
                tokens: r.output.input_tokens + r.output.output_tokens,
            })
            .collect();

        DetailedReport {
            summary,
            by_category,
            by_difficulty,
            latency,
            token_usage,
            task_results,
        }
    }

    /// Generate JSON report string.
    pub fn to_json(report: &DetailedReport) -> String {
        serde_json::to_string_pretty(report)
            .unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
    }

    /// Generate Markdown report string.
    pub fn to_markdown(report: &DetailedReport) -> String {
        let mut md = String::new();

        md.push_str("# Evaluation Report\n\n");

        // Summary
        md.push_str("## Summary\n\n");
        md.push_str("| Metric | Value |\n");
        md.push_str("|--------|-------|\n");
        md.push_str(&format!("| Total Tasks | {} |\n", report.summary.total));
        md.push_str(&format!("| Passed | {} |\n", report.summary.passed));
        md.push_str(&format!("| Failed | {} |\n", report.summary.failed));
        md.push_str(&format!(
            "| Pass Rate | {:.1}% |\n",
            report.summary.pass_rate * 100.0
        ));
        md.push_str(&format!(
            "| Avg Score | {:.3} |\n",
            report.summary.avg_score
        ));
        md.push('\n');

        // Category breakdown
        if !report.by_category.is_empty() {
            md.push_str("## By Category\n\n");
            md.push_str("| Category | Total | Passed | Pass Rate | Avg Score |\n");
            md.push_str("|----------|-------|--------|-----------|----------|\n");
            let mut cats: Vec<_> = report.by_category.iter().collect();
            cats.sort_by_key(|(k, _)| *k);
            for (cat, stats) in cats {
                md.push_str(&format!(
                    "| {} | {} | {} | {:.1}% | {:.3} |\n",
                    cat,
                    stats.total,
                    stats.passed,
                    stats.pass_rate * 100.0,
                    stats.avg_score
                ));
            }
            md.push('\n');
        }

        // Difficulty breakdown
        if !report.by_difficulty.is_empty() {
            md.push_str("## By Difficulty\n\n");
            md.push_str("| Difficulty | Total | Passed | Pass Rate | Avg Score |\n");
            md.push_str("|------------|-------|--------|-----------|----------|\n");
            let mut diffs: Vec<_> = report.by_difficulty.iter().collect();
            diffs.sort_by_key(|(k, _)| *k);
            for (diff, stats) in diffs {
                md.push_str(&format!(
                    "| {} | {} | {} | {:.1}% | {:.3} |\n",
                    diff,
                    stats.total,
                    stats.passed,
                    stats.pass_rate * 100.0,
                    stats.avg_score
                ));
            }
            md.push('\n');
        }

        // Latency
        md.push_str("## Latency\n\n");
        md.push_str("| Metric | Value |\n");
        md.push_str("|--------|-------|\n");
        md.push_str(&format!("| Min | {}ms |\n", report.latency.min_ms));
        md.push_str(&format!("| Max | {}ms |\n", report.latency.max_ms));
        md.push_str(&format!("| Avg | {}ms |\n", report.latency.avg_ms));
        md.push_str(&format!("| P95 | {}ms |\n", report.latency.p95_ms));
        md.push_str(&format!("| Total | {}ms |\n", report.latency.total_ms));
        md.push('\n');

        // Token usage
        md.push_str("## Token Usage\n\n");
        md.push_str("| Metric | Value |\n");
        md.push_str("|--------|-------|\n");
        md.push_str(&format!(
            "| Input Tokens | {} |\n",
            report.token_usage.total_input
        ));
        md.push_str(&format!(
            "| Output Tokens | {} |\n",
            report.token_usage.total_output
        ));
        md.push_str(&format!(
            "| Total Tokens | {} |\n",
            report.token_usage.total
        ));
        md.push_str(&format!(
            "| Avg per Task | {} |\n",
            report.token_usage.avg_per_task
        ));
        md.push('\n');

        // Task results table
        md.push_str("## Task Results\n\n");
        md.push_str("| Task ID | Passed | Score | Duration | Tokens |\n");
        md.push_str("|---------|--------|-------|----------|--------|\n");
        for tr in &report.task_results {
            let status = if tr.passed { "PASS" } else { "FAIL" };
            md.push_str(&format!(
                "| {} | {} | {:.3} | {}ms | {} |\n",
                tr.task_id, status, tr.score, tr.duration_ms, tr.tokens
            ));
        }

        md
    }
}

/// Build a category/difficulty breakdown from results using a key extractor.
fn build_breakdown<F>(results: &[TaskResult], key_fn: F) -> HashMap<String, CategoryStats>
where
    F: Fn(&TaskResult) -> String,
{
    let mut accum: HashMap<String, (usize, usize, f64)> = HashMap::new();
    for result in results {
        let key = key_fn(result);
        let entry = accum.entry(key).or_insert((0, 0, 0.0));
        entry.0 += 1;
        if result.score.passed {
            entry.1 += 1;
        }
        entry.2 += result.score.score;
    }
    accum
        .into_iter()
        .map(|(k, (total, passed, score_sum))| {
            (
                k,
                CategoryStats {
                    total,
                    passed,
                    pass_rate: if total > 0 {
                        passed as f64 / total as f64
                    } else {
                        0.0
                    },
                    avg_score: if total > 0 {
                        score_sum / total as f64
                    } else {
                        0.0
                    },
                },
            )
        })
        .collect()
}

fn compute_latency_stats(results: &[TaskResult]) -> LatencyStats {
    if results.is_empty() {
        return LatencyStats::default();
    }

    let mut durations: Vec<u64> = results.iter().map(|r| r.duration_ms).collect();
    durations.sort_unstable();

    let total: u64 = durations.iter().sum();
    let p95_idx = ((durations.len() as f64 * 0.95).ceil() as usize).min(durations.len()) - 1;

    LatencyStats {
        min_ms: durations[0],
        max_ms: *durations.last().unwrap(),
        avg_ms: total / durations.len() as u64,
        p95_ms: durations[p95_idx],
        total_ms: total,
    }
}

fn compute_token_stats(results: &[TaskResult]) -> TokenUsageStats {
    if results.is_empty() {
        return TokenUsageStats::default();
    }

    let total_input: u64 = results.iter().map(|r| r.output.input_tokens).sum();
    let total_output: u64 = results.iter().map(|r| r.output.output_tokens).sum();
    let total = total_input + total_output;

    TokenUsageStats {
        total_input,
        total_output,
        total,
        avg_per_task: total / results.len() as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::{EvalScore, ScoreDetails};
    use crate::task::AgentOutput;

    fn make_result(
        id: &str,
        passed: bool,
        score: f64,
        tokens: u64,
        duration: u64,
    ) -> TaskResult {
        TaskResult {
            task_id: id.into(),
            output: AgentOutput {
                input_tokens: tokens / 2,
                output_tokens: tokens / 2,
                ..AgentOutput::default()
            },
            score: EvalScore {
                passed,
                score,
                details: ScoreDetails::Custom {
                    message: "test".into(),
                },
            },
            duration_ms: duration,
        }
    }

    #[test]
    fn test_generate_report() {
        let results = vec![
            make_result("t1", true, 1.0, 100, 50),
            make_result("t2", false, 0.3, 200, 100),
            make_result("t3", true, 0.8, 150, 75),
        ];
        let report = EvalReport::from_results(results);

        let categories: HashMap<String, String> = [
            ("t1".into(), "tool_call".into()),
            ("t2".into(), "security".into()),
            ("t3".into(), "tool_call".into()),
        ]
        .into_iter()
        .collect();

        let difficulties: HashMap<String, Difficulty> = [
            ("t1".into(), Difficulty::Easy),
            ("t2".into(), Difficulty::Medium),
            ("t3".into(), Difficulty::Easy),
        ]
        .into_iter()
        .collect();

        let detailed = Reporter::generate(&report, &categories, &difficulties);

        assert_eq!(detailed.summary.total, 3);
        assert_eq!(detailed.summary.passed, 2);
        assert_eq!(detailed.summary.failed, 1);

        // Category check
        let tool_call = detailed.by_category.get("tool_call").unwrap();
        assert_eq!(tool_call.total, 2);
        assert_eq!(tool_call.passed, 2);

        let security = detailed.by_category.get("security").unwrap();
        assert_eq!(security.total, 1);
        assert_eq!(security.passed, 0);

        // Difficulty check
        let easy = detailed.by_difficulty.get("Easy").unwrap();
        assert_eq!(easy.total, 2);
        assert_eq!(easy.passed, 2);

        let medium = detailed.by_difficulty.get("Medium").unwrap();
        assert_eq!(medium.total, 1);
        assert_eq!(medium.passed, 0);

        // Latency
        assert_eq!(detailed.latency.min_ms, 50);
        assert_eq!(detailed.latency.max_ms, 100);
        assert_eq!(detailed.latency.total_ms, 225);

        // Token usage
        assert_eq!(detailed.token_usage.total, 450);
        assert_eq!(detailed.token_usage.avg_per_task, 150);
    }

    #[test]
    fn test_json_output() {
        let results = vec![make_result("t1", true, 1.0, 100, 50)];
        let report = EvalReport::from_results(results);
        let detailed = Reporter::generate(&report, &HashMap::new(), &HashMap::new());
        let json = Reporter::to_json(&detailed);
        assert!(json.contains("\"total\": 1"));
        assert!(json.contains("\"passed\": 1"));
    }

    #[test]
    fn test_markdown_output() {
        let results = vec![
            make_result("t1", true, 1.0, 100, 50),
            make_result("t2", false, 0.0, 200, 100),
        ];
        let report = EvalReport::from_results(results);
        let detailed = Reporter::generate(&report, &HashMap::new(), &HashMap::new());
        let md = Reporter::to_markdown(&detailed);

        assert!(md.contains("# Evaluation Report"));
        assert!(md.contains("| Total Tasks | 2 |"));
        assert!(md.contains("| Passed | 1 |"));
        assert!(md.contains("PASS"));
        assert!(md.contains("FAIL"));
    }

    #[test]
    fn test_empty_report() {
        let report = EvalReport::default();
        let detailed = Reporter::generate(&report, &HashMap::new(), &HashMap::new());
        assert_eq!(detailed.summary.total, 0);
        assert_eq!(detailed.latency.min_ms, 0);
        assert_eq!(detailed.token_usage.total, 0);
    }

    #[test]
    fn test_uncategorized_tasks() {
        let results = vec![make_result("t1", true, 0.9, 100, 50)];
        let report = EvalReport::from_results(results);
        let detailed = Reporter::generate(&report, &HashMap::new(), &HashMap::new());

        assert!(detailed.by_category.contains_key("uncategorized"));
        assert!(detailed.by_difficulty.contains_key("Unknown"));
    }

    #[test]
    fn test_p95_latency() {
        // With 20 items, p95 should be the 19th element (index 18)
        let results: Vec<TaskResult> = (1..=20)
            .map(|i| make_result(&format!("t{}", i), true, 1.0, 100, i * 10))
            .collect();
        let report = EvalReport::from_results(results);
        let detailed = Reporter::generate(&report, &HashMap::new(), &HashMap::new());
        assert_eq!(detailed.latency.p95_ms, 190);
    }
}
