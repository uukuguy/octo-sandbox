//! `octo init` command — initialize project directory structure.

use crate::commands::AppState;
use anyhow::Result;
use std::path::Path;

/// Run `octo init` to set up project structure.
pub async fn execute_init(state: &AppState) -> Result<()> {
    let root = &state.grid_root;
    let project_root = root.project_root();

    println!("Initializing Octo project...\n");

    // 1. Ensure all GridRoot directories exist
    root.ensure_dirs()?;
    println!("  Created {}", project_root.display());

    // 2. Generate project config.yaml if not exists
    let project_config = root.project_config();
    if project_config.exists() {
        println!("  Exists  {}", project_config.display());
    } else {
        let template = generate_project_config_template();
        std::fs::write(&project_config, &template)?;
        println!("  Created {}", project_config.display());
    }

    // 3. Create config.local.yaml template if not exists
    let local_config = root.project_local_config();
    if local_config.exists() {
        println!("  Exists  {}", local_config.display());
    } else {
        let template = generate_local_config_template();
        std::fs::write(&local_config, &template)?;
        println!("  Created {}", local_config.display());
    }

    // 4. Ensure .grid/.gitignore includes config.local.yaml
    let gitignore_path = project_root.join(".gitignore");
    ensure_gitignore(&gitignore_path)?;

    // 5. Create global credentials template if not exists
    let cred_path = root.credentials_path();
    if cred_path.exists() {
        println!("  Exists  {}", cred_path.display());
    } else {
        if let Some(parent) = cred_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let template = generate_credentials_template();
        std::fs::write(&cred_path, &template)?;
        // Set restrictive permissions on credentials file
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&cred_path, std::fs::Permissions::from_mode(0o600))?;
        }
        println!("  Created {} (mode 600)", cred_path.display());
    }

    println!("\nProject initialized successfully!");
    println!("\nNext steps:");
    println!("  1. Edit {} to set project-level config", project_config.display());
    println!(
        "  2. Edit {} to set API keys",
        cred_path.display()
    );
    println!("  3. Run `octo config show` to verify configuration");

    Ok(())
}

fn generate_project_config_template() -> String {
    r#"# Octo Project Configuration
# This file is version-controlled. Do NOT put secrets here.
# Use ~/.grid/credentials.yaml for API keys.
# Use .grid/config.local.yaml for local overrides (git-ignored).

# Provider configuration
# provider:
#   name: anthropic        # anthropic or openai

# Server configuration (for octo-server)
# server:
#   port: 3001
#   host: 127.0.0.1

# Skills directories
# skills:
#   dirs: []

# Logging
# logging:
#   level: "grid_server=info,grid_engine=info"
"#
    .to_string()
}

fn generate_local_config_template() -> String {
    r#"# Octo Local Configuration (git-ignored)
# Put local development overrides here.
# This file is NOT version-controlled.

# Example: use debug logging locally
# logging:
#   level: "grid_engine=debug"

# Example: override server port
# server:
#   port: 4000
"#
    .to_string()
}

fn generate_credentials_template() -> String {
    r#"# Octo Credentials (mode 600 — do NOT commit this file)
# API keys for LLM providers.

providers:
  anthropic:
    api_key: ""
    # base_url: ""
  openai:
    api_key: ""
    # base_url: ""
"#
    .to_string()
}

fn ensure_gitignore(path: &Path) -> Result<()> {
    let entries = [
        "config.local.yaml",
        "*.db",
        "*.db-journal",
    ];

    if path.exists() {
        let content = std::fs::read_to_string(path)?;
        let mut additions = Vec::new();
        for entry in &entries {
            if !content.lines().any(|l| l.trim() == *entry) {
                additions.push(*entry);
            }
        }
        if !additions.is_empty() {
            let mut new_content = content;
            if !new_content.ends_with('\n') {
                new_content.push('\n');
            }
            for entry in &additions {
                new_content.push_str(entry);
                new_content.push('\n');
            }
            std::fs::write(path, new_content)?;
            println!("  Updated {} (+{})", path.display(), additions.join(", "));
        } else {
            println!("  Exists  {}", path.display());
        }
    } else {
        let content = format!(
            "# Octo project local files (do not commit)\n{}\n",
            entries.join("\n")
        );
        std::fs::write(path, content)?;
        println!("  Created {}", path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_config_template() {
        let t = generate_project_config_template();
        assert!(t.contains("provider"));
        assert!(!t.contains("api_key"));
    }

    #[test]
    fn test_local_config_template() {
        let t = generate_local_config_template();
        assert!(t.contains("git-ignored"));
        assert!(t.contains("logging"));
    }

    #[test]
    fn test_credentials_template() {
        let t = generate_credentials_template();
        assert!(t.contains("anthropic"));
        assert!(t.contains("openai"));
        assert!(t.contains("api_key"));
    }

    #[test]
    fn test_ensure_gitignore_creates_new() {
        let tmp = tempfile::tempdir().unwrap();
        let gi = tmp.path().join(".gitignore");
        ensure_gitignore(&gi).unwrap();
        let content = std::fs::read_to_string(&gi).unwrap();
        assert!(content.contains("config.local.yaml"));
        assert!(content.contains("*.db"));
    }

    #[test]
    fn test_ensure_gitignore_appends_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let gi = tmp.path().join(".gitignore");
        std::fs::write(&gi, "existing\n").unwrap();
        ensure_gitignore(&gi).unwrap();
        let content = std::fs::read_to_string(&gi).unwrap();
        assert!(content.contains("existing"));
        assert!(content.contains("config.local.yaml"));
    }

    #[test]
    fn test_ensure_gitignore_no_duplicates() {
        let tmp = tempfile::tempdir().unwrap();
        let gi = tmp.path().join(".gitignore");
        std::fs::write(&gi, "config.local.yaml\n*.db\n*.db-journal\n").unwrap();
        ensure_gitignore(&gi).unwrap();
        let content = std::fs::read_to_string(&gi).unwrap();
        let count = content.matches("config.local.yaml").count();
        assert_eq!(count, 1);
    }
}
