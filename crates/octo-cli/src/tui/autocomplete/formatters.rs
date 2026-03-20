//! Format completion items for popup display.

use std::path::Path;

use super::file_finder::format_file_size;
use super::{CompletionItem, CompletionKind};

/// Return a short type tag and a color hint for a file path.
pub fn file_type_indicator(path: &str) -> (&'static str, &'static str) {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let name = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    match ext.to_lowercase().as_str() {
        "py" => ("py", "Cyan"),
        "js" => ("js", "Yellow"),
        "jsx" => ("jsx", "Yellow"),
        "ts" => ("ts", "Blue"),
        "tsx" => ("tsx", "Blue"),
        "rs" => ("rs", "Red"),
        "go" => ("go", "Cyan"),
        "java" => ("java", "Red"),
        "c" | "h" => ("c", "Blue"),
        "cpp" | "cc" | "hpp" => ("cpp", "Blue"),
        "rb" => ("rb", "Red"),
        "html" | "htm" => ("html", "Yellow"),
        "css" => ("css", "Blue"),
        "json" => ("json", "Yellow"),
        "yaml" | "yml" => ("yaml", "Magenta"),
        "toml" => ("toml", "Magenta"),
        "md" | "markdown" => ("md", "Blue"),
        "txt" => ("txt", "Gray"),
        "sh" | "bash" | "zsh" => ("sh", "Green"),
        "sql" => ("sql", "Yellow"),
        _ => match name {
            "Makefile" => ("make", "Red"),
            "Dockerfile" => ("dock", "Blue"),
            _ => ("file", "Gray"),
        },
    }
}

/// Shorten a path for display in the popup.
pub fn shorten_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        return path.to_string();
    }
    let parts: Vec<&str> = path.split('/').collect();
    let mut result = String::new();
    for (i, part) in parts.iter().enumerate().rev() {
        let candidate = if result.is_empty() {
            part.to_string()
        } else {
            format!("{}/{}", part, result)
        };
        if candidate.len() + 4 > max_len && i > 0 {
            return format!(".../{}", result);
        }
        result = candidate;
    }
    result
}

/// Formats completion items into display strings.
pub struct CompletionFormatter;

impl CompletionFormatter {
    /// Format a completion item into `(left_label, right_meta)`.
    pub fn format(item: &CompletionItem) -> (String, String) {
        match item.kind {
            CompletionKind::Command => {
                let label = format!("{:<18}", item.label);
                (label, item.description.clone())
            }
            CompletionKind::File => {
                let (type_tag, _color) = file_type_indicator(&item.label);
                let shortened = shorten_path(&item.label, 46);
                let label = format!("{} {:<46}", type_tag, shortened);
                let meta = if item.description.is_empty() {
                    String::new()
                } else {
                    format!("{:>10}", item.description)
                };
                (label, meta)
            }
            CompletionKind::Symbol => {
                let label = format!("{:<30}", item.label);
                (label, item.description.clone())
            }
        }
    }

    /// Get a human-readable file size string for a path.
    pub fn file_size_string(path: &Path) -> String {
        match std::fs::metadata(path) {
            Ok(meta) => format_file_size(meta.len()),
            Err(_) => String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_type_indicator_rust() {
        let (tag, color) = file_type_indicator("src/main.rs");
        assert_eq!(tag, "rs");
        assert_eq!(color, "Red");
    }

    #[test]
    fn test_file_type_indicator_python() {
        let (tag, _) = file_type_indicator("script.py");
        assert_eq!(tag, "py");
    }

    #[test]
    fn test_file_type_indicator_unknown() {
        let (tag, _) = file_type_indicator("data.xyz");
        assert_eq!(tag, "file");
    }

    #[test]
    fn test_shorten_path_short() {
        assert_eq!(shorten_path("src/lib.rs", 30), "src/lib.rs");
    }

    #[test]
    fn test_shorten_path_long() {
        let long = "very/deep/nested/directory/structure/file.rs";
        let shortened = shorten_path(long, 25);
        assert!(shortened.contains("...") || shortened.len() <= 25);
    }

    #[test]
    fn test_format_command() {
        let item = CompletionItem {
            insert_text: "/help".into(),
            label: "/help".into(),
            description: "show available commands".into(),
            kind: CompletionKind::Command,
            score: 0.0,
        };
        let (label, desc) = CompletionFormatter::format(&item);
        assert!(label.contains("/help"));
        assert!(desc.contains("show available commands"));
    }
}
