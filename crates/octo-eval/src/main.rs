//! octo-eval CLI — run evaluation suites and multi-model comparisons.
//!
//! Usage:
//!   cargo run -p octo-eval -- list-suites
//!   cargo run -p octo-eval -- run --suite tool_call
//!   cargo run -p octo-eval -- compare --suite tool_call

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;

use octo_eval::benchmark::BenchmarkAggregator;
use octo_eval::comparison::{ComparisonReport, ComparisonRunner};
use octo_eval::config::{EvalConfig, EvalTomlConfig, EngineConfig, MultiModelConfig, ModelEntry};
use octo_eval::model::{ModelInfo, ModelTier};
use octo_eval::reporter::Reporter;
use octo_eval::runner::EvalRunner;
use octo_eval::suites::context::ContextSuite;
use octo_eval::suites::e2e::E2eSuite;
use octo_eval::suites::memory::MemorySuite;
use octo_eval::suites::output_format::OutputFormatSuite;
use octo_eval::suites::provider::ProviderSuite;
use octo_eval::suites::reasoning::ReasoningSuite;
use octo_eval::suites::security::SecuritySuite;
use octo_eval::suites::tool_boundary::ToolBoundarySuite;
use octo_eval::benchmarks::BenchmarkRegistry;
use octo_eval::suites::platform_security::PlatformSecuritySuite;
use octo_eval::suites::provider_resilience::ProviderResilienceSuite;
use octo_eval::suites::resilience::ResilienceSuite;
use octo_eval::suites::tool_call::ToolCallSuite;
use octo_eval::task::EvalTask;

fn main() -> Result<()> {
    // Load .env from project root (walk up from crate dir)
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt::init();

    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).map(|s| s.as_str()).unwrap_or("help");

    match command {
        "list-suites" => cmd_list_suites(),
        "run" => cmd_run(&args[2..]),
        "compare" => cmd_compare(&args[2..]),
        "benchmark" => cmd_benchmark(&args[2..]),
        "help" | "--help" | "-h" => cmd_help(),
        _ => {
            eprintln!("Unknown command: {command}");
            let _ = cmd_help();
            std::process::exit(1);
        }
    }
}

fn cmd_help() -> Result<()> {
    println!("octo-eval — Agent Evaluation Harness\n");
    println!("USAGE:");
    println!("  cargo run -p octo-eval -- <COMMAND> [OPTIONS]\n");
    println!("COMMANDS:");
    println!("  list-suites              List available evaluation suites");
    println!("  run --suite <NAME>       Run a single-model evaluation");
    println!("  compare --suite <NAME>   Run multi-model comparison");
    println!("  benchmark                Aggregate multi-suite comparisons into a unified report");
    println!("  help                     Show this help\n");
    println!("OPTIONS:");
    println!("  --suite <NAME>           Suite name: tool_call, security, context, output_format, tool_boundary, reasoning, resilience, provider, memory, e2e, gaia, swe_bench, tau_bench");
    println!("  --suites <A,B,C>         Comma-separated suite list (for benchmark command)");
    println!("  --input <DIR>            Input directory with comparison.json files (for benchmark command)");
    println!("  --output <DIR>           Output directory (default: eval_output)");
    println!("  --format <FMT>           Output format: json, markdown, both (default: both)");
    println!("  --baseline <PATH>        Baseline report JSON for regression detection");
    println!("  --config <PATH>          Config file path (default: eval.toml)");
    println!("  --replay <DIR>           Replay mode: use saved traces (zero LLM cost)");
    println!("  --target <MODE>          Target mode: engine (default), cli, or server");
    println!("  --binary <PATH>          CLI binary path (default: target/debug/octo-cli)");
    println!("  --server-url <URL>       Server base URL (default: http://127.0.0.1:3001)");
    println!("  --tag <NAME>            Tag this run for future reference");
    Ok(())
}

fn cmd_list_suites() -> Result<()> {
    println!("Available evaluation suites:\n");
    println!("  Agent Loop Suites (require LLM provider):");
    println!("    tool_call   — Tool calling accuracy (23 tasks, L1-L4)");
    println!("    security    — Security policy enforcement (14 tasks, S1-S4)");
    println!("    context     — Output quality & error handling (6 tasks, CX1-CX3)");
    println!("    output_format — Structured output format verification (JSON, YAML, CSV, Markdown)");
    println!("    tool_boundary — Tool boundary awareness and creative tool use");
    println!("    reasoning     — Reasoning, planning, and task decomposition (LlmJudge)");
    println!();
    println!("    resilience    — Resilience: retry, e-stop, canary detection, error recovery (20 tasks)");
    println!("    platform_security — Platform security: SecurityPolicy modes, autonomy, path traversal (15 tasks)");
    println!("    provider_resilience — Provider resilience: failover, rate limit, timeout health (12 tasks)");
    println!();
    println!("  Direct API Suites (mock-based, no LLM required):");
    println!("    provider    — Provider fault tolerance & failover (10 tests)");
    println!("    memory      — Memory system consistency across 4 layers (12 tests)");
    println!("    e2e         — End-to-end bug-fix verification (8 fixtures)");
    println!();
    println!("  External Dataset Suites:");
    println!("    bfcl        — Berkeley Function Calling Leaderboard simple subset (10 tasks)");
    println!();
    println!("  External Benchmarks:");
    let registry = BenchmarkRegistry::with_defaults();
    for bm in registry.list() {
        let sandbox_note = if bm.requires_sandbox() {
            if bm.sandbox_available() {
                " [sandbox: available]"
            } else {
                " [sandbox: unavailable — mock mode]"
            }
        } else {
            ""
        };
        println!("    {:12} — {}{}", bm.name(), bm.description(), sandbox_note);
    }
    Ok(())
}

fn load_suite(name: &str) -> Result<Vec<Box<dyn EvalTask>>> {
    match name {
        "tool_call" => ToolCallSuite::load(),
        "security" => SecuritySuite::load(),
        "context" => ContextSuite::load(),
        "output_format" => OutputFormatSuite::load(),
        "tool_boundary" => ToolBoundarySuite::load(),
        "reasoning" => ReasoningSuite::load(),
        "resilience" => ResilienceSuite::load(),
        "platform_security" => PlatformSecuritySuite::load(),
        "provider_resilience" => ProviderResilienceSuite::load(),
        "bfcl" => {
            let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let path = manifest_dir.join("datasets/bfcl_simple.jsonl");
            octo_eval::datasets::bfcl::load_bfcl_as_tasks(&path)
        }
        // External benchmarks — delegate to BenchmarkRegistry
        name if BenchmarkRegistry::with_defaults().contains(name) => {
            let registry = BenchmarkRegistry::with_defaults();
            let bm = registry.get(name).unwrap();
            bm.load_tasks()
        }
        _ => anyhow::bail!("Unknown suite: {name}. Use 'list-suites' to see available suites."),
    }
}

struct CliArgs {
    suite: String,
    suites: Option<String>,
    input: Option<PathBuf>,
    output: PathBuf,
    output_explicit: bool,
    format: String,
    baseline: Option<PathBuf>,
    config_path: Option<PathBuf>,
    replay: Option<PathBuf>,
    target: String,
    binary: Option<PathBuf>,
    server_url: Option<String>,
    tag: Option<String>,
}

fn parse_args(args: &[String]) -> CliArgs {
    let mut suite = "tool_call".to_string();
    let mut suites: Option<String> = None;
    let mut input: Option<PathBuf> = None;
    let mut output = PathBuf::from("eval_output");
    let mut output_explicit = false;
    let mut format = "both".to_string();
    let mut baseline = None;
    let mut config_path = None;
    let mut replay = None;
    let mut target = "engine".to_string();
    let mut binary = None;
    let mut server_url = None;
    let mut tag = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--suite" => {
                if i + 1 < args.len() {
                    suite = args[i + 1].clone();
                    i += 1;
                }
            }
            "--output" => {
                if i + 1 < args.len() {
                    output = PathBuf::from(&args[i + 1]);
                    output_explicit = true;
                    i += 1;
                }
            }
            "--format" => {
                if i + 1 < args.len() {
                    format = args[i + 1].clone();
                    i += 1;
                }
            }
            "--baseline" => {
                if i + 1 < args.len() {
                    baseline = Some(PathBuf::from(&args[i + 1]));
                    i += 1;
                }
            }
            "--config" => {
                if i + 1 < args.len() {
                    config_path = Some(PathBuf::from(&args[i + 1]));
                    i += 1;
                }
            }
            "--replay" => {
                if i + 1 < args.len() {
                    replay = Some(PathBuf::from(&args[i + 1]));
                    i += 1;
                }
            }
            "--target" => {
                if i + 1 < args.len() {
                    target = args[i + 1].clone();
                    i += 1;
                }
            }
            "--binary" => {
                if i + 1 < args.len() {
                    binary = Some(PathBuf::from(&args[i + 1]));
                    i += 1;
                }
            }
            "--server-url" => {
                if i + 1 < args.len() {
                    server_url = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "--suites" => {
                if i + 1 < args.len() {
                    suites = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "--input" => {
                if i + 1 < args.len() {
                    input = Some(PathBuf::from(&args[i + 1]));
                    i += 1;
                }
            }
            "--tag" => {
                if i + 1 < args.len() {
                    tag = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    CliArgs {
        suite,
        suites,
        input,
        output,
        output_explicit,
        format,
        baseline,
        config_path,
        replay,
        target,
        binary,
        server_url,
        tag,
    }
}

fn cmd_run(args: &[String]) -> Result<()> {
    let cli = parse_args(args);

    // Direct API suites — run their own runner, no LLM required
    match cli.suite.as_str() {
        "provider" | "memory" | "e2e" => return cmd_run_direct_suite(&cli),
        _ => {}
    }

    let tasks = load_suite(&cli.suite)?;

    println!("Running suite '{}' ({} tasks)...\n", cli.suite, tasks.len());

    // Layer config: code defaults < eval.toml < CLI args
    let toml_path = cli.config_path.clone().unwrap_or_else(|| PathBuf::from("eval.toml"));
    let toml_config = EvalTomlConfig::load(&toml_path)?;

    let mut config = EvalConfig::default();
    // Layer 1: Apply TOML defaults (overrides code defaults)
    if let Some(ref tc) = toml_config {
        tc.apply_to_eval_config(&mut config);
        // If TOML defines models, use the first model's engine config for single-model run
        let model_entries = tc.to_model_entries();
        if let Some(first) = model_entries.first() {
            config.target = octo_eval::config::EvalTarget::Engine(first.engine.clone());
        }
    }
    // Layer 2: CLI overrides (highest priority)
    if cli.output_explicit {
        config.output_dir = cli.output.clone();
    }

    // Apply target mode
    match cli.target.as_str() {
        "cli" => {
            let binary_path = cli
                .binary
                .clone()
                .unwrap_or_else(|| PathBuf::from("target/debug/octo-cli"));
            config.target = octo_eval::config::EvalTarget::Cli(octo_eval::config::CliConfig {
                binary_path,
                ..Default::default()
            });
        }
        "server" => {
            let base_url = cli
                .server_url
                .clone()
                .unwrap_or_else(|| "http://127.0.0.1:3001".to_string());
            config.target =
                octo_eval::config::EvalTarget::Server(octo_eval::config::ServerConfig {
                    base_url,
                    ..Default::default()
                });
        }
        _ => {} // "engine" — default
    }

    // Extract model name before config is moved into async block
    let run_model_name = match &config.target {
        octo_eval::config::EvalTarget::Engine(e) => e.model.clone(),
        _ => cli.target.clone(),
    };

    let rt = tokio::runtime::Runtime::new()?;
    let report = rt.block_on(async {
        if let Some(ref replay_dir) = cli.replay {
            run_with_replay(&config, &tasks, replay_dir).await
        } else {
            let runner = EvalRunner::new(config)?;
            runner.run_suite(&tasks).await
        }
    })?;

    println!("Results: {}/{} passed ({:.1}%)\n", report.passed, report.total, report.pass_rate * 100.0);

    let (categories, difficulties) = build_metadata(&tasks);
    let detailed = Reporter::generate(&report, &categories, &difficulties);

    output_report(&cli, &detailed)?;

    // Save to RunStore
    match save_to_run_store("run", &cli.suite, &[run_model_name], &detailed, None, cli.tag.as_deref()) {
        Ok(run_id) => println!("Run saved: {} (eval_output/runs/{})", run_id, run_id),
        Err(e) => eprintln!("Warning: Failed to save run: {}", e),
    }

    Ok(())
}

/// Run direct API suites (provider, memory, e2e) — no LLM provider needed
fn cmd_run_direct_suite(cli: &CliArgs) -> Result<()> {
    println!("Running direct API suite '{}'...\n", cli.suite);

    let rt = tokio::runtime::Runtime::new()?;
    let report = rt.block_on(async {
        match cli.suite.as_str() {
            "provider" => ProviderSuite::run().await,
            "memory" => MemorySuite::run().await,
            "e2e" => E2eSuite::run().await,
            _ => unreachable!(),
        }
    })?;

    println!(
        "Results: {}/{} passed ({:.1}%)\n",
        report.passed, report.total, report.pass_rate * 100.0
    );

    // Generate report with empty metadata (direct suites don't use JSONL metadata)
    let categories = HashMap::new();
    let difficulties = HashMap::new();
    let detailed = Reporter::generate(&report, &categories, &difficulties);

    output_report(cli, &detailed)?;
    Ok(())
}

/// Write report files and handle regression detection
fn output_report(cli: &CliArgs, detailed: &octo_eval::reporter::DetailedReport) -> Result<()> {
    std::fs::create_dir_all(&cli.output)?;

    if cli.format == "json" || cli.format == "both" {
        let json = Reporter::to_json(detailed);
        let path = cli.output.join("report.json");
        std::fs::write(&path, &json)?;
        println!("JSON report: {}", path.display());
    }

    if cli.format == "markdown" || cli.format == "both" {
        let md = Reporter::to_markdown(detailed);
        let path = cli.output.join("report.md");
        std::fs::write(&path, &md)?;
        println!("Markdown report: {}", path.display());
    }

    // Regression detection against baseline
    if let Some(baseline_path) = &cli.baseline {
        let baseline_content = std::fs::read_to_string(baseline_path)?;
        let baseline: octo_eval::reporter::DetailedReport =
            serde_json::from_str(&baseline_content)?;
        let regression = Reporter::diff_report(detailed, &baseline);
        let regression_md = Reporter::regression_to_markdown(&regression);

        let regression_path = cli.output.join("regression.md");
        std::fs::write(&regression_path, &regression_md)?;
        println!("Regression report: {}", regression_path.display());

        let regression_json_path = cli.output.join("regression.json");
        let regression_json = serde_json::to_string_pretty(&regression)?;
        std::fs::write(&regression_json_path, &regression_json)?;
        println!("Regression JSON: {}", regression_json_path.display());

        // Print summary
        let delta = regression.current_pass_rate - regression.baseline_pass_rate;
        let arrow = if delta > 0.0 { "▲" } else if delta < 0.0 { "▼" } else { "=" };
        println!(
            "\nRegression: {:.1}% → {:.1}% ({}{:+.1}%) | {} improved, {} regressed, {} unchanged",
            regression.baseline_pass_rate * 100.0,
            regression.current_pass_rate * 100.0,
            arrow,
            delta * 100.0,
            regression.improved,
            regression.regressed,
            regression.unchanged,
        );
    }

    Ok(())
}

/// Run evaluation in replay mode using saved traces (zero LLM cost).
async fn run_with_replay(
    config: &EvalConfig,
    tasks: &[Box<dyn EvalTask>],
    replay_dir: &std::path::Path,
) -> Result<octo_eval::runner::EvalReport> {
    use octo_eval::mock_provider::ReplayProvider;
    use octo_eval::recorder::EvalRecorder;

    // Try to load summary JSONL first, then try individual trace files
    let summary_path = replay_dir.join("eval_traces.jsonl");
    let traces = if summary_path.exists() {
        EvalRecorder::load_summary(&summary_path)?
    } else {
        // Load individual trace files from the directory
        let mut traces = Vec::new();
        for entry in std::fs::read_dir(replay_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "json") {
                if let Ok(trace) = EvalRecorder::load_trace(&path) {
                    traces.push(trace);
                }
            }
        }
        traces
    };

    if traces.is_empty() {
        eprintln!(
            "WARNING: No traces found in {}. Falling back to normal provider.",
            replay_dir.display()
        );
        let runner = EvalRunner::new(config.clone())?;
        return runner.run_suite(tasks).await;
    }

    eprintln!(
        "Replay mode: loaded {} traces from {}",
        traces.len(),
        replay_dir.display()
    );

    // Extract all interactions from traces for the ReplayProvider
    let mut all_interactions = Vec::new();
    for trace in &traces {
        let interactions = EvalRecorder::extract_interactions(trace);
        all_interactions.extend(interactions);
    }

    let replay_provider = Arc::new(ReplayProvider::new(all_interactions));
    let runner = EvalRunner::with_provider(config.clone(), replay_provider);
    runner.run_suite(tasks).await
}

fn cmd_compare(args: &[String]) -> Result<()> {
    let cli = parse_args(args);
    let suite_name = &cli.suite;
    let output_dir = &cli.output;
    let format = &cli.format;
    let tasks = load_suite(suite_name)?;

    // Load TOML config for model definitions
    let toml_path = cli.config_path.clone().unwrap_or_else(|| PathBuf::from("eval.toml"));
    let toml_config = EvalTomlConfig::load(&toml_path)?;

    // Load model configurations: EVAL_MODEL_* env vars > TOML models > auto-detect
    let mut models = load_models_from_env();
    if models.is_empty() {
        if let Some(ref tc) = toml_config {
            models = tc.to_model_entries();
        }
    }
    if models.is_empty() {
        models = auto_detect_models();
    }
    if models.is_empty() {
        println!("No models configured. Using mock models for demonstration.\n");
        return run_mock_comparison(&tasks, &output_dir, &format, &suite_name);
    }

    println!(
        "Comparing {} models on suite '{}' ({} tasks)...\n",
        models.len(),
        suite_name,
        tasks.len()
    );

    let config = MultiModelConfig {
        models,
        output_dir: output_dir.clone(),
        ..MultiModelConfig::default()
    };

    let rt = tokio::runtime::Runtime::new()?;
    let report = rt.block_on(async {
        let runner = ComparisonRunner::new(config);
        runner.run_comparison(&tasks).await
    })?;

    output_comparison(&report, &tasks, &output_dir, &format)?;

    // Save to RunStore — build a summary report from comparison
    let model_names: Vec<String> = report.model_reports.iter().map(|r| r.0.name.clone()).collect();
    let (categories, difficulties) = build_metadata(&tasks);
    // Use first model's report as a representative
    if let Some((_, first_report)) = report.model_reports.first() {
        let detailed = Reporter::generate(first_report, &categories, &difficulties);
        let comparison_json = serde_json::from_str::<serde_json::Value>(
            &report.to_json(&categories, &difficulties),
        )
        .ok();
        match save_to_run_store(
            "compare",
            suite_name,
            &model_names,
            &detailed,
            comparison_json.as_ref(),
            cli.tag.as_deref(),
        ) {
            Ok(run_id) => println!("Run saved: {} (eval_output/runs/{})", run_id, run_id),
            Err(e) => eprintln!("Warning: Failed to save run: {}", e),
        }
    }

    Ok(())
}

fn auto_detect_models() -> Vec<ModelEntry> {
    let api_key = std::env::var("OPENAI_API_KEY").ok();
    let base_url = std::env::var("OPENAI_BASE_URL").ok();

    if api_key.is_none() {
        return vec![];
    }

    let models = vec![
        ("DeepSeek-V3", "deepseek/deepseek-chat-v3-0324", ModelTier::Economy, 0.30, 0.88),
        ("Qwen3-30B", "qwen/qwen3-30b-a3b", ModelTier::Economy, 0.15, 0.60),
        ("Qwen3.5-122B", "qwen/qwen3.5-122b-a10b", ModelTier::Standard, 0.30, 1.20),
    ];

    models
        .into_iter()
        .map(|(name, model_id, tier, cost_in, cost_out)| ModelEntry {
            engine: EngineConfig {
                provider_name: "openai".into(),
                api_key: api_key.clone(),
                base_url: base_url.clone(),
                model: model_id.into(),
                ..EngineConfig::default()
            },
            info: ModelInfo {
                name: name.into(),
                model_id: model_id.into(),
                provider: "openrouter".into(),
                tier,
                cost_per_1m_input: cost_in,
                cost_per_1m_output: cost_out,
            },
        })
        .collect()
}

fn load_models_from_env() -> Vec<ModelEntry> {
    let mut models = Vec::new();

    // Check for EVAL_MODEL_* environment variables
    // Format: EVAL_MODEL_1_NAME, EVAL_MODEL_1_PROVIDER, EVAL_MODEL_1_MODEL, EVAL_MODEL_1_KEY
    for i in 1..=10 {
        let prefix = format!("EVAL_MODEL_{}", i);
        let name = std::env::var(format!("{}_NAME", prefix)).ok();
        let provider = std::env::var(format!("{}_PROVIDER", prefix)).ok();
        let model_id = std::env::var(format!("{}_MODEL", prefix)).ok();
        let api_key = std::env::var(format!("{}_KEY", prefix)).ok();
        let base_url = std::env::var(format!("{}_BASE_URL", prefix)).ok();

        if let (Some(name), Some(provider), Some(model_id)) = (name, provider, model_id) {
            let tier = std::env::var(format!("{}_TIER", prefix))
                .ok()
                .and_then(|t| match t.to_lowercase().as_str() {
                    "free" | "t0" => Some(ModelTier::Free),
                    "economy" | "t1" => Some(ModelTier::Economy),
                    "standard" | "t2" => Some(ModelTier::Standard),
                    "high" | "t3" => Some(ModelTier::HighPerformance),
                    "flagship" | "t4" => Some(ModelTier::Flagship),
                    "top" | "t5" => Some(ModelTier::TopTier),
                    _ => None,
                })
                .unwrap_or(ModelTier::Standard);

            models.push(ModelEntry {
                engine: EngineConfig {
                    provider_name: provider.clone(),
                    api_key,
                    base_url,
                    model: model_id.clone(),
                    ..EngineConfig::default()
                },
                info: ModelInfo {
                    name,
                    model_id,
                    provider,
                    tier,
                    cost_per_1m_input: 0.0,
                    cost_per_1m_output: 0.0,
                },
            });
        }
    }

    models
}

fn run_mock_comparison(
    tasks: &[Box<dyn EvalTask>],
    output_dir: &PathBuf,
    format: &str,
    suite_name: &str,
) -> Result<()> {
    use octo_eval::mock_provider::MockProvider;

    println!("Running mock comparison demo with 2 simulated models...\n");

    let model_a = ModelInfo {
        name: "MockEconomy".into(),
        model_id: "mock-economy".into(),
        provider: "mock".into(),
        tier: ModelTier::Economy,
        cost_per_1m_input: 0.15,
        cost_per_1m_output: 0.75,
    };

    let model_b = ModelInfo {
        name: "MockFlagship".into(),
        model_id: "mock-flagship".into(),
        provider: "mock".into(),
        tier: ModelTier::Flagship,
        cost_per_1m_input: 3.0,
        cost_per_1m_output: 15.0,
    };

    let provider_a = Arc::new(MockProvider::with_text("mock response economy"));
    let provider_b = Arc::new(MockProvider::with_text("mock response flagship"));

    let config = MultiModelConfig {
        output_dir: output_dir.clone(),
        ..MultiModelConfig::default()
    };

    let rt = tokio::runtime::Runtime::new()?;
    let report = rt.block_on(async {
        ComparisonRunner::run_comparison_with_providers(
            vec![
                (model_a, provider_a),
                (model_b, provider_b),
            ],
            tasks,
            &config,
        )
        .await
    })?;

    println!(
        "Compared {} models on suite '{}'\n",
        report.model_count(),
        suite_name
    );

    output_comparison(&report, tasks, output_dir, format)?;
    Ok(())
}

fn output_comparison(
    report: &ComparisonReport,
    tasks: &[Box<dyn EvalTask>],
    output_dir: &PathBuf,
    format: &str,
) -> Result<()> {
    let (categories, difficulties) = build_metadata(tasks);

    std::fs::create_dir_all(output_dir)?;

    if format == "json" || format == "both" {
        let json = report.to_json(&categories, &difficulties);
        let path = output_dir.join("comparison.json");
        std::fs::write(&path, &json)?;
        println!("JSON comparison: {}", path.display());
    }

    if format == "markdown" || format == "both" {
        let md = report.to_markdown(&categories, &difficulties);
        let path = output_dir.join("comparison.md");
        std::fs::write(&path, &md)?;
        println!("Markdown comparison: {}", path.display());

        // Print summary to stdout
        println!("\n{}", md);
    }

    Ok(())
}

fn cmd_benchmark(args: &[String]) -> Result<()> {
    let cli = parse_args(args);

    // Mode 1: Aggregate from existing comparison.json files in --input directory
    if let Some(ref input_dir) = cli.input {
        return cmd_benchmark_from_files(input_dir, &cli);
    }

    // Mode 2: Run all suites and aggregate
    let suite_list = cli
        .suites
        .as_deref()
        .unwrap_or("tool_call,security,bfcl,context,resilience,reasoning");
    let suite_names: Vec<&str> = suite_list.split(',').map(|s| s.trim()).collect();

    // Load TOML config for model definitions
    let toml_path = cli
        .config_path
        .clone()
        .unwrap_or_else(|| PathBuf::from("eval.benchmark.toml"));
    let toml_config = EvalTomlConfig::load(&toml_path)?;

    let mut models = load_models_from_env();
    if models.is_empty() {
        if let Some(ref tc) = toml_config {
            models = tc.to_model_entries();
        }
    }
    if models.is_empty() {
        models = auto_detect_models();
    }
    if models.is_empty() {
        anyhow::bail!(
            "No models configured. Set OPENAI_API_KEY or provide --config with model definitions."
        );
    }

    println!(
        "Benchmark: {} models x {} suites\n",
        models.len(),
        suite_names.len()
    );

    let mut suite_comparisons: Vec<(String, ComparisonReport)> = Vec::new();

    for suite_name in &suite_names {
        let tasks = match load_suite(suite_name) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("WARNING: Skipping suite '{}': {}", suite_name, e);
                continue;
            }
        };

        println!(
            "=== Suite: {} ({} tasks) ===\n",
            suite_name,
            tasks.len()
        );

        let suite_output = cli.output.join(suite_name);
        let config = MultiModelConfig {
            models: models.clone(),
            output_dir: suite_output.clone(),
            ..MultiModelConfig::default()
        };

        let rt = tokio::runtime::Runtime::new()?;
        let report = rt.block_on(async {
            let runner = ComparisonRunner::new(config);
            runner.run_comparison(&tasks).await
        })?;

        // Save per-suite comparison
        output_comparison(&report, &tasks, &suite_output, &cli.format)?;

        suite_comparisons.push((suite_name.to_string(), report));
    }

    // Aggregate all suite reports
    let suite_refs: Vec<(&str, &ComparisonReport)> = suite_comparisons
        .iter()
        .map(|(name, report)| (name.as_str(), report))
        .collect();

    let benchmark = BenchmarkAggregator::aggregate(suite_refs);
    output_benchmark(&benchmark, &cli)?;

    // Save to RunStore — use aggregated data
    let model_names: Vec<String> = benchmark.models.iter().map(|m| m.info.name.clone()).collect();
    let summary_report = octo_eval::reporter::DetailedReport {
        summary: octo_eval::reporter::ReportSummary {
            total: benchmark
                .models
                .first()
                .map(|m| m.per_suite.values().map(|s| s.total).sum())
                .unwrap_or(0),
            passed: benchmark
                .models
                .first()
                .map(|m| m.per_suite.values().map(|s| s.passed).sum())
                .unwrap_or(0),
            failed: 0,
            pass_rate: benchmark
                .models
                .first()
                .map(|m| m.overall_pass_rate)
                .unwrap_or(0.0),
            avg_score: benchmark
                .models
                .first()
                .map(|m| m.overall_avg_score)
                .unwrap_or(0.0),
        },
        by_category: HashMap::new(),
        by_difficulty: HashMap::new(),
        latency: octo_eval::reporter::LatencyStats::default(),
        token_usage: octo_eval::reporter::TokenUsageStats::default(),
        task_results: vec![],
    };
    match save_to_run_store(
        "benchmark",
        suite_list,
        &model_names,
        &summary_report,
        None,
        cli.tag.as_deref(),
    ) {
        Ok(run_id) => println!("Run saved: {} (eval_output/runs/{})", run_id, run_id),
        Err(e) => eprintln!("Warning: Failed to save run: {}", e),
    }

    Ok(())
}

fn cmd_benchmark_from_files(input_dir: &PathBuf, cli: &CliArgs) -> Result<()> {
    println!("Aggregating benchmark from {}\n", input_dir.display());

    let mut suite_comparisons: Vec<(String, ComparisonReport)> = Vec::new();

    // Scan subdirectories for comparison.json files
    if input_dir.is_dir() {
        let mut entries: Vec<_> = std::fs::read_dir(input_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let json_path = entry.path().join("comparison.json");
            if json_path.exists() {
                let suite_name = entry
                    .file_name()
                    .to_string_lossy()
                    .to_string();

                match BenchmarkAggregator::load_comparison_json(&json_path) {
                    Ok((models, reports)) => {
                        let model_reports: Vec<_> =
                            models.into_iter().zip(reports.into_iter()).collect();
                        let comparison = ComparisonReport { model_reports };
                        println!("  Loaded: {} ({} models)", suite_name, comparison.model_count());
                        suite_comparisons.push((suite_name, comparison));
                    }
                    Err(e) => {
                        eprintln!("  WARNING: Failed to load {}: {}", json_path.display(), e);
                    }
                }
            }
        }
    }

    if suite_comparisons.is_empty() {
        anyhow::bail!("No comparison.json files found in {}", input_dir.display());
    }

    let suite_refs: Vec<(&str, &ComparisonReport)> = suite_comparisons
        .iter()
        .map(|(name, report)| (name.as_str(), report))
        .collect();

    let benchmark = BenchmarkAggregator::aggregate(suite_refs);
    output_benchmark(&benchmark, cli)?;

    Ok(())
}

fn output_benchmark(
    benchmark: &octo_eval::benchmark::BenchmarkReport,
    cli: &CliArgs,
) -> Result<()> {
    let output_dir = &cli.output;
    std::fs::create_dir_all(output_dir)?;

    if cli.format == "json" || cli.format == "both" {
        let json = BenchmarkAggregator::to_json(benchmark);
        let path = output_dir.join("benchmark.json");
        std::fs::write(&path, &json)?;
        println!("\nBenchmark JSON: {}", path.display());
    }

    if cli.format == "markdown" || cli.format == "both" {
        let md = BenchmarkAggregator::to_markdown(benchmark);
        let path = output_dir.join("benchmark.md");
        std::fs::write(&path, &md)?;
        println!("Benchmark report: {}", path.display());
        println!("\n{}", md);
    }

    Ok(())
}

/// Save evaluation results to the versioned RunStore.
fn save_to_run_store(
    command: &str,
    suite: &str,
    models: &[String],
    report: &octo_eval::reporter::DetailedReport,
    comparison: Option<&serde_json::Value>,
    tag: Option<&str>,
) -> Result<String> {
    use octo_eval::run_store::{RunData, RunManifest, RunStore};
    use std::process::Command;

    let store = RunStore::new(PathBuf::from("eval_output/runs"))?;
    let run_id = store.next_run_id();

    // Get git info
    let git_commit = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".into());
    let git_branch = Command::new("git")
        .args(["branch", "--show-current"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".into());

    // Compute config hash (simplified: hash the suite name + model list)
    let config_str = format!("{}:{}", suite, models.join(","));
    let config_hash = {
        let mut h: u64 = 0;
        for b in config_str.bytes() {
            h = h.wrapping_mul(31).wrapping_add(b as u64);
        }
        let hex = format!("{:x}", h);
        hex[..8.min(hex.len())].to_string()
    };

    let manifest = RunManifest {
        run_id: run_id.clone(),
        tag: tag.map(|s| s.to_string()),
        timestamp: chrono::Local::now().to_rfc3339(),
        command: command.to_string(),
        suite: suite.to_string(),
        models: models.to_vec(),
        git_commit,
        git_branch,
        task_count: report.summary.total,
        passed: report.summary.passed,
        pass_rate: report.summary.pass_rate,
        avg_score: report.summary.avg_score,
        duration_ms: report.latency.total_ms,
        total_tokens: report.token_usage.total,
        estimated_cost: 0.0,
        eval_config_hash: config_hash,
        failure_summary: octo_eval::benchmark::FailureSummary::default(),
    };

    let run_data = RunData {
        manifest,
        report: Some(report.clone()),
        comparison: comparison.cloned(),
        traces: vec![],
    };

    store.save_run(&run_data)?;
    store.update_latest_link(&run_id)?;

    Ok(run_id)
}

fn build_metadata(
    tasks: &[Box<dyn EvalTask>],
) -> (
    HashMap<String, String>,
    HashMap<String, octo_eval::task::Difficulty>,
) {
    let mut categories = HashMap::new();
    let mut difficulties = HashMap::new();

    for task in tasks {
        let meta = task.metadata();
        categories.insert(task.id().to_string(), meta.category);
        difficulties.insert(task.id().to_string(), meta.difficulty);
    }

    (categories, difficulties)
}
