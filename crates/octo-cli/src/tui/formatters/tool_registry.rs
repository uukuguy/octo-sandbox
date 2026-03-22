//! Centralized tool display registry — single source of truth for how tools appear in the TUI.
//!
//! Adding a new tool = adding ONE entry to `TOOL_REGISTRY`.

use ratatui::style::Color;
use std::collections::HashMap;

use super::style_tokens;

/// Tool category for grouping purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCategory {
    FileRead,
    FileWrite,
    Bash,
    Search,
    Web,
    Agent,
    Symbol,
    Mcp,
    Plan,
    Docker,
    UserInteraction,
    Notebook,
    Other,
}

/// Which result formatter to use for a tool's output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultFormat {
    Bash,
    File,
    Directory,
    Generic,
    Todo,
}

/// Single source of truth for how a tool appears in the TUI.
pub struct ToolDisplayEntry {
    pub names: &'static [&'static str],
    pub category: ToolCategory,
    pub verb: &'static str,
    pub label: &'static str,
    pub primary_arg_keys: &'static [&'static str],
    pub result_format: ResultFormat,
}

static TOOL_REGISTRY: &[ToolDisplayEntry] = &[
    // File read tools
    ToolDisplayEntry {
        names: &["read_file", "Read", "file_read"],
        category: ToolCategory::FileRead,
        verb: "Read",
        label: "file",
        primary_arg_keys: &["file_path", "path"],
        result_format: ResultFormat::File,
    },
    ToolDisplayEntry {
        names: &["read_pdf"],
        category: ToolCategory::FileRead,
        verb: "Read",
        label: "pdf",
        primary_arg_keys: &["file_path", "path"],
        result_format: ResultFormat::File,
    },
    ToolDisplayEntry {
        names: &["list_files", "Glob"],
        category: ToolCategory::FileRead,
        verb: "List",
        label: "files",
        primary_arg_keys: &["path", "directory", "pattern"],
        result_format: ResultFormat::Directory,
    },
    // File write tools
    ToolDisplayEntry {
        names: &["write_file", "Write", "file_write"],
        category: ToolCategory::FileWrite,
        verb: "Write",
        label: "file",
        primary_arg_keys: &["file_path", "path"],
        result_format: ResultFormat::File,
    },
    ToolDisplayEntry {
        names: &["edit_file", "Edit", "file_edit"],
        category: ToolCategory::FileWrite,
        verb: "Edit",
        label: "file",
        primary_arg_keys: &["file_path", "path"],
        result_format: ResultFormat::File,
    },
    ToolDisplayEntry {
        names: &["multi_edit"],
        category: ToolCategory::FileWrite,
        verb: "Edit",
        label: "file",
        primary_arg_keys: &["file_path", "path"],
        result_format: ResultFormat::File,
    },
    ToolDisplayEntry {
        names: &["patch_file", "patch"],
        category: ToolCategory::FileWrite,
        verb: "Patch",
        label: "file",
        primary_arg_keys: &["file_path", "path"],
        result_format: ResultFormat::File,
    },
    // Bash/command tools
    ToolDisplayEntry {
        names: &["run_command", "bash_execute", "Bash", "bash"],
        category: ToolCategory::Bash,
        verb: "Bash",
        label: "command",
        primary_arg_keys: &["command"],
        result_format: ResultFormat::Bash,
    },
    // Search tools
    ToolDisplayEntry {
        names: &["grep", "search", "Grep"],
        category: ToolCategory::Search,
        verb: "Grep",
        label: "project",
        primary_arg_keys: &["pattern", "query"],
        result_format: ResultFormat::Directory,
    },
    ToolDisplayEntry {
        names: &["web_search"],
        category: ToolCategory::Search,
        verb: "Search",
        label: "web",
        primary_arg_keys: &["query", "pattern"],
        result_format: ResultFormat::Generic,
    },
    // Web tools
    ToolDisplayEntry {
        names: &["fetch_url", "web_fetch"],
        category: ToolCategory::Web,
        verb: "Fetch",
        label: "url",
        primary_arg_keys: &["url"],
        result_format: ResultFormat::Generic,
    },
    // Agent tools
    ToolDisplayEntry {
        names: &["spawn_subagent"],
        category: ToolCategory::Agent,
        verb: "Spawn",
        label: "subagent",
        primary_arg_keys: &["description"],
        result_format: ResultFormat::Generic,
    },
    // User interaction
    ToolDisplayEntry {
        names: &["ask_user"],
        category: ToolCategory::UserInteraction,
        verb: "Ask",
        label: "user",
        primary_arg_keys: &["question", "message"],
        result_format: ResultFormat::Generic,
    },
    // Skill execution
    ToolDisplayEntry {
        names: &["execute_skill"],
        category: ToolCategory::Agent,
        verb: "Execute Skill",
        label: "skill",
        primary_arg_keys: &["skill_name"],
        result_format: ResultFormat::Generic,
    },
    // Batch
    ToolDisplayEntry {
        names: &["batch_tool"],
        category: ToolCategory::Other,
        verb: "Batch",
        label: "tools",
        primary_arg_keys: &["invocations"],
        result_format: ResultFormat::Generic,
    },
];

static DEFAULT_ENTRY: ToolDisplayEntry = ToolDisplayEntry {
    names: &[],
    category: ToolCategory::Other,
    verb: "Call",
    label: "",
    primary_arg_keys: &["command", "file_path", "path", "url", "query", "pattern", "name"],
    result_format: ResultFormat::Generic,
};

static MCP_ENTRY: ToolDisplayEntry = ToolDisplayEntry {
    names: &[],
    category: ToolCategory::Mcp,
    verb: "MCP",
    label: "tool",
    primary_arg_keys: &["command", "file_path", "path", "url", "query", "pattern", "name"],
    result_format: ResultFormat::Generic,
};

static DOCKER_ENTRY: ToolDisplayEntry = ToolDisplayEntry {
    names: &[],
    category: ToolCategory::Docker,
    verb: "Docker",
    label: "operation",
    primary_arg_keys: &["command", "container", "image", "name"],
    result_format: ResultFormat::Generic,
};

/// Look up a tool's display metadata by name.
pub fn lookup_tool(name: &str) -> &'static ToolDisplayEntry {
    for entry in TOOL_REGISTRY {
        if entry.names.contains(&name) {
            return entry;
        }
    }
    if name.starts_with("mcp__") {
        return &MCP_ENTRY;
    }
    if name.starts_with("docker_") {
        return &DOCKER_ENTRY;
    }
    &DEFAULT_ENTRY
}

/// Classify a tool name into its category.
pub fn categorize_tool(tool_name: &str) -> ToolCategory {
    lookup_tool(tool_name).category
}

/// Get the primary display color for a tool category.
pub fn tool_color(_category: ToolCategory) -> Color {
    style_tokens::WARNING
}

/// Human-friendly display name for a tool. Returns `(verb, label)`.
pub fn tool_display_parts(tool_name: &str) -> (&'static str, &'static str) {
    let entry = lookup_tool(tool_name);
    (entry.verb, entry.label)
}

fn extract_arg_from_keys(
    keys: &[&str],
    args: &HashMap<String, serde_json::Value>,
) -> Option<String> {
    if args.is_empty() {
        return None;
    }
    for key in keys {
        if let Some(val) = args.get(*key) {
            if let Some(s) = val.as_str() {
                return Some(s.replace('\n', " "));
            }
        }
    }
    None
}

/// Format a tool call with arguments for display.
pub fn format_tool_call_display(
    tool_name: &str,
    args: &HashMap<String, serde_json::Value>,
) -> String {
    let (verb, arg) = format_tool_call_parts(tool_name, args);
    format!("{verb}({arg})")
}

/// Format a tool call into separate verb and arg parts.
pub fn format_tool_call_parts(
    tool_name: &str,
    args: &HashMap<String, serde_json::Value>,
) -> (String, String) {
    use super::path_shortener::PathShortener;
    let shortener = PathShortener::default();
    format_tool_call_parts_short(tool_name, args, &shortener)
}

/// Format a tool call into separate verb and arg parts, with optional working directory.
pub fn format_tool_call_parts_with_wd(
    tool_name: &str,
    args: &HashMap<String, serde_json::Value>,
    working_dir: Option<&str>,
) -> (String, String) {
    use super::path_shortener::PathShortener;
    let shortener = PathShortener::new(working_dir);
    format_tool_call_parts_short(tool_name, args, &shortener)
}

/// Format a tool call into separate verb and arg parts using a cached `PathShortener`.
pub fn format_tool_call_parts_short(
    tool_name: &str,
    args: &HashMap<String, serde_json::Value>,
    shortener: &super::path_shortener::PathShortener,
) -> (String, String) {
    let (verb, arg) = format_parts_inner(tool_name, args, shortener);
    let shortened = shortener.shorten_text(&arg);
    let truncated = if shortened.len() > 80 {
        format!("{}...", &shortened[..77])
    } else {
        shortened
    };
    (verb, truncated)
}

fn format_parts_inner(
    tool_name: &str,
    args: &HashMap<String, serde_json::Value>,
    shortener: &super::path_shortener::PathShortener,
) -> (String, String) {
    let entry = lookup_tool(tool_name);

    // Special case: batch_tool
    if tool_name == "batch_tool" {
        let count = args
            .get("invocations")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        return ("Batch".to_string(), format!("{count} tool calls"));
    }

    // Special case: spawn_subagent
    if tool_name == "spawn_subagent" {
        let verb = args
            .get("agent_type")
            .and_then(|v| v.as_str())
            .map(|s| match s {
                "Explore" | "Code-Explorer" | "code_explorer" => "Explore".to_string(),
                "Planner" | "planner" => "Plan".to_string(),
                "ask-user" | "ask_user" => "AskUser".to_string(),
                other => other.to_string(),
            })
            .unwrap_or_else(|| "Agent".to_string());
        let task = extract_arg_from_keys(&["description", "task"], args)
            .unwrap_or_else(|| "working...".to_string());
        return (verb, task);
    }

    // Special case: grep tools
    if matches!(tool_name, "grep" | "search" | "Grep") {
        let pattern = args
            .get("pattern")
            .or_else(|| args.get("query"))
            .and_then(|v| v.as_str())
            .unwrap_or("...");
        let pattern_display = if pattern.len() > 40 {
            format!("\"{}...\"", &pattern[..37])
        } else {
            format!("\"{pattern}\"")
        };
        if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
            let rel = shortener.shorten(path);
            return ("Grep".to_string(), format!("{pattern_display} in {rel}"));
        }
        return ("Grep".to_string(), pattern_display);
    }

    // Unknown tools: derive pretty display name
    if entry.verb == "Call" {
        let pretty_name = tool_name
            .replace('_', " ")
            .split_whitespace()
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    Some(ch) => format!("{}{}", ch.to_uppercase(), c.as_str()),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        if let Some(arg) = extract_arg_from_keys(entry.primary_arg_keys, args) {
            return (pretty_name, arg);
        }
        return (pretty_name, String::new());
    }

    // Try to extract a meaningful summary from args
    if let Some(summary) = extract_arg_from_keys(entry.primary_arg_keys, args) {
        let is_path_arg = entry
            .primary_arg_keys
            .first()
            .is_some_and(|k| *k == "file_path" || *k == "path");
        let summary = if is_path_arg {
            shortener.shorten(&summary)
        } else {
            summary
        };
        return (entry.verb.to_string(), summary);
    }

    // MCP tool: show server/tool format
    if tool_name.starts_with("mcp__") {
        let parts: Vec<&str> = tool_name.splitn(3, "__").collect();
        if parts.len() == 3 {
            return ("MCP".to_string(), format!("{}/{}", parts[1], parts[2]));
        }
    }

    (entry.verb.to_string(), entry.label.to_string())
}

/// Green gradient colors for nested tool spinner animation.
pub const GREEN_GRADIENT: &[Color] = &[
    Color::Rgb(0, 200, 80),
    Color::Rgb(0, 220, 100),
    Color::Rgb(0, 240, 120),
    Color::Rgb(0, 255, 140),
    Color::Rgb(0, 240, 120),
    Color::Rgb(0, 220, 100),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_duplicate_names_in_registry() {
        let mut seen = std::collections::HashSet::new();
        for entry in TOOL_REGISTRY {
            for name in entry.names {
                assert!(seen.insert(name), "Duplicate tool name in registry: {name}");
            }
        }
    }

    #[test]
    fn test_categorize_tool() {
        assert_eq!(categorize_tool("read_file"), ToolCategory::FileRead);
        assert_eq!(categorize_tool("edit_file"), ToolCategory::FileWrite);
        assert_eq!(categorize_tool("run_command"), ToolCategory::Bash);
        assert_eq!(categorize_tool("mcp__server__func"), ToolCategory::Mcp);
        assert_eq!(categorize_tool("docker_start"), ToolCategory::Docker);
        assert_eq!(categorize_tool("unknown_tool"), ToolCategory::Other);
    }

    #[test]
    fn test_tool_display_parts() {
        assert_eq!(tool_display_parts("read_file"), ("Read", "file"));
        assert_eq!(tool_display_parts("run_command"), ("Bash", "command"));
        assert_eq!(tool_display_parts("mcp__something"), ("MCP", "tool"));
        assert_eq!(tool_display_parts("unknown_xyz"), ("Call", ""));
    }

    #[test]
    fn test_format_tool_call_display() {
        let mut args = HashMap::new();
        args.insert(
            "command".to_string(),
            serde_json::Value::String("ls -la".to_string()),
        );
        let display = format_tool_call_display("run_command", &args);
        assert_eq!(display, "Bash(ls -la)");
    }

    #[test]
    fn test_format_tool_call_no_args() {
        let args = HashMap::new();
        let (verb, _arg) = format_tool_call_parts("ask_user", &args);
        assert_eq!(verb, "Ask");
    }

    #[test]
    fn test_format_mcp_tool() {
        let args = HashMap::new();
        let display = format_tool_call_display("mcp__sqlite__query", &args);
        assert_eq!(display, "MCP(sqlite/query)");
    }

    #[test]
    fn test_all_tools_have_consistent_color() {
        let categories = [
            ToolCategory::FileRead,
            ToolCategory::FileWrite,
            ToolCategory::Bash,
            ToolCategory::Search,
            ToolCategory::Mcp,
            ToolCategory::Other,
        ];
        for cat in categories {
            assert_eq!(tool_color(cat), style_tokens::WARNING);
        }
    }

    #[test]
    fn test_lookup_tool_exact_match() {
        let entry = lookup_tool("read_file");
        assert_eq!(entry.verb, "Read");
        assert_eq!(entry.label, "file");
        assert_eq!(entry.category, ToolCategory::FileRead);
    }

    #[test]
    fn test_lookup_tool_prefix_fallback() {
        let entry = lookup_tool("mcp__some_server__some_tool");
        assert_eq!(entry.category, ToolCategory::Mcp);

        let entry = lookup_tool("docker_run");
        assert_eq!(entry.category, ToolCategory::Docker);
    }

    #[test]
    fn test_unknown_tool_derives_pretty_name() {
        let args = HashMap::new();
        let (verb, arg) = format_tool_call_parts("some_fancy_tool", &args);
        assert_eq!(verb, "Some Fancy Tool");
        assert_eq!(arg, "");
    }

    #[test]
    fn test_batch_tool_display() {
        let mut args = HashMap::new();
        args.insert(
            "invocations".to_string(),
            serde_json::json!([{"tool": "read_file"}, {"tool": "edit_file"}, {"tool": "bash"}]),
        );
        let (verb, arg) = format_tool_call_parts("batch_tool", &args);
        assert_eq!(verb, "Batch");
        assert_eq!(arg, "3 tool calls");
    }

    #[test]
    fn test_result_format_mapping() {
        assert_eq!(lookup_tool("run_command").result_format, ResultFormat::Bash);
        assert_eq!(lookup_tool("read_file").result_format, ResultFormat::File);
        assert_eq!(
            lookup_tool("list_files").result_format,
            ResultFormat::Directory
        );
        assert_eq!(lookup_tool("ask_user").result_format, ResultFormat::Generic);
    }

    #[test]
    fn test_format_spawn_subagent_strips_paths() {
        let mut args = HashMap::new();
        args.insert(
            "agent_type".to_string(),
            serde_json::Value::String("Explore".to_string()),
        );
        args.insert(
            "task".to_string(),
            serde_json::Value::String(
                "Explore repo at /Users/me/project with focus on tests".to_string(),
            ),
        );
        let (verb, arg) =
            format_tool_call_parts_with_wd("spawn_subagent", &args, Some("/Users/me/project"));
        assert_eq!(verb, "Explore");
        assert_eq!(arg, "Explore repo at . with focus on tests");
    }
}
