//! Completer trait and concrete implementations.

use std::path::PathBuf;

use super::file_finder::FileFinder;
use super::{builtin_commands, CompletionItem, CompletionKind, SlashCommand};

// ── Completer trait ────────────────────────────────────────────────

/// Trait for types that can produce completion items for a query.
pub trait Completer {
    fn complete(&self, query: &str) -> Vec<CompletionItem>;
}

// ── CommandCompleter ───────────────────────────────────────────────

/// Completes slash commands from a registry.
pub struct CommandCompleter {
    builtin_commands: Vec<SlashCommand>,
    extra_commands: Vec<SlashCommand>,
}

impl CommandCompleter {
    pub fn new(extra: Option<&[SlashCommand]>) -> Self {
        Self {
            builtin_commands: builtin_commands(),
            extra_commands: extra.map(|e| e.to_vec()).unwrap_or_default(),
        }
    }

    pub fn add_commands(&mut self, commands: &[SlashCommand]) {
        for cmd in commands {
            self.extra_commands.push(cmd.clone());
        }
    }

    fn all_commands(&self) -> impl Iterator<Item = &SlashCommand> {
        self.builtin_commands.iter().chain(self.extra_commands.iter())
    }

    /// Provide argument completions for a specific slash command.
    pub fn complete_args(&self, command: &str, query: &str) -> Vec<CompletionItem> {
        let candidates = match command {
            "mode" => vec![
                ("plan", "Read-only tools, planning mode"),
                ("normal", "Full tool access, normal mode"),
            ],
            "model" => vec![
                ("gpt-4o", "OpenAI GPT-4o"),
                ("gpt-4o-mini", "OpenAI GPT-4o Mini"),
                ("claude-sonnet-4", "Anthropic Claude Sonnet 4"),
                ("claude-3-opus", "Anthropic Claude 3 Opus"),
                ("claude-3-haiku", "Anthropic Claude 3 Haiku"),
                ("deepseek-chat", "DeepSeek Chat"),
            ],
            "memory" => vec![
                ("list", "List memory entries"),
                ("add", "Add a memory entry"),
                ("clear", "Clear all memories"),
            ],
            "theme" => vec![
                ("cyan", "Cyan theme"),
                ("blue", "Blue theme"),
                ("emerald", "Emerald theme"),
                ("amber", "Amber theme"),
                ("rose", "Rose theme"),
                ("slate", "Slate theme"),
            ],
            _ => vec![],
        };

        let query_lower = query.to_lowercase();
        candidates
            .into_iter()
            .filter(|(name, _)| name.starts_with(&query_lower))
            .map(|(name, desc)| CompletionItem {
                insert_text: name.to_string(),
                label: name.to_string(),
                description: desc.to_string(),
                kind: CompletionKind::Command,
                score: 0.0,
            })
            .collect()
    }
}

impl Completer for CommandCompleter {
    fn complete(&self, query: &str) -> Vec<CompletionItem> {
        let query_lower = query.to_lowercase();
        self.all_commands()
            .filter(|cmd| cmd.name.starts_with(&query_lower))
            .map(|cmd| CompletionItem {
                insert_text: format!("/{}", cmd.name),
                label: format!("/{}", cmd.name),
                description: cmd.description.to_string(),
                kind: CompletionKind::Command,
                score: 0.0,
            })
            .collect()
    }
}

// ── FileCompleter ──────────────────────────────────────────────────

/// Completes file paths relative to a working directory.
pub struct FileCompleter {
    finder: FileFinder,
}

impl FileCompleter {
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            finder: FileFinder::new(working_dir),
        }
    }
}

impl Completer for FileCompleter {
    fn complete(&self, query: &str) -> Vec<CompletionItem> {
        let paths = self.finder.find_files(query, 50);
        paths
            .into_iter()
            .map(|rel| {
                let is_dir = self.finder.working_dir().join(&rel).is_dir();
                let display = if is_dir {
                    format!("{}/", rel.display())
                } else {
                    rel.display().to_string()
                };
                CompletionItem {
                    insert_text: format!("@{}", display),
                    label: display,
                    description: if is_dir {
                        "dir".to_string()
                    } else {
                        super::formatters::CompletionFormatter::file_size_string(
                            &self.finder.working_dir().join(&rel),
                        )
                    },
                    kind: CompletionKind::File,
                    score: 0.0,
                }
            })
            .collect()
    }
}

// ── SymbolCompleter ────────────────────────────────────────────────

/// Placeholder completer for code symbols.
pub struct SymbolCompleter {
    symbols: Vec<(String, String)>,
}

impl SymbolCompleter {
    pub fn new() -> Self {
        Self { symbols: Vec::new() }
    }
}

impl Default for SymbolCompleter {
    fn default() -> Self { Self::new() }
}

impl Completer for SymbolCompleter {
    fn complete(&self, query: &str) -> Vec<CompletionItem> {
        let query_lower = query.to_lowercase();
        self.symbols
            .iter()
            .filter(|(name, _)| name.to_lowercase().contains(&query_lower))
            .map(|(name, kind)| CompletionItem {
                insert_text: name.clone(),
                label: name.clone(),
                description: kind.clone(),
                kind: CompletionKind::Symbol,
                score: 0.0,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_completer_basic() {
        let c = CommandCompleter::new(None);
        let results = c.complete("hel");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].insert_text, "/help");
    }

    #[test]
    fn test_command_completer_empty_query() {
        let c = CommandCompleter::new(None);
        let results = c.complete("");
        assert_eq!(results.len(), builtin_commands().len());
    }

    #[test]
    fn test_command_completer_no_match() {
        let c = CommandCompleter::new(None);
        let results = c.complete("zzzzz");
        assert!(results.is_empty());
    }

    #[test]
    fn test_arg_completion_mode() {
        let c = CommandCompleter::new(None);
        let results = c.complete_args("mode", "");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_arg_completion_mode_prefix() {
        let c = CommandCompleter::new(None);
        let results = c.complete_args("mode", "pl");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].label, "plan");
    }

    #[test]
    fn test_arg_completion_unknown_command() {
        let c = CommandCompleter::new(None);
        let results = c.complete_args("nonexistent", "");
        assert!(results.is_empty());
    }

    #[test]
    fn test_symbol_completer_empty() {
        let c = SymbolCompleter::new();
        let results = c.complete("anything");
        assert!(results.is_empty());
    }

    #[test]
    fn test_file_completer_in_temp_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("hello.txt"), "content").unwrap();
        let c = FileCompleter::new(dir.path().to_path_buf());
        let results = c.complete("hello");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].kind, CompletionKind::File);
        assert!(results[0].label.contains("hello.txt"));
    }
}
