//! Authentication and credential management — `octo auth login/status/logout`
//!
//! Stores API keys in `~/.grid/credentials.yaml` (YAML key-value pairs).
//! Keys are provider-specific env var names (e.g. `ANTHROPIC_API_KEY`).

use std::collections::BTreeMap;
use std::io::{self, BufRead};

use anyhow::{Context, Result};
use grid_engine::providers::defaults::resolve_api_key_env;

use super::types::AuthCommands;
use super::AppState;

/// Handle auth subcommands
pub async fn handle_auth(action: AuthCommands, state: &AppState) -> Result<()> {
    match action {
        AuthCommands::Login { provider, key } => login(&provider, key, state),
        AuthCommands::Status => status(state),
        AuthCommands::Logout { provider } => logout(&provider, state),
    }
}

/// Store an API key for a provider
fn login(provider: &str, key: Option<String>, state: &AppState) -> Result<()> {
    let env_key = resolve_api_key_env(provider).ok_or_else(|| {
        anyhow::anyhow!(
            "Unknown provider '{}'. Known providers: anthropic, openai, openrouter, deepseek, groq, etc.",
            provider
        )
    })?;

    let api_key = match key {
        Some(k) => k,
        None => {
            eprintln!("Enter API key for {} ({}):", provider, env_key);
            let mut line = String::new();
            io::stdin()
                .lock()
                .read_line(&mut line)
                .context("Failed to read API key from stdin")?;
            line.trim().to_string()
        }
    };

    if api_key.is_empty() {
        anyhow::bail!("API key cannot be empty");
    }

    let creds_path = state.grid_root.credentials_path();

    // Ensure parent directory exists
    if let Some(parent) = creds_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Load existing credentials
    let mut creds = load_credentials(&creds_path)?;
    creds.insert(env_key.to_string(), api_key);
    save_credentials(&creds_path, &creds)?;

    // Restrict file permissions (owner-only read/write)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&creds_path, std::fs::Permissions::from_mode(0o600))?;
    }

    println!("Saved {} credential to {}", provider, creds_path.display());
    println!("  Key: {} ({}...)", env_key, &"*".repeat(8));

    Ok(())
}

/// Show all stored credentials (masked)
fn status(state: &AppState) -> Result<()> {
    let creds_path = state.grid_root.credentials_path();

    println!("Credentials file: {}", creds_path.display());
    println!();

    if !creds_path.exists() {
        println!("  (no credentials stored)");
        return Ok(());
    }

    let creds = load_credentials(&creds_path)?;
    if creds.is_empty() {
        println!("  (no credentials stored)");
        return Ok(());
    }

    for (key, value) in &creds {
        let masked = if value.len() > 8 {
            format!("{}...{}", &value[..4], &value[value.len() - 4..])
        } else {
            "*".repeat(value.len())
        };
        println!("  {}: {}", key, masked);
    }

    // Also show which providers have env var keys set (not from file)
    println!();
    println!("Environment overrides (higher priority):");
    let env_keys = [
        "ANTHROPIC_API_KEY",
        "OPENAI_API_KEY",
        "OPENROUTER_API_KEY",
        "DEEPSEEK_API_KEY",
    ];
    let mut found_env = false;
    for key in &env_keys {
        if std::env::var(key).is_ok() {
            println!("  {} = (set in environment)", key);
            found_env = true;
        }
    }
    if !found_env {
        println!("  (none)");
    }

    Ok(())
}

/// Remove a stored credential
fn logout(provider: &str, state: &AppState) -> Result<()> {
    let env_key = resolve_api_key_env(provider).ok_or_else(|| {
        anyhow::anyhow!("Unknown provider '{}'. Use the env var name directly if needed.", provider)
    })?;

    let creds_path = state.grid_root.credentials_path();
    if !creds_path.exists() {
        println!("No credentials file found. Nothing to remove.");
        return Ok(());
    }

    let mut creds = load_credentials(&creds_path)?;
    if creds.remove(env_key).is_some() {
        save_credentials(&creds_path, &creds)?;
        println!("Removed {} ({}) from {}", provider, env_key, creds_path.display());
    } else {
        println!("No credential found for {} ({})", provider, env_key);
    }

    Ok(())
}

/// Load credentials from YAML file
fn load_credentials(path: &std::path::Path) -> Result<BTreeMap<String, String>> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let content = std::fs::read_to_string(path).context("Failed to read credentials file")?;
    let map: BTreeMap<String, String> =
        serde_yaml::from_str(&content).unwrap_or_default();
    Ok(map)
}

/// Save credentials to YAML file
fn save_credentials(path: &std::path::Path, creds: &BTreeMap<String, String>) -> Result<()> {
    let content = serde_yaml::to_string(creds).context("Failed to serialize credentials")?;
    std::fs::write(path, content).context("Failed to write credentials file")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_save_credentials() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "ANTHROPIC_API_KEY: sk-ant-test123").unwrap();
        let creds = load_credentials(f.path()).unwrap();
        assert_eq!(creds.get("ANTHROPIC_API_KEY").unwrap(), "sk-ant-test123");

        // Round-trip
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("creds.yaml");
        save_credentials(&out, &creds).unwrap();
        let loaded = load_credentials(&out).unwrap();
        assert_eq!(loaded, creds);
    }

    #[test]
    fn test_load_missing_file() {
        let creds = load_credentials(std::path::Path::new("/nonexistent/path")).unwrap();
        assert!(creds.is_empty());
    }
}
