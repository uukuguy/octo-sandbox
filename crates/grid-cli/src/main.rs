//! Grid CLI - Local CLI for interacting with Grid agents
//!
//! This is the lightweight CLI binary (`grid`).
//! For the full-screen TUI + Dashboard, use `grid-studio` (requires "studio" feature).

use anyhow::Result;
use clap::Parser;
use tracing::info;
use tracing_subscriber::{fmt, EnvFilter};

use grid_cli::commands::{
    self, execute_ask, execute_init, execute_run, generate_completions, handle_agent, handle_config,
    handle_eval, handle_mcp, handle_memory, handle_root, handle_sandbox, handle_session,
    handle_skill, handle_tools, run_doctor, AppState, CompletionsCommands,
};
use grid_cli::output;
use grid_cli::{Cli, Commands};

fn init_logging(verbose: bool) {
    let filter = if verbose {
        EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("grid_cli=info,grid_engine=debug,grid_eval=debug"))
    } else {
        EnvFilter::new("grid_cli=warn,grid_engine=warn,grid_eval=warn")
    };

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(true)
        .with_line_number(true)
        .init();
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let cli = Cli::parse();
    init_logging(cli.verbose);

    info!("Starting Grid CLI");

    let grid_root = if let Some(ref project_path) = cli.project {
        grid_engine::GridRoot::with_project_dir(project_path)
            .unwrap_or_else(|e| {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            })
    } else {
        grid_engine::GridRoot::discover()
            .unwrap_or_else(|e| {
                tracing::warn!("Failed to discover GridRoot: {}, using defaults", e);
                let wd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
                grid_engine::GridRoot::with_working_dir(&wd).expect("GridRoot fallback failed")
            })
    };

    if let Err(e) = grid_root.ensure_dirs() {
        tracing::warn!("Failed to ensure GridRoot directories: {}", e);
    }

    let db_path = cli.db.unwrap_or_else(|| {
        grid_root.resolve_db_path().to_string_lossy().to_string()
    });

    let output_config = output::OutputConfig {
        format: match cli.output.as_str() {
            "json" => output::OutputFormat::Json,
            "stream-json" => output::OutputFormat::StreamJson,
            _ => output::OutputFormat::Text,
        },
        color: !cli.no_color && std::io::IsTerminal::is_terminal(&std::io::stdout()),
        quiet: cli.quiet,
    };

    let state = AppState::with_grid_root(db_path.into(), output_config, grid_root).await?;

    match cli.command {
        Commands::Run {
            resume,
            session,
            agent,
            theme,
            add_dirs,
            dual,
        } => {
            execute_run(
                commands::run::RunOptions {
                    resume,
                    session_id: session,
                    agent_id: agent,
                    theme,
                    add_dirs,
                    dual,
                },
                &state,
            )
            .await?;
        }
        Commands::Ask {
            message,
            session,
            agent,
        } => {
            execute_ask(
                commands::ask::AskOptions {
                    message,
                    session_id: session,
                    agent_id: agent,
                },
                &state,
            )
            .await?;
        }
        Commands::Agent { action } => handle_agent(action, &state).await?,
        Commands::Session { action } => handle_session(action, &state).await?,
        Commands::Memory { action } => handle_memory(action, &state).await?,
        Commands::Tool { action } => handle_tools(action, &state).await?,
        Commands::Mcp { action } => handle_mcp(action, &state).await?,
        Commands::Config { action } => handle_config(action, &state).await?,
        Commands::Auth { action } => commands::handle_auth(action, &state).await?,
        Commands::Init => execute_init(&state).await?,
        Commands::Doctor { repair } => run_doctor(repair, &state).await?,
        Commands::Completions { action } => match action {
            CompletionsCommands::Generate { shell } => generate_completions(shell)?,
        },
        Commands::Skill { action } => handle_skill(action, &state).await?,
        Commands::Root { action } => handle_root(action, &state).await?,
        Commands::Sandbox { action } => handle_sandbox(action, &state).await?,
        Commands::Eval { action } => handle_eval(action, &state).await?,
    }

    Ok(())
}
