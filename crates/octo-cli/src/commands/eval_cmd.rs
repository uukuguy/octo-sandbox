//! Handler for `octo eval` subcommands.

use std::path::PathBuf;

use anyhow::Result;

use crate::commands::state::AppState;
use crate::commands::types::EvalCommands;

/// Route eval subcommands to their handlers.
pub async fn handle_eval(action: EvalCommands, _state: &AppState) -> Result<()> {
    match action {
        EvalCommands::List => cmd_list(),
        EvalCommands::Config { path } => cmd_config(&path),
        EvalCommands::Run {
            suite,
            tag,
            config,
            target,
        } => cmd_run(&suite, tag.as_deref(), config.as_deref(), &target).await,
        EvalCommands::Compare { suite, tag, config } => {
            cmd_compare(&suite, tag.as_deref(), config.as_deref()).await
        }
        EvalCommands::Benchmark {
            suites,
            tag,
            config,
        } => cmd_benchmark(suites.as_deref(), tag.as_deref(), config.as_deref()).await,
        EvalCommands::Report { run_id, format } => cmd_report(&run_id, &format),
        EvalCommands::Trace {
            run_id,
            task_id,
            full,
        } => cmd_trace(&run_id, &task_id, full),
        EvalCommands::Diagnose { run_id, category } => {
            cmd_diagnose(&run_id, category.as_deref())
        }
        EvalCommands::Diff { run_a, run_b } => cmd_diff(&run_a, &run_b),
        EvalCommands::History {
            limit,
            suite,
            since,
        } => cmd_history(limit, suite.as_deref(), since.as_deref()),
        EvalCommands::Watch { suite, interval } => cmd_watch(&suite, interval).await,
    }
}

// -- G3: list + config -------------------------------------------------------

fn cmd_list() -> Result<()> {
    println!("Available evaluation suites:\n");
    println!("  Agent Loop Suites (require LLM provider):");
    println!("    tool_call          Tool calling accuracy (23 tasks, L1-L4)");
    println!("    security           Security policy enforcement (14 tasks, S1-S4)");
    println!("    context            Output quality & error handling (6 tasks, CX1-CX3)");
    println!("    output_format      Structured output format verification");
    println!("    tool_boundary      Tool boundary awareness and creative tool use");
    println!("    reasoning          Reasoning, planning, and task decomposition");
    println!("    resilience         Resilience: retry, e-stop, canary detection (20 tasks)");
    println!("    platform_security  Platform security: SecurityPolicy modes (15 tasks)");
    println!(
        "    provider_resilience Provider resilience: failover, rate limit (12 tasks)"
    );
    println!();
    println!("  Direct API Suites (mock-based, no LLM required):");
    println!("    provider           Provider fault tolerance & failover (10 tests)");
    println!("    memory             Memory system consistency (12 tests)");
    println!("    e2e                End-to-end bug-fix verification (8 fixtures)");
    println!();
    println!("  External Dataset Suites:");
    println!("    bfcl               Berkeley Function Calling Leaderboard (10 tasks)");
    println!();
    println!("  External Benchmarks:");
    println!("    gaia               GAIA multi-step reasoning (50 tasks)");
    println!("    swe_bench          SWE-bench code repair (50 tasks)");
    println!("    tau_bench          tau-bench retail domain (30 tasks)");
    Ok(())
}

fn cmd_config(path: &str) -> Result<()> {
    use octo_eval::config::EvalTomlConfig;

    let config_path = PathBuf::from(path);
    match EvalTomlConfig::load(&config_path)? {
        Some(config) => {
            println!("Evaluation config: {}\n", path);

            // Defaults
            println!("Defaults:");
            if let Some(t) = config.default.timeout_secs {
                println!("  timeout_secs: {}", t);
            }
            if let Some(c) = config.default.concurrency {
                println!("  concurrency: {}", c);
            }
            if let Some(r) = config.default.record_traces {
                println!("  record_traces: {}", r);
            }
            if let Some(ref d) = config.default.output_dir {
                println!("  output_dir: {}", d);
            }

            // Models
            if !config.models.is_empty() {
                println!("\nModels ({}):", config.models.len());
                for m in &config.models {
                    let tier = m.tier.as_deref().unwrap_or("standard");
                    println!("  {} ({}/{}) [{}]", m.name, m.provider, m.model, tier);
                }
            }

            println!("\nConfig is valid.");
        }
        None => {
            println!("Config file not found: {}", path);
            println!("Using default configuration. Create eval.toml to customize.");
        }
    }
    Ok(())
}

// -- G3: run + compare + benchmark --------------------------------------------

async fn cmd_run(
    suite: &str,
    tag: Option<&str>,
    _config: Option<&str>,
    target: &str,
) -> Result<()> {
    println!("Running evaluation suite '{}'...", suite);
    println!("Target: {}", target);
    if let Some(t) = tag {
        println!("Tag: {}", t);
    }

    // For now, print instructions to use octo-eval directly.
    // Full integration requires refactoring octo-eval::main into lib functions.
    println!("\nTo execute:");
    println!(
        "  cargo run -p octo-eval -- run --suite {} --target {}",
        suite, target
    );
    if let Some(t) = tag {
        println!("\nRun will be tagged as '{}'", t);
    }

    println!("\n(Direct execution will be available when octo-eval exposes lib API)");
    Ok(())
}

async fn cmd_compare(suite: &str, tag: Option<&str>, config: Option<&str>) -> Result<()> {
    println!("Running multi-model comparison on suite '{}'...", suite);
    if let Some(t) = tag {
        println!("Tag: {}", t);
    }
    if let Some(c) = config {
        println!("Config: {}", c);
    }

    println!("\nTo execute:");
    let mut cmd = format!("cargo run -p octo-eval -- compare --suite {}", suite);
    if let Some(c) = config {
        cmd.push_str(&format!(" --config {}", c));
    }
    println!("  {}", cmd);

    println!("\n(Direct execution will be available when octo-eval exposes lib API)");
    Ok(())
}

async fn cmd_benchmark(
    suites: Option<&str>,
    tag: Option<&str>,
    config: Option<&str>,
) -> Result<()> {
    let suites_str = suites.unwrap_or("tool_call,security,bfcl,context,resilience,reasoning");
    println!("Running benchmark across suites: {}", suites_str);
    if let Some(t) = tag {
        println!("Tag: {}", t);
    }
    if let Some(c) = config {
        println!("Config: {}", c);
    }

    println!("\nTo execute:");
    let mut cmd = format!(
        "cargo run -p octo-eval -- benchmark --suites {}",
        suites_str
    );
    if let Some(c) = config {
        cmd.push_str(&format!(" --config {}", c));
    }
    println!("  {}", cmd);

    println!("\n(Direct execution will be available when octo-eval exposes lib API)");
    Ok(())
}

// -- G4: history + report -----------------------------------------------------

fn cmd_history(limit: usize, suite: Option<&str>, since: Option<&str>) -> Result<()> {
    use octo_eval::run_store::{RunFilter, RunStore};

    let store = RunStore::new(PathBuf::from("eval_output/runs"))?;
    let filter = RunFilter {
        suite: suite.map(|s| s.to_string()),
        since: since.map(|s| s.to_string()),
        limit,
        tag: None,
    };

    let runs = store.list_runs(&filter)?;

    if runs.is_empty() {
        println!("No evaluation runs found.");
        if suite.is_some() || since.is_some() {
            println!("Try removing filters to see all runs.");
        }
        return Ok(());
    }

    // Table header
    println!(
        "{:<20} {:<16} {:>10} {:>6} {:>10} {:<16}",
        "Run ID", "Suite", "Pass Rate", "Tasks", "Duration", "Tag"
    );
    println!("{}", "-".repeat(84));

    for m in &runs {
        let tag_display = m.tag.as_deref().unwrap_or("-");
        let duration = if m.duration_ms >= 1000 {
            format!("{:.1}s", m.duration_ms as f64 / 1000.0)
        } else {
            format!("{}ms", m.duration_ms)
        };
        println!(
            "{:<20} {:<16} {:>9.1}% {:>6} {:>10} {:<16}",
            m.run_id, m.suite, m.pass_rate * 100.0, m.task_count, duration, tag_display,
        );
    }

    println!("\n{} runs shown.", runs.len());
    Ok(())
}

fn cmd_report(run_id: &str, format: &str) -> Result<()> {
    use octo_eval::run_store::RunStore;

    let store = RunStore::new(PathBuf::from("eval_output/runs"))?;
    let run = store.load_run(run_id)?;

    println!(
        "Run: {}  |  Suite: {}  |  {}/{} passed ({:.1}%)",
        run.manifest.run_id,
        run.manifest.suite,
        run.manifest.passed,
        run.manifest.task_count,
        run.manifest.pass_rate * 100.0,
    );
    println!(
        "Git: {} ({})",
        run.manifest.git_commit, run.manifest.git_branch
    );
    println!(
        "Duration: {:.1}s  |  Tokens: {}  |  Cost: ${:.4}",
        run.manifest.duration_ms as f64 / 1000.0,
        run.manifest.total_tokens,
        run.manifest.estimated_cost,
    );
    if let Some(ref tag) = run.manifest.tag {
        println!("Tag: {}", tag);
    }

    match format {
        "json" => {
            if let Some(ref report) = run.report {
                let json = serde_json::to_string_pretty(report)?;
                println!("\n{}", json);
            } else {
                println!("\nNo detailed report available for this run.");
            }
        }
        "markdown" => {
            if let Some(ref report) = run.report {
                let md = octo_eval::reporter::Reporter::to_markdown(report);
                println!("\n{}", md);
            } else {
                println!("\nNo detailed report available for this run.");
            }
        }
        _ => {
            // text format -- show task results table
            if let Some(ref report) = run.report {
                println!("\nTask Results:");
                println!(
                    "{:<16} {:>6} {:>8} {:>10} {:>8}",
                    "Task ID", "Status", "Score", "Duration", "Tokens"
                );
                println!("{}", "-".repeat(54));
                for tr in &report.task_results {
                    let status = if tr.passed { "PASS" } else { "FAIL" };
                    println!(
                        "{:<16} {:>6} {:>8.3} {:>9}ms {:>8}",
                        tr.task_id, status, tr.score, tr.duration_ms, tr.tokens
                    );
                }
            }
        }
    }

    Ok(())
}

// -- G4: trace + diagnose -----------------------------------------------------

fn cmd_trace(run_id: &str, task_id: &str, full: bool) -> Result<()> {
    use octo_eval::run_store::RunStore;
    use octo_eval::trace::TraceEvent;

    let store = RunStore::new(PathBuf::from("eval_output/runs"))?;
    let run = store.load_run(run_id)?;

    // Find the matching trace
    let trace = run.traces.iter().find(|t| t.task_id == task_id);
    let trace = match trace {
        Some(t) => t,
        None => {
            println!("Task '{}' not found in run '{}'.", task_id, run_id);
            if !run.traces.is_empty() {
                println!("\nAvailable tasks:");
                for t in &run.traces {
                    println!("  {}", t.task_id);
                }
            }
            return Ok(());
        }
    };

    let status = if trace.score.passed { "PASS" } else { "FAIL" };
    println!(
        "Task: {}  |  Score: {:.1}  |  {}  |  {}ms",
        trace.task_id, trace.score.score, status, trace.output.duration_ms
    );

    // Timeline
    if !trace.timeline.is_empty() {
        println!("\nTimeline:");
        for event in &trace.timeline {
            match event {
                TraceEvent::RoundStart {
                    round,
                    timestamp_ms,
                } => {
                    println!("  [{:>6}ms]  RoundStart    round={}", timestamp_ms, round);
                }
                TraceEvent::LlmCall {
                    round: _,
                    input_tokens,
                    output_tokens,
                    duration_ms,
                    model,
                } => {
                    println!(
                        "  [       ]  LlmCall       in={} out={} model={} {}ms",
                        input_tokens, output_tokens, model, duration_ms
                    );
                }
                TraceEvent::Thinking { round: _, content } => {
                    let snippet = if full {
                        content.clone()
                    } else {
                        content.chars().take(80).collect::<String>()
                    };
                    println!("  [       ]  Thinking      \"{}\"", snippet);
                }
                TraceEvent::ToolCall {
                    round: _,
                    tool_name,
                    input,
                    output,
                    success,
                    duration_ms,
                } => {
                    let status_str = if *success { "OK" } else { "FAIL" };
                    let input_str = if full {
                        serde_json::to_string(input).unwrap_or_default()
                    } else {
                        let s = serde_json::to_string(input).unwrap_or_default();
                        s.chars().take(60).collect()
                    };
                    let output_str = if full {
                        output.clone()
                    } else {
                        output.chars().take(60).collect()
                    };
                    println!(
                        "  [       ]  ToolCall      {} {{{}}} -> {} {}ms",
                        tool_name, input_str, status_str, duration_ms
                    );
                    if full {
                        println!("             Output: {}", output_str);
                    }
                }
                TraceEvent::Error {
                    round: _,
                    source,
                    message,
                } => {
                    println!("  [       ]  Error         [{}] {}", source, message);
                }
                TraceEvent::SecurityBlocked {
                    round: _,
                    tool,
                    risk_level,
                    reason,
                } => {
                    println!(
                        "  [       ]  SecurityBlock {} risk={} \"{}\"",
                        tool, risk_level, reason
                    );
                }
                TraceEvent::ContextDegraded {
                    round: _,
                    stage,
                    usage_pct,
                } => {
                    println!(
                        "  [       ]  CtxDegraded   stage={} usage={:.0}%",
                        stage, usage_pct
                    );
                }
                TraceEvent::BudgetSnapshot {
                    round: _,
                    input_used,
                    output_used,
                    limit,
                } => {
                    println!(
                        "  [       ]  Budget        in={} out={} limit={}",
                        input_used, output_used, limit
                    );
                }
                TraceEvent::LoopGuardVerdict {
                    round: _,
                    verdict,
                    reason,
                } => {
                    println!("  [       ]  LoopGuard     {} \"{}\"", verdict, reason);
                }
                TraceEvent::Completed {
                    rounds,
                    stop_reason,
                    total_duration_ms,
                } => {
                    println!(
                        "  [       ]  Completed     rounds={} stop={} {}ms",
                        rounds, stop_reason, total_duration_ms
                    );
                }
            }
        }
    }

    // Dimensions
    if !trace.score.dimensions.is_empty() {
        println!("\nDimensions:");
        let mut dims: Vec<_> = trace.score.dimensions.iter().collect();
        dims.sort_by_key(|(k, _)| *k);
        for (name, val) in dims {
            println!("  {}: {:.2}", name, val);
        }
    }

    // Failure class
    if let Some(ref fc) = trace.score.failure_class {
        println!("\nFailure: {}", fc);
    }

    Ok(())
}

fn cmd_diagnose(run_id: &str, category: Option<&str>) -> Result<()> {
    use std::collections::HashMap;
    use octo_eval::run_store::RunStore;

    let store = RunStore::new(PathBuf::from("eval_output/runs"))?;
    let run = store.load_run(run_id)?;

    println!(
        "Run: {}  |  Suite: {}  |  {}/{} passed ({:.1}%)",
        run.manifest.run_id,
        run.manifest.suite,
        run.manifest.passed,
        run.manifest.task_count,
        run.manifest.pass_rate * 100.0,
    );

    // Count failures from FailureSummary in manifest
    let summary = &run.manifest.failure_summary;

    if summary.total_classified == 0 && summary.total_unclassified == 0 {
        // Try to classify from traces if available
        let mut by_category: HashMap<&str, Vec<String>> = HashMap::new();
        let mut infra_count = 0usize;

        for trace in &run.traces {
            if !trace.score.passed {
                if let Some(ref fc) = trace.score.failure_class {
                    let cat = fc.category();
                    by_category
                        .entry(cat)
                        .or_default()
                        .push(format!("  {}: {}", trace.task_id, fc));
                    if cat == "infrastructure" {
                        infra_count += 1;
                    }
                }
            }
        }

        if by_category.is_empty() {
            println!("\nNo failure classification data available.");
            println!("Re-run evaluation with trace recording enabled.");
            return Ok(());
        }

        let categories = ["infrastructure", "harness", "capability"];
        let labels = [
            "Infrastructure (not model capability)",
            "Harness Issues (framework bugs)",
            "Capability Gaps (real model weaknesses)",
        ];

        for (cat, label) in categories.iter().zip(labels.iter()) {
            if let Some(filter_cat) = category {
                if *cat != filter_cat {
                    continue;
                }
            }
            println!("\n{}:", label);
            match by_category.get(cat) {
                Some(items) => {
                    for item in items {
                        println!("{}", item);
                    }
                }
                None => println!("  (none)"),
            }
        }

        let total_failed = run.manifest.task_count - run.manifest.passed;
        if infra_count > 0 && total_failed > 0 {
            let adjusted_total = run.manifest.task_count - infra_count;
            let adjusted_rate = if adjusted_total > 0 {
                run.manifest.passed as f64 / adjusted_total as f64
            } else {
                1.0
            };
            println!(
                "\nAdjusted pass rate: {:.1}% (excluding {} infra failures)",
                adjusted_rate * 100.0,
                infra_count
            );
        }
    } else {
        // Use pre-computed FailureSummary
        println!(
            "\nFailure classification ({} classified, {} unclassified):",
            summary.total_classified, summary.total_unclassified
        );

        println!("\nBy category:");
        let mut cats: Vec<_> = summary.by_category.iter().collect();
        cats.sort_by_key(|(k, _)| k.as_str());
        for (cat, count) in &cats {
            println!("  {}: {}", cat, count);
        }

        println!("\nBy class:");
        let mut classes: Vec<_> = summary.by_class.iter().collect();
        classes.sort_by_key(|(k, _)| k.as_str());
        for (class, count) in &classes {
            println!("  {}: {}", class, count);
        }
    }

    Ok(())
}

// -- G4: diff + watch ---------------------------------------------------------

fn cmd_diff(run_a: &str, run_b: &str) -> Result<()> {
    use octo_eval::run_store::RunStore;

    let store = RunStore::new(PathBuf::from("eval_output/runs"))?;
    let a = store.load_run(run_a)?;
    let b = store.load_run(run_b)?;

    println!("Comparing: {} -> {}\n", run_a, run_b);

    // Basic manifest comparison
    println!(
        "Pass rate: {:.1}% -> {:.1}% ({:+.1}%)",
        a.manifest.pass_rate * 100.0,
        b.manifest.pass_rate * 100.0,
        (b.manifest.pass_rate - a.manifest.pass_rate) * 100.0,
    );

    // If both have reports, do detailed diff
    if let (Some(ref report_a), Some(ref report_b)) = (&a.report, &b.report) {
        let regression = octo_eval::reporter::Reporter::diff_report(report_b, report_a);

        println!(
            "Improved: {}  |  Regressed: {}  |  Unchanged: {}",
            regression.improved, regression.regressed, regression.unchanged
        );

        if regression.new_tasks > 0 {
            println!("New tasks: {}", regression.new_tasks);
        }
        if regression.removed_tasks > 0 {
            println!("Removed tasks: {}", regression.removed_tasks);
        }

        // Show changed tasks
        let changes: Vec<_> = regression
            .task_diffs
            .iter()
            .filter(|d| d.status != octo_eval::reporter::DiffStatus::Unchanged)
            .collect();

        if !changes.is_empty() {
            println!("\nChanges:");
            for diff in changes {
                let status = match diff.status {
                    octo_eval::reporter::DiffStatus::Improved => "IMPROVED ",
                    octo_eval::reporter::DiffStatus::Regressed => "REGRESSED",
                    octo_eval::reporter::DiffStatus::New => "NEW      ",
                    octo_eval::reporter::DiffStatus::Removed => "REMOVED  ",
                    octo_eval::reporter::DiffStatus::Unchanged => continue,
                };
                let baseline = diff
                    .baseline_score
                    .map(|s| format!("{:.1}", s))
                    .unwrap_or_else(|| "-".into());
                let current = diff
                    .current_score
                    .map(|s| format!("{:.1}", s))
                    .unwrap_or_else(|| "-".into());
                println!(
                    "  {}  {:<16}  {} -> {}",
                    status, diff.task_id, baseline, current
                );
            }
        }
    } else {
        println!("\nDetailed diff unavailable -- one or both runs lack report.json.");
        println!(
            "Pass delta: {:+.1}%",
            (b.manifest.pass_rate - a.manifest.pass_rate) * 100.0
        );
    }

    Ok(())
}

async fn cmd_watch(suite: &str, interval: u64) -> Result<()> {
    println!("Watch mode: suite '{}', interval {}s", suite, interval);
    println!("Press Ctrl+C to stop.\n");

    println!("To execute:");
    println!(
        "  cargo run -p octo-eval -- run --suite {} (repeating every {}s)",
        suite, interval
    );
    println!("\n(Watch with live RunStore delta will be available when octo-eval exposes lib API)");

    Ok(())
}
