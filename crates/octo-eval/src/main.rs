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

use octo_eval::comparison::{ComparisonReport, ComparisonRunner};
use octo_eval::config::{EvalConfig, EvalTarget, EngineConfig, MultiModelConfig, ModelEntry};
use octo_eval::model::{ModelInfo, ModelTier};
use octo_eval::reporter::Reporter;
use octo_eval::runner::EvalRunner;
use octo_eval::suites::context::ContextSuite;
use octo_eval::suites::security::SecuritySuite;
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
    println!("  help                     Show this help\n");
    println!("OPTIONS:");
    println!("  --suite <NAME>           Suite name: tool_call, security, context");
    println!("  --output <DIR>           Output directory (default: eval_output)");
    println!("  --format <FMT>           Output format: json, markdown, both (default: both)");
    Ok(())
}

fn cmd_list_suites() -> Result<()> {
    println!("Available evaluation suites:\n");
    println!("  tool_call   — Tool calling accuracy (23 tasks, L1-L4)");
    println!("  security    — Security policy enforcement (14 tasks, S1-S4)");
    println!("  context     — Output quality & error handling (6 tasks, CX1-CX3)");
    println!("\nDatasets (JSONL):");
    println!("  datasets/octo_tool_call.jsonl");
    println!("  datasets/octo_security.jsonl");
    println!("  datasets/octo_context.jsonl");
    Ok(())
}

fn load_suite(name: &str) -> Result<Vec<Box<dyn EvalTask>>> {
    match name {
        "tool_call" => ToolCallSuite::load(),
        "security" => SecuritySuite::load(),
        "context" => ContextSuite::load(),
        _ => anyhow::bail!("Unknown suite: {name}. Use 'list-suites' to see available suites."),
    }
}

fn parse_args(args: &[String]) -> (String, PathBuf, String) {
    let mut suite = "tool_call".to_string();
    let mut output = PathBuf::from("eval_output");
    let mut format = "both".to_string();

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
                    i += 1;
                }
            }
            "--format" => {
                if i + 1 < args.len() {
                    format = args[i + 1].clone();
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    (suite, output, format)
}

fn cmd_run(args: &[String]) -> Result<()> {
    let (suite_name, output_dir, format) = parse_args(args);
    let tasks = load_suite(&suite_name)?;

    println!("Running suite '{}' ({} tasks)...\n", suite_name, tasks.len());

    let config = EvalConfig {
        target: EvalTarget::Engine(EngineConfig::default()),
        output_dir: output_dir.clone(),
        ..EvalConfig::default()
    };

    let rt = tokio::runtime::Runtime::new()?;
    let report = rt.block_on(async {
        let runner = EvalRunner::new(config)?;
        runner.run_suite(&tasks).await
    })?;

    println!("Results: {}/{} passed ({:.1}%)\n", report.passed, report.total, report.pass_rate * 100.0);

    let (categories, difficulties) = build_metadata(&tasks);
    let detailed = Reporter::generate(&report, &categories, &difficulties);

    std::fs::create_dir_all(&output_dir)?;

    if format == "json" || format == "both" {
        let json = Reporter::to_json(&detailed);
        let path = output_dir.join("report.json");
        std::fs::write(&path, &json)?;
        println!("JSON report: {}", path.display());
    }

    if format == "markdown" || format == "both" {
        let md = Reporter::to_markdown(&detailed);
        let path = output_dir.join("report.md");
        std::fs::write(&path, &md)?;
        println!("Markdown report: {}", path.display());
    }

    Ok(())
}

fn cmd_compare(args: &[String]) -> Result<()> {
    let (suite_name, output_dir, format) = parse_args(args);
    let tasks = load_suite(&suite_name)?;

    // Load model configurations: EVAL_MODEL_* env vars, or auto-detect from .env
    let mut models = load_models_from_env();
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
