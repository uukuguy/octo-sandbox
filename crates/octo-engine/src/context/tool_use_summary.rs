//! Heuristic tool use summarizer — extracts key information from tool outputs
//! for context-preserving observation masking.
//!
//! When old tool results are masked to save tokens, this module generates
//! brief summaries instead of the generic "[output hidden - N chars]" placeholder.
//! No LLM calls — pure heuristic extraction.

use std::collections::HashSet;

/// Summarize a tool's output heuristically.
///
/// Returns a brief summary suitable for replacing the full output in masked messages.
/// The summary preserves key information (file paths, error messages, counts, status).
pub fn summarize_tool_output(tool_name: &str, input: &str, output: &str, is_error: bool) -> String {
    if is_error {
        return summarize_error(tool_name, output);
    }

    match tool_name {
        "bash" => summarize_bash(input, output),
        "file_read" => summarize_file_read(input, output),
        "file_write" | "file_edit" => summarize_file_write(tool_name, input, output),
        "grep" => summarize_grep(output),
        "glob" => summarize_glob(output),
        "web_search" => summarize_web_search(output),
        "web_fetch" => summarize_web_fetch(output),
        "memory_store" | "memory_recall" | "memory_search" | "memory_edit"
        | "memory_timeline" | "memory_forget" | "memory_update" | "memory_compress" => {
            summarize_memory_tool(tool_name, output)
        }
        _ => summarize_generic(tool_name, output),
    }
}

fn summarize_error(tool_name: &str, output: &str) -> String {
    // Extract first meaningful error line
    let first_error = output
        .lines()
        .find(|l| {
            let lower = l.to_lowercase();
            lower.contains("error")
                || lower.contains("failed")
                || lower.contains("exception")
                || lower.contains("panic")
                || lower.contains("denied")
        })
        .unwrap_or_else(|| output.lines().next().unwrap_or("unknown error"));

    let truncated: String = first_error.chars().take(120).collect();
    format!("[{tool_name} ERROR: {truncated}]")
}

fn summarize_bash(input: &str, output: &str) -> String {
    let cmd: String = input.chars().take(60).collect();
    let line_count = output.lines().count();
    let char_count = output.len();

    // Check for common patterns
    if output.contains("test result:") {
        // Test output
        let test_line = output
            .lines()
            .find(|l| l.contains("test result:"))
            .unwrap_or("");
        return format!("[bash({cmd}): {test_line}]");
    }

    if output.contains("Compiling") || output.contains("Finished") {
        let last_line = output.lines().last().unwrap_or("");
        let truncated: String = last_line.chars().take(80).collect();
        return format!("[bash({cmd}): {truncated}]");
    }

    // Default: line/char count summary
    format!("[bash({cmd}): {line_count} lines, {char_count} chars]")
}

fn summarize_file_read(input: &str, output: &str) -> String {
    let line_count = output.lines().count();
    let path = extract_path_from_input(input);
    format!("[read {path}: {line_count} lines]")
}

fn summarize_file_write(tool_name: &str, input: &str, output: &str) -> String {
    let path = extract_path_from_input(input);
    if output.contains("success") || output.contains("updated") || output.contains("created") {
        let verb = if tool_name == "file_edit" {
            "edited"
        } else {
            "wrote"
        };
        format!("[{verb} {path}: ok]")
    } else {
        format!("[{tool_name} {path}: {}]", output_preview(output, 80))
    }
}

fn summarize_grep(output: &str) -> String {
    let line_count = output.lines().count();
    if line_count == 0 {
        return "[grep: no matches]".into();
    }
    // Count unique files
    let file_count = output
        .lines()
        .filter_map(|l| l.split(':').next())
        .collect::<HashSet<_>>()
        .len();
    format!("[grep: {line_count} matches in {file_count} files]")
}

fn summarize_glob(output: &str) -> String {
    let file_count = output.lines().filter(|l| !l.is_empty()).count();
    if file_count == 0 {
        return "[glob: no matches]".into();
    }
    format!("[glob: {file_count} files matched]")
}

fn summarize_web_search(output: &str) -> String {
    let result_count = output
        .lines()
        .filter(|l| l.starts_with("- ") || l.starts_with("* "))
        .count();
    if result_count > 0 {
        format!("[web_search: {result_count} results]")
    } else {
        format!("[web_search: {}]", output_preview(output, 80))
    }
}

fn summarize_web_fetch(output: &str) -> String {
    let char_count = output.len();
    let line_count = output.lines().count();
    format!("[web_fetch: {line_count} lines, {char_count} chars]")
}

fn summarize_memory_tool(tool_name: &str, output: &str) -> String {
    if output.contains("success") || output.contains("stored") || output.contains("updated") {
        format!("[{tool_name}: ok]")
    } else {
        format!("[{tool_name}: {}]", output_preview(output, 80))
    }
}

fn summarize_generic(tool_name: &str, output: &str) -> String {
    let char_count = output.len();
    let line_count = output.lines().count();
    format!("[{tool_name}: {line_count} lines, {char_count} chars]")
}

/// Extract file path from tool input string.
fn extract_path_from_input(input: &str) -> String {
    // Try to parse as JSON first
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(input) {
        if let Some(path) = v.get("file_path").and_then(|v| v.as_str()) {
            return shorten_path(path);
        }
        if let Some(path) = v.get("path").and_then(|v| v.as_str()) {
            return shorten_path(path);
        }
    }
    // Fallback: take first token
    input.split_whitespace().next().unwrap_or("?").to_string()
}

/// Shorten a path for display (keep last 2 components).
fn shorten_path(path: &str) -> String {
    let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if components.len() <= 2 {
        return path.to_string();
    }
    format!(".../{}", components[components.len() - 2..].join("/"))
}

/// Get a preview of output text, truncated to max chars.
fn output_preview(output: &str, max_chars: usize) -> String {
    let first_line = output.lines().next().unwrap_or("");
    let truncated: String = first_line.chars().take(max_chars).collect();
    if first_line.chars().count() > max_chars {
        format!("{truncated}...")
    } else {
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summarize_bash_test_output() {
        let output = "running 5 tests\ntest foo ... ok\ntest result: ok. 5 passed; 0 failed";
        let result = summarize_tool_output("bash", "cargo test", output, false);
        assert!(result.contains("bash(cargo test)"));
        assert!(result.contains("test result:"));
        assert!(result.contains("5 passed"));
    }

    #[test]
    fn test_summarize_bash_compile_output() {
        let output = "   Compiling octo-engine v0.1.0\n   Finished dev [unoptimized + debuginfo]";
        let result = summarize_tool_output("bash", "cargo build", output, false);
        assert!(result.contains("bash(cargo build)"));
        assert!(result.contains("Finished"));
    }

    #[test]
    fn test_summarize_bash_generic() {
        let output = "line1\nline2\nline3\n";
        let result = summarize_tool_output("bash", "ls -la", output, false);
        assert!(result.contains("bash(ls -la)"));
        assert!(result.contains("3 lines"));
    }

    #[test]
    fn test_summarize_file_read() {
        let input = r#"{"file_path": "/home/user/src/main.rs"}"#;
        let output = "fn main() {\n    println!(\"hello\");\n}\n";
        let result = summarize_tool_output("file_read", input, output, false);
        assert!(result.contains("read"));
        assert!(result.contains("3 lines"));
        assert!(result.contains("src/main.rs"));
    }

    #[test]
    fn test_summarize_file_write_success() {
        let input = r#"{"file_path": "/home/user/src/lib.rs"}"#;
        let output = "file updated successfully";
        let result = summarize_tool_output("file_write", input, output, false);
        assert!(result.contains("wrote"));
        assert!(result.contains("ok"));
    }

    #[test]
    fn test_summarize_file_edit_success() {
        let input = r#"{"file_path": "/home/user/src/lib.rs"}"#;
        let output = "file updated successfully";
        let result = summarize_tool_output("file_edit", input, output, false);
        assert!(result.contains("edited"));
        assert!(result.contains("ok"));
    }

    #[test]
    fn test_summarize_grep_with_matches() {
        let output = "src/main.rs:10:fn main\nsrc/main.rs:20:fn helper\nsrc/lib.rs:5:fn init";
        let result = summarize_tool_output("grep", "", output, false);
        assert!(result.contains("3 matches"));
        assert!(result.contains("2 files"));
    }

    #[test]
    fn test_summarize_grep_no_matches() {
        let result = summarize_tool_output("grep", "", "", false);
        assert_eq!(result, "[grep: no matches]");
    }

    #[test]
    fn test_summarize_glob_with_files() {
        let output = "src/main.rs\nsrc/lib.rs\nsrc/util.rs";
        let result = summarize_tool_output("glob", "", output, false);
        assert!(result.contains("3 files matched"));
    }

    #[test]
    fn test_summarize_glob_no_files() {
        let result = summarize_tool_output("glob", "", "", false);
        assert_eq!(result, "[glob: no matches]");
    }

    #[test]
    fn test_summarize_error() {
        let output = "some preamble\nerror[E0308]: mismatched types\n  --> src/main.rs:10:5";
        let result = summarize_tool_output("bash", "cargo build", output, true);
        assert!(result.contains("bash ERROR"));
        assert!(result.contains("error[E0308]"));
    }

    #[test]
    fn test_summarize_web_search() {
        let output = "Results:\n- Result 1: foo\n- Result 2: bar\n- Result 3: baz";
        let result = summarize_tool_output("web_search", "", output, false);
        assert!(result.contains("3 results"));
    }

    #[test]
    fn test_summarize_memory_tool() {
        let result = summarize_tool_output("memory_store", "", "stored successfully", false);
        assert_eq!(result, "[memory_store: ok]");

        let result2 = summarize_tool_output("memory_search", "", "no results found", false);
        assert!(result2.contains("memory_search"));
        assert!(result2.contains("no results found"));
    }

    #[test]
    fn test_summarize_generic() {
        let output = "some output\nmore output";
        let result = summarize_tool_output("custom_tool", "", output, false);
        assert!(result.contains("custom_tool"));
        assert!(result.contains("2 lines"));
    }

    #[test]
    fn test_shorten_path() {
        assert_eq!(shorten_path("/a/b"), "/a/b");
        assert_eq!(shorten_path("/a/b/c/d/e.rs"), ".../d/e.rs");
        assert_eq!(shorten_path("ab"), "ab");
    }

    #[test]
    fn test_extract_path_from_json() {
        let input = r#"{"file_path": "/home/user/project/src/main.rs"}"#;
        let result = extract_path_from_input(input);
        assert_eq!(result, ".../src/main.rs");

        // Fallback to path key
        let input2 = r#"{"path": "/tmp/foo.txt"}"#;
        let result2 = extract_path_from_input(input2);
        assert_eq!(result2, "/tmp/foo.txt");

        // Non-JSON fallback
        let result3 = extract_path_from_input("some-file.txt extra");
        assert_eq!(result3, "some-file.txt");
    }

    #[test]
    fn test_output_preview() {
        assert_eq!(output_preview("short", 80), "short");
        let long = "a".repeat(100);
        let preview = output_preview(&long, 80);
        assert!(preview.ends_with("..."));
        assert_eq!(preview.len(), 83); // 80 chars + "..."
    }
}
