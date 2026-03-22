//! Centralized path shortening for all TUI display.
//!
//! `PathShortener` caches the home directory and working directory at construction
//! time, avoiding repeated syscalls. All path display in the TUI should flow
//! through this struct.

/// Caches home_dir and working_dir at construction time.
/// All methods are cheap string operations -- no syscalls after construction.
#[derive(Debug, Clone)]
pub struct PathShortener {
    working_dir: Option<String>,
    home_dir: Option<String>,
}

impl PathShortener {
    /// Construct with cached dirs. Resolves home directory exactly once.
    pub fn new(working_dir: Option<&str>) -> Self {
        Self {
            working_dir: working_dir.filter(|s| !s.is_empty()).map(|s| s.to_string()),
            home_dir: dirs::home_dir().map(|h| h.to_string_lossy().into_owned()),
        }
    }

    /// Single path: wd-prefix -> relative, home-prefix -> ~/..., else as-is.
    pub fn shorten(&self, path: &str) -> String {
        if let Some(ref wd) = self.working_dir {
            if path.starts_with(wd.as_str()) {
                let rel = path.strip_prefix(wd.as_str()).unwrap_or(path);
                let rel = rel.strip_prefix('/').unwrap_or(rel);
                if rel.is_empty() {
                    return ".".to_string();
                }
                return rel.to_string();
            }
        }
        let cleaned = path.strip_prefix("./").unwrap_or(path);
        self.replace_home_prefix(cleaned)
    }

    /// Free-form text: replace all occurrences of wd and home with short forms.
    pub fn shorten_text(&self, text: &str) -> String {
        let result = if let Some(ref wd) = self.working_dir {
            let wd_slash = format!("{wd}/");
            let result = text.replace(&wd_slash, "");
            self.replace_at_boundary(&result, wd, ".")
        } else {
            text.to_string()
        };
        self.replace_home_in_text(&result)
    }

    /// Shorten a path for status bar display: home -> ~, then keep last 1 component.
    pub fn shorten_display(&self, path: &str) -> String {
        let display = self.replace_home_prefix(path);

        if let Some(after_tilde) = display.strip_prefix("~/") {
            let parts: Vec<&str> = after_tilde.split('/').filter(|p| !p.is_empty()).collect();
            if parts.len() <= 1 {
                return display;
            }
            return format!("~/\u{2026}/{}", parts[parts.len() - 1]);
        }

        let parts: Vec<&str> = display.split('/').filter(|p| !p.is_empty()).collect();
        if parts.len() <= 1 {
            return display;
        }
        format!("\u{2026}/{}", parts[parts.len() - 1])
    }

    fn replace_home_prefix(&self, path: &str) -> String {
        if let Some(ref home) = self.home_dir {
            if let Some(rest) = path.strip_prefix(home.as_str()) {
                let rest = rest.strip_prefix('/').unwrap_or(rest);
                if rest.is_empty() {
                    return "~".to_string();
                }
                return format!("~/{rest}");
            }
        }
        path.to_string()
    }

    fn replace_home_in_text(&self, text: &str) -> String {
        let home = match self.home_dir {
            Some(ref h) => h,
            None => return text.to_string(),
        };
        let home_slash = format!("{home}/");
        let result = text.replace(&home_slash, "~/");
        self.replace_at_boundary(&result, home, "~")
    }

    fn replace_at_boundary(&self, text: &str, needle: &str, replacement: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut remaining = text;
        while let Some(pos) = remaining.find(needle) {
            out.push_str(&remaining[..pos]);
            let after = &remaining[pos + needle.len()..];
            let extends_path = after
                .as_bytes()
                .first()
                .is_some_and(|&b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.');
            if extends_path {
                out.push_str(needle);
            } else {
                out.push_str(replacement);
            }
            remaining = after;
        }
        out.push_str(remaining);
        out
    }
}

impl Default for PathShortener {
    fn default() -> Self {
        Self::new(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> String {
        dirs::home_dir().unwrap().to_string_lossy().into_owned()
    }

    #[test]
    fn test_shorten_relative_to_working_dir() {
        let home = home();
        let ps = PathShortener::new(Some(&format!("{home}/project")));
        assert_eq!(
            ps.shorten(&format!("{home}/project/src/main.rs")),
            "src/main.rs"
        );
    }

    #[test]
    fn test_shorten_working_dir_itself() {
        let home = home();
        let ps = PathShortener::new(Some(&format!("{home}/project")));
        assert_eq!(ps.shorten(&format!("{home}/project")), ".");
    }

    #[test]
    fn test_shorten_outside_working_dir_uses_tilde() {
        let home = home();
        let ps = PathShortener::new(Some(&format!("{home}/project")));
        assert_eq!(
            ps.shorten(&format!("{home}/other/src/main.rs")),
            "~/other/src/main.rs"
        );
    }

    #[test]
    fn test_shorten_strips_dot_slash() {
        let ps = PathShortener::new(Some("/project"));
        assert_eq!(ps.shorten("./src/main.rs"), "src/main.rs");
    }

    #[test]
    fn test_shorten_text_replaces_wd() {
        let home = home();
        let ps = PathShortener::new(Some(&format!("{home}/project")));
        let text = format!("Explore repo at {home}/project/src with focus on tests");
        assert_eq!(
            ps.shorten_text(&text),
            "Explore repo at src with focus on tests"
        );
    }

    #[test]
    fn test_default_no_working_dir() {
        let ps = PathShortener::default();
        assert!(ps.working_dir.is_none());
        assert!(ps.home_dir.is_some());
    }
}
