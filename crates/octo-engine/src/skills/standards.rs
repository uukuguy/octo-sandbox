//! Agent Skills standard validation module.
//!
//! This module provides validation functions for Agent Skills standard directory
//! structure and allowed-tools field.

use std::path::Path;

use anyhow::{bail, Result};

/// Standard directories that may exist in a skill package.
/// These are optional but follow the Agent Skills convention.
pub const STANDARD_DIRS: &[&str] = &["scripts", "references", "assets"];

/// Validate the skill directory structure according to Agent Skills standard.
///
/// # Requirements
/// - SKILL.md must exist (required)
/// - Optional standard directories: scripts, references, assets
///
/// # Arguments
/// * `dir` - Path to the skill directory
///
/// # Returns
/// * `Ok(())` if the structure is valid
/// * `Err` if validation fails
pub fn validate_skill_structure(dir: &Path) -> Result<()> {
    // SKILL.md is required
    let skill_file = dir.join("SKILL.md");
    if !skill_file.exists() {
        bail!("SKILL.md not found in {}", dir.display());
    }

    if !skill_file.is_file() {
        bail!("SKILL.md is not a file in {}", dir.display());
    }

    // Validate optional standard directories exist and are actually directories
    for std_dir in STANDARD_DIRS {
        let dir_path = dir.join(std_dir);
        if dir_path.exists() && !dir_path.is_dir() {
            bail!(
                "{} exists but is not a directory in {}",
                std_dir,
                dir.display()
            );
        }
    }

    Ok(())
}

/// Validate the allowed-tools field values.
///
/// Tool names should follow a specific format:
/// - Must start with a lowercase letter or underscore
/// - Can contain lowercase letters, digits, hyphens, and underscores
/// - Should be descriptive and meaningful
///
/// # Arguments
/// * `tools` - List of tool names to validate
///
/// # Returns
/// * `Ok(())` if all tool names are valid
/// * `Err` if any tool name is invalid
pub fn validate_allowed_tools(tools: &[String]) -> Result<()> {
    if tools.is_empty() {
        bail!("allowed-tools cannot be empty");
    }

    for tool in tools {
        validate_tool_name(tool)?;
    }

    Ok(())
}

/// Validate a single tool name format.
///
/// Valid tool names:
/// - Start with a lowercase letter (a-z) or underscore
/// - Contain only lowercase letters (a-z), digits (0-9), hyphens (-), and underscores (_)
/// - Have at least 1 character
/// - May end with `*` as a wildcard suffix (e.g., `mcp__myserver__*`)
///
/// MCP tool names use double underscores as separators: `mcp__server__tool`
///
/// # Arguments
/// * `name` - Tool name to validate
///
/// # Returns
/// * `Ok(())` if the tool name is valid
/// * `Err` if the tool name is invalid
pub fn validate_tool_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("tool name cannot be empty");
    }

    // Handle global wildcard
    if name == "*" {
        return Ok(());
    }

    // Strip trailing wildcard for validation of the prefix part
    let check_name = name.strip_suffix('*').unwrap_or(name);
    if check_name.is_empty() {
        // Already handled "*" above; this shouldn't happen
        return Ok(());
    }

    let mut chars = check_name.chars();

    // First character must be lowercase letter or underscore
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() || c == '_' => {}
        Some(c) => {
            bail!(
                "tool name '{}' must start with a lowercase letter or underscore, got '{}'",
                name,
                c
            );
        }
        None => {
            bail!("tool name cannot be empty");
        }
    }

    // Remaining characters can be lowercase letters, digits, hyphens, or underscores
    for c in chars {
        if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '-' && c != '_' {
            bail!(
                "tool name '{}' contains invalid character '{}'. Only lowercase letters, digits, hyphens, underscores, and trailing '*' wildcard are allowed",
                name,
                c
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Create a valid skill directory structure.
    fn create_valid_skill_dir(temp_dir: &TempDir, name: &str) -> std::path::PathBuf {
        let skill_dir = temp_dir.path().join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();

        let content = r#"---
name: test-skill
description: A test skill
---

# Test Skill
This is a test skill.
"#;
        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();

        skill_dir
    }

    /// Create a skill directory with optional standard directories.
    fn create_skill_with_standard_dirs(temp_dir: &TempDir, name: &str) -> std::path::PathBuf {
        let skill_dir = temp_dir.path().join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::create_dir_all(skill_dir.join("scripts")).unwrap();
        std::fs::create_dir_all(skill_dir.join("references")).unwrap();
        std::fs::create_dir_all(skill_dir.join("assets")).unwrap();

        let content = r#"---
name: test-skill
description: A test skill with standard dirs
---

# Test Skill
This is a test skill.
"#;
        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();

        skill_dir
    }

    #[test]
    fn test_validate_skill_structure_valid() {
        let temp_dir = TempDir::new().unwrap();
        let skill_dir = create_valid_skill_dir(&temp_dir, "valid-skill");

        let result = validate_skill_structure(&skill_dir);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_skill_structure_with_standard_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let skill_dir = create_skill_with_standard_dirs(&temp_dir, "skill-with-dirs");

        let result = validate_skill_structure(&skill_dir);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_skill_structure_missing_skill_md() {
        let temp_dir = TempDir::new().unwrap();
        let skill_dir = temp_dir.path().join("no-skill-md");
        std::fs::create_dir_all(&skill_dir).unwrap();

        let result = validate_skill_structure(&skill_dir);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("SKILL.md not found"));
    }

    #[test]
    fn test_validate_skill_structure_skill_md_not_file() {
        let temp_dir = TempDir::new().unwrap();
        let skill_dir = temp_dir.path().join("skill-not-file");
        std::fs::create_dir_all(&skill_dir).unwrap();
        // Create SKILL.md as a directory instead of a file
        std::fs::create_dir_all(skill_dir.join("SKILL.md")).unwrap();

        let result = validate_skill_structure(&skill_dir);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not a file"));
    }

    #[test]
    fn test_validate_skill_structure_standard_dir_is_file() {
        let temp_dir = TempDir::new().unwrap();
        let skill_dir = temp_dir.path().join("invalid-dir");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "content").unwrap();
        // Create "scripts" as a file instead of directory
        std::fs::write(skill_dir.join("scripts"), "not a dir").unwrap();

        let result = validate_skill_structure(&skill_dir);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not a directory"));
    }

    #[test]
    fn test_validate_allowed_tools_empty() {
        let result = validate_allowed_tools(&[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[test]
    fn test_validate_allowed_tools_valid() {
        let tools = vec![
            "bash".to_string(),
            "read".to_string(),
            "file_tool".to_string(),
            "tool-123".to_string(),
        ];
        let result = validate_allowed_tools(&tools);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_tool_name_valid() {
        assert!(validate_tool_name("bash").is_ok());
        assert!(validate_tool_name("read").is_ok());
        assert!(validate_tool_name("file_tool").is_ok());
        assert!(validate_tool_name("tool-123").is_ok());
        assert!(validate_tool_name("_private").is_ok());
        assert!(validate_tool_name("a").is_ok());
    }

    #[test]
    fn test_validate_tool_name_empty() {
        let result = validate_tool_name("");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[test]
    fn test_validate_tool_name_uppercase() {
        let result = validate_tool_name("Bash");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must start with a lowercase letter or underscore"));
    }

    #[test]
    fn test_validate_tool_name_digit_start() {
        let result = validate_tool_name("123tool");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must start with a lowercase letter or underscore"));
    }

    #[test]
    fn test_validate_tool_name_special_chars() {
        let result = validate_tool_name("tool@name");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid character"));
    }

    #[test]
    fn test_validate_tool_name_space() {
        let result = validate_tool_name("tool name");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid character"));
    }
}
