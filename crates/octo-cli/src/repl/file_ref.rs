//! @file reference expansion — resolves @path references in user input

use std::path::Path;

/// Expand @file references in user input.
///
/// Scans for `@path` patterns and replaces them with file contents.
/// If a file doesn't exist or can't be read, includes an error note.
///
/// # Examples
/// ```
/// let input = "Explain @src/main.rs";
/// let expanded = expand_file_refs(input, &std::env::current_dir().unwrap());
/// // → "Explain \n<file path=\"src/main.rs\">\n...file contents...\n</file>"
/// ```
pub fn expand_file_refs(input: &str, working_dir: &Path) -> String {
    if !input.contains('@') {
        return input.to_string();
    }

    let mut result = String::new();
    let mut chars = input.char_indices().peekable();

    while let Some((i, ch)) = chars.next() {
        if ch == '@' {
            // Check if this looks like a file path:
            // - not at end of string
            // - next char is not whitespace or @
            // - there's a path-like character following
            let rest = &input[i + 1..];
            if let Some(path_str) = extract_path(rest) {
                // Advance past the path characters
                for _ in 0..path_str.len() {
                    chars.next();
                }

                // Resolve the path
                let file_path = if Path::new(path_str).is_absolute() {
                    std::path::PathBuf::from(path_str)
                } else {
                    working_dir.join(path_str)
                };

                match std::fs::read_to_string(&file_path) {
                    Ok(contents) => {
                        result.push_str(&format!(
                            "\n<file path=\"{}\">\n{}\n</file>",
                            path_str,
                            contents.trim_end()
                        ));
                    }
                    Err(e) => {
                        result.push_str(&format!(
                            "\n<file path=\"{}\" error=\"{}\" />",
                            path_str, e
                        ));
                    }
                }
            } else {
                result.push(ch);
            }
        } else {
            result.push(ch);
        }
    }

    result
}

/// Extract a file path from the start of a string.
/// Returns the path string if it looks like a valid path, None otherwise.
fn extract_path(s: &str) -> Option<&str> {
    if s.is_empty() {
        return None;
    }

    let first = s.chars().next()?;
    // Path must start with a letter, dot, slash, or ~
    if !first.is_alphanumeric() && first != '.' && first != '/' && first != '~' {
        return None;
    }

    // Find the end of the path (whitespace or certain punctuation)
    let end = s
        .char_indices()
        .find(|(_, c)| c.is_whitespace() || *c == ',' || *c == ';' || *c == ')')
        .map(|(i, _)| i)
        .unwrap_or(s.len());

    if end == 0 {
        return None;
    }

    // Must contain at least one path separator or dot to look like a file
    let candidate = &s[..end];
    if candidate.contains('/') || candidate.contains('.') {
        Some(candidate)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_refs() {
        let input = "Hello world";
        assert_eq!(expand_file_refs(input, Path::new(".")), "Hello world");
    }

    #[test]
    fn test_at_not_file() {
        // @ followed by non-path characters (no dot or slash)
        let input = "Send email to @user";
        assert_eq!(
            expand_file_refs(input, Path::new(".")),
            "Send email to @user"
        );
    }

    #[test]
    fn test_nonexistent_file() {
        let input = "Read @nonexistent_file.txt please";
        let result = expand_file_refs(input, Path::new("."));
        assert!(result.contains("error="));
        assert!(result.contains("nonexistent_file.txt"));
    }

    #[test]
    fn test_extract_path_with_slash() {
        assert_eq!(extract_path("src/main.rs rest"), Some("src/main.rs"));
    }

    #[test]
    fn test_extract_path_dot_file() {
        assert_eq!(extract_path("./file.txt"), Some("./file.txt"));
    }

    #[test]
    fn test_extract_path_no_dot_or_slash() {
        assert_eq!(extract_path("user"), None);
    }

    #[test]
    fn test_extract_path_starts_with_space() {
        assert_eq!(extract_path(" space"), None);
    }

    #[test]
    fn test_extract_path_empty() {
        assert_eq!(extract_path(""), None);
    }

    #[test]
    fn test_real_file() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        {
            let mut f = std::fs::File::create(&file_path).unwrap();
            writeln!(f, "hello world").unwrap();
        }
        let input = "Explain @test.txt";
        let result = expand_file_refs(input, dir.path());
        assert!(result.contains("<file path=\"test.txt\">"));
        assert!(result.contains("hello world"));
        assert!(result.contains("</file>"));
    }

    #[test]
    fn test_multiple_refs() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let f1 = dir.path().join("a.rs");
        let f2 = dir.path().join("b.rs");
        {
            let mut f = std::fs::File::create(&f1).unwrap();
            write!(f, "fn a() {{}}").unwrap();
        }
        {
            let mut f = std::fs::File::create(&f2).unwrap();
            write!(f, "fn b() {{}}").unwrap();
        }
        let input = "Compare @a.rs and @b.rs";
        let result = expand_file_refs(input, dir.path());
        assert!(result.contains("<file path=\"a.rs\">"));
        assert!(result.contains("<file path=\"b.rs\">"));
        assert!(result.contains("fn a()"));
        assert!(result.contains("fn b()"));
    }
}
