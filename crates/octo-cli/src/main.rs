//! Octo CLI - Local CLI for interacting with Octo agents
//!
//! This CLI provides commands for managing agents, sessions, memories, and tools.

use anyhow::Result;
use clap::Parser;
use tracing::info;
use tracing_subscriber::{fmt, EnvFilter};

use octo_cli::commands::{
    self, execute_ask, execute_run, generate_completions, handle_agent, handle_config, handle_eval,
    handle_mcp, handle_memory, handle_session, handle_skill, handle_tools, run_doctor, AppState,
    CompletionsCommands,
};
use octo_cli::output;
use octo_cli::tui;
use octo_cli::{Cli, Commands};

fn init_logging(verbose: bool) {
    let filter = if verbose {
        // -v: respect RUST_LOG if set, otherwise use debug level
        EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("octo_cli=info,octo_engine=debug,octo_eval=debug"))
    } else {
        // Default: always quiet, ignore RUST_LOG from .env
        EnvFilter::new("octo_cli=warn,octo_engine=warn,octo_eval=warn")
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
    // Load .env file if present
    dotenvy::dotenv().ok();

    let cli = Cli::parse();
    init_logging(cli.verbose);

    info!("Starting Octo CLI");

    // Determine database path
    let db_path = cli.db.unwrap_or_else(|| {
        // Try to get from config or use default
        std::env::var("OCTO_DB_PATH").unwrap_or_else(|_| "octo.db".to_string())
    });

    // Build output config from CLI flags
    let output_config = output::OutputConfig {
        format: match cli.output.as_str() {
            "json" => output::OutputFormat::Json,
            "stream-json" => output::OutputFormat::StreamJson,
            _ => output::OutputFormat::Text,
        },
        color: !cli.no_color && std::io::IsTerminal::is_terminal(&std::io::stdout()),
        quiet: cli.quiet,
    };

    // Initialize app state
    let state = AppState::new(db_path.into(), output_config).await?;

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
        Commands::Tui { theme: _ } => {
            tui::run_tui_conversation(&state).await?;
        }
        Commands::Doctor { repair } => run_doctor(repair, &state).await?,
        Commands::Completions { action } => match action {
            CompletionsCommands::Generate { shell } => generate_completions(shell)?,
        },
        Commands::Dashboard { port, host, open, enable_tls, cert_path, key_path, require_auth, allowed_origins, generate_cert } => {
            let (cert_path, key_path, tls_enabled) = if generate_cert {
                let cert_dir = std::path::PathBuf::from("./data/certs");
                let (cp, kp) = commands::dashboard_cert::generate_self_signed_cert(&cert_dir)?;
                (Some(cp), Some(kp), true)
            } else {
                (cert_path, key_path, enable_tls)
            };

            let opts = commands::dashboard::DashboardOptions {
                port,
                host,
                open,
                tls_enabled,
                cert_path,
                key_path,
                require_auth,
                allowed_origins,
            };
            commands::dashboard::run_dashboard(&opts).await?;
        }
        Commands::Skill { action } => handle_skill(action, &state).await?,
        Commands::Eval { action } => handle_eval(action, &state).await?,
    }

    Ok(())
}
