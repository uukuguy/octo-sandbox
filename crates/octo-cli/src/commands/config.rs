//! Config commands implementation

use crate::commands::{AppState, ConfigCommands};
use anyhow::Result;

/// Handle config commands
pub async fn handle_config(action: ConfigCommands, _state: &AppState) -> Result<()> {
    match action {
        ConfigCommands::Show => show_config().await?,
        ConfigCommands::Validate => validate_config().await?,
    }
    Ok(())
}

/// Show current configuration
async fn show_config() -> Result<()> {
    println!("Current configuration:");

    // Show environment variables
    println!("\nEnvironment Variables:");
    println!(
        "  OCTO_DB_PATH: {}",
        std::env::var("OCTO_DB_PATH").unwrap_or_else(|_| "not set".to_string())
    );
    println!(
        "  ANTHROPIC_API_KEY: {}",
        if std::env::var("ANTHROPIC_API_KEY").is_ok() {
            "***set***"
        } else {
            "not set"
        }
    );
    println!(
        "  OPENAI_API_KEY: {}",
        if std::env::var("OPENAI_API_KEY").is_ok() {
            "***set***"
        } else {
            "not set"
        }
    );
    println!(
        "  LLM_PROVIDER: {}",
        std::env::var("LLM_PROVIDER").unwrap_or_else(|_| "anthropic (default)".to_string())
    );
    println!(
        "  RUST_LOG: {}",
        std::env::var("RUST_LOG").unwrap_or_else(|_| "not set".to_string())
    );

    // Show CLI defaults
    println!("\nDefault Paths:");
    println!("  Database: octo.db (or $OCTO_DB_PATH)");
    println!("  Config: config.yaml");

    Ok(())
}

/// Validate configuration
async fn validate_config() -> Result<()> {
    println!("Validating configuration...");

    let mut has_errors = false;

    // Check required env vars
    let provider = std::env::var("LLM_PROVIDER").unwrap_or_else(|_| "anthropic".to_string());

    match provider.as_str() {
        "anthropic" => {
            if std::env::var("ANTHROPIC_API_KEY").is_err() {
                println!("ERROR: ANTHROPIC_API_KEY is not set");
                has_errors = true;
            }
        }
        "openai" => {
            if std::env::var("OPENAI_API_KEY").is_err() {
                println!("ERROR: OPENAI_API_KEY is not set");
                has_errors = true;
            }
        }
        _ => {
            println!("WARNING: Unknown LLM_PROVIDER: {}", provider);
        }
    }

    if has_errors {
        println!("\nConfiguration validation FAILED");
    } else {
        println!("\nConfiguration validation OK");
    }

    Ok(())
}
