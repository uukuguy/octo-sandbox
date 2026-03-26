//! Custom slash command loader.
//!
//! Loads `.md` files from `~/.octo/commands/` (global) and `$PWD/.octo/commands/`
//! (project). Each `.md` file becomes a `/filename` slash command whose content
//! is used as a prompt template. `$ARGUMENTS` in the template is replaced with
//! user-provided arguments.
//!
//! Subdirectories create namespaced commands: `commands/foo/bar.md` → `/foo:bar`.
//!
//! Project commands take priority over global commands with the same name.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tracing::{debug, warn};

/// A loaded custom command.
#[derive(Debug, Clone)]
pub struct CustomCommand {
    /// Command name as shown in autocomplete (e.g. "deploy", "review:pr").
    pub name: String,
    /// Short description (first non-empty line of the file, or filename).
    pub description: String,
    /// Full prompt template content.
    pub template: String,
    /// Whether the template contains `$ARGUMENTS`.
    pub has_arguments: bool,
    /// Source file path (for diagnostics).
    pub source_path: PathBuf,
    /// Whether this is a project-level command (vs global).
    pub is_project: bool,
}

impl CustomCommand {
    /// Expand the template, replacing `$ARGUMENTS` with the given args string.
    pub fn expand(&self, arguments: &str) -> String {
        self.template.replace("$ARGUMENTS", arguments)
    }
}

/// Load custom commands from the given directories.
///
/// Directories are processed in order; earlier entries take priority
/// (project before global). Returns commands keyed by name.
pub fn load_commands(dirs: &[PathBuf]) -> Vec<CustomCommand> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut commands = Vec::new();

    for (dir_idx, dir) in dirs.iter().enumerate() {
        if !dir.is_dir() {
            continue;
        }
        let is_project = dir_idx == 0;
        load_from_dir(dir, dir, is_project, &mut seen, &mut commands);
    }

    debug!(count = commands.len(), "Loaded custom commands");
    commands
}

/// Recursively load `.md` files from a directory tree.
fn load_from_dir(
    base: &Path,
    dir: &Path,
    is_project: bool,
    seen: &mut HashMap<String, usize>,
    commands: &mut Vec<CustomCommand>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            debug!(dir = %dir.display(), error = %e, "Cannot read commands directory");
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            load_from_dir(base, &path, is_project, seen, commands);
            continue;
        }

        // Only process .md files
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }

        let name = derive_command_name(base, &path);
        if name.is_empty() {
            continue;
        }

        // Project commands (earlier dirs) take priority
        if seen.contains_key(&name) {
            debug!(name, path = %path.display(), "Skipping duplicate command");
            continue;
        }

        match std::fs::read_to_string(&path) {
            Ok(content) => {
                let description = extract_description(&content, &name);
                let has_arguments = content.contains("$ARGUMENTS");
                let idx = commands.len();
                seen.insert(name.clone(), idx);
                commands.push(CustomCommand {
                    name,
                    description,
                    template: content,
                    has_arguments,
                    source_path: path,
                    is_project,
                });
            }
            Err(e) => {
                warn!(path = %path.display(), error = %e, "Failed to read command file");
            }
        }
    }
}

/// Derive a command name from the file path relative to the base directory.
///
/// `commands/deploy.md` → `"deploy"`
/// `commands/review/pr.md` → `"review:pr"`
fn derive_command_name(base: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(base).unwrap_or(path);
    let stem = relative.with_extension("");
    let parts: Vec<&str> = stem
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();
    parts.join(":")
}

/// Extract a short description from the command file content.
///
/// Uses the first non-empty, non-heading line. Falls back to the command name.
fn extract_description(content: &str, fallback_name: &str) -> String {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("---") {
            continue;
        }
        // Truncate long descriptions
        if trimmed.len() > 80 {
            return format!("{}...", &trimmed[..77]);
        }
        return trimmed.to_string();
    }
    format!("Custom command: {}", fallback_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_derive_command_name_simple() {
        let base = Path::new("/home/user/.octo/commands");
        let path = Path::new("/home/user/.octo/commands/deploy.md");
        assert_eq!(derive_command_name(base, path), "deploy");
    }

    #[test]
    fn test_derive_command_name_nested() {
        let base = Path::new("/home/user/.octo/commands");
        let path = Path::new("/home/user/.octo/commands/review/pr.md");
        assert_eq!(derive_command_name(base, path), "review:pr");
    }

    #[test]
    fn test_extract_description_first_line() {
        let content = "# Title\n\nDeploy the application to production.\n\nMore details...";
        assert_eq!(
            extract_description(content, "deploy"),
            "Deploy the application to production."
        );
    }

    #[test]
    fn test_extract_description_fallback() {
        let content = "# Just a heading\n\n";
        assert_eq!(
            extract_description(content, "deploy"),
            "Custom command: deploy"
        );
    }

    #[test]
    fn test_custom_command_expand() {
        let cmd = CustomCommand {
            name: "test".into(),
            description: "A test".into(),
            template: "Do $ARGUMENTS for me".into(),
            has_arguments: true,
            source_path: PathBuf::from("test.md"),
            is_project: false,
        };
        assert_eq!(cmd.expand("something cool"), "Do something cool for me");
    }

    #[test]
    fn test_custom_command_expand_no_placeholder() {
        let cmd = CustomCommand {
            name: "test".into(),
            description: "A test".into(),
            template: "Always do this".into(),
            has_arguments: false,
            source_path: PathBuf::from("test.md"),
            is_project: false,
        };
        assert_eq!(cmd.expand("ignored"), "Always do this");
    }

    #[test]
    fn test_load_commands_from_dirs() {
        let tmp = tempdir().unwrap();

        // Create project commands dir
        let project_dir = tmp.path().join("project");
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::write(
            project_dir.join("deploy.md"),
            "Deploy the app.\n\nDeploy $ARGUMENTS to production.",
        )
        .unwrap();

        // Create global commands dir
        let global_dir = tmp.path().join("global");
        std::fs::create_dir_all(&global_dir).unwrap();
        std::fs::write(
            global_dir.join("deploy.md"),
            "Global deploy (should be shadowed).",
        )
        .unwrap();
        std::fs::write(global_dir.join("greet.md"), "Say hello to $ARGUMENTS.").unwrap();

        let commands = load_commands(&[project_dir, global_dir]);
        assert_eq!(commands.len(), 2); // deploy (project) + greet (global)

        let deploy = commands.iter().find(|c| c.name == "deploy").unwrap();
        assert!(deploy.is_project);
        assert!(deploy.has_arguments);
        assert!(deploy.template.contains("production"));

        let greet = commands.iter().find(|c| c.name == "greet").unwrap();
        assert!(!greet.is_project);
        assert!(greet.has_arguments);
    }

    #[test]
    fn test_load_commands_nested() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path().join("commands");
        let sub = dir.join("review");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("pr.md"), "Review this PR.").unwrap();

        let commands = load_commands(&[dir]);
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name, "review:pr");
    }

    #[test]
    fn test_load_commands_empty_dir() {
        let tmp = tempdir().unwrap();
        let commands = load_commands(&[tmp.path().to_path_buf()]);
        assert!(commands.is_empty());
    }

    #[test]
    fn test_load_commands_nonexistent_dir() {
        let commands = load_commands(&[PathBuf::from("/nonexistent/path")]);
        assert!(commands.is_empty());
    }

    #[test]
    fn test_ignores_non_md_files() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path().join("commands");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("readme.txt"), "Not a command").unwrap();
        std::fs::write(dir.join("deploy.md"), "Deploy it.").unwrap();

        let commands = load_commands(&[dir]);
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name, "deploy");
    }
}
