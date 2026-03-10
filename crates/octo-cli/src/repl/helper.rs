//! REPL helper providing completion, hints, and highlighting for slash commands

use std::borrow::Cow;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::Context;
use rustyline::Helper;

/// Known slash commands with descriptions
const SLASH_COMMANDS: &[(&str, &str)] = &[
    ("/help", "Show help information"),
    ("/compact", "Compress conversation context"),
    ("/undo", "Undo last tool operation"),
    ("/cost", "Show token usage and costs"),
    ("/model", "Switch LLM model"),
    ("/mode", "Switch mode (plan/build)"),
    ("/clear", "Clear current conversation"),
    ("/save", "Save session"),
    ("/theme", "Switch color theme"),
    ("/exit", "Exit REPL"),
    ("/memory", "Manage auto-memory"),
];

/// REPL helper that provides tab completion, inline hints, and prompt highlighting
/// for slash commands in the interactive REPL.
pub struct ReplHelper {
    /// Whether the agent is currently streaming a response
    pub is_streaming: Arc<AtomicBool>,
}

impl Completer for ReplHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        if !line.starts_with('/') {
            return Ok((pos, vec![]));
        }

        let prefix = &line[..pos];
        let candidates: Vec<Pair> = SLASH_COMMANDS
            .iter()
            .filter(|(cmd, _)| cmd.starts_with(prefix))
            .map(|(cmd, desc)| Pair {
                display: format!("{cmd}  — {desc}"),
                replacement: cmd.to_string(),
            })
            .collect();

        Ok((0, candidates))
    }
}

impl Hinter for ReplHelper {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Option<String> {
        // Only show hints when cursor is at end of input and line starts with /
        if !line.starts_with('/') || pos != line.len() {
            return None;
        }

        SLASH_COMMANDS
            .iter()
            .find(|(cmd, _)| cmd.starts_with(line) && *cmd != line)
            .map(|(cmd, _)| cmd[line.len()..].to_string())
    }
}

impl Highlighter for ReplHelper {
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        _default: bool,
    ) -> Cow<'b, str> {
        // Cyan prompt: \x1b[36m ... \x1b[0m
        Cow::Owned(format!("\x1b[36m{prompt}\x1b[0m"))
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        // Dim hint: \x1b[2m ... \x1b[0m
        Cow::Owned(format!("\x1b[2m{hint}\x1b[0m"))
    }
}

impl Validator for ReplHelper {}

impl Helper for ReplHelper {}
