use std::path::{Path, PathBuf};
use tracing::debug;

const BOOTSTRAP_MAX_CHARS: usize = 20_000;
const BOOTSTRAP_TOTAL_MAX_CHARS: usize = 50_000;
const BOOTSTRAP_MAX_FILES: usize = 10;

const BOOTSTRAP_FILENAMES: &[&str] = &[
    "AGENTS.md",
    "CLAUDE.md",
    "SOUL.md",
    "TOOLS.md",
    "IDENTITY.md",
    "BOOTSTRAP.md",
];

const CORE_INSTRUCTIONS: &str = r#"You are Octo, an AI coding assistant running inside a sandboxed environment.

You have access to tools for executing commands, reading and writing files, searching codebases, and browsing the web. Use these tools to help users with software engineering tasks.

## Guidelines
- Be concise, accurate, and helpful
- Always read files before suggesting modifications
- Use tools to verify your work
- Do not introduce security vulnerabilities
- Prefer editing existing files over creating new ones
"#;

const OUTPUT_GUIDELINES: &str = r#"## Output Format
- Use Markdown for formatting
- Include file paths when referencing code
- Use code blocks with language identifiers
"#;

#[derive(Debug, Clone)]
pub struct BootstrapFile {
    pub path: PathBuf,
    pub content: String,
    pub truncated: bool,
}

pub struct SystemPromptBuilder {
    core_instructions: String,
    bootstrap_files: Vec<BootstrapFile>,
    output_guidelines: String,
    extra_parts: Vec<String>,
}

impl SystemPromptBuilder {
    pub fn new() -> Self {
        Self {
            core_instructions: CORE_INSTRUCTIONS.to_string(),
            bootstrap_files: Vec::new(),
            output_guidelines: OUTPUT_GUIDELINES.to_string(),
            extra_parts: Vec::new(),
        }
    }

    /// Discover and load bootstrap files from a workspace directory.
    pub fn with_bootstrap_dir(mut self, workspace_dir: &Path) -> Self {
        self.bootstrap_files = Self::discover_bootstrap_files(workspace_dir);
        self
    }

    /// Add an extra section (e.g., working memory XML from Zone B).
    pub fn with_extra(mut self, part: String) -> Self {
        if !part.is_empty() {
            self.extra_parts.push(part);
        }
        self
    }

    /// Build the complete system prompt (Zone A).
    pub fn build_system_prompt(&self) -> String {
        let mut output = self.core_instructions.clone();

        if !self.bootstrap_files.is_empty() {
            output.push_str("\n\n");
            output.push_str(&self.format_bootstrap_section());
        }

        output.push_str("\n\n");
        output.push_str(&self.output_guidelines);

        for part in &self.extra_parts {
            output.push_str("\n\n");
            output.push_str(part);
        }

        output
    }

    /// Build Zone B dynamic context (date/time, session state, working memory).
    pub fn build_dynamic_context(
        datetime: &str,
        session_state: &str,
        working_memory_xml: &str,
    ) -> String {
        let mut parts = Vec::new();

        if !datetime.is_empty() {
            parts.push(datetime.to_string());
        }
        if !session_state.is_empty() {
            parts.push(session_state.to_string());
        }
        if !working_memory_xml.is_empty() {
            parts.push(working_memory_xml.to_string());
        }

        parts.join("\n\n")
    }

    fn format_bootstrap_section(&self) -> String {
        let mut section = String::from("## Project Context\n");
        for file in &self.bootstrap_files {
            let filename = file
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            section.push_str(&format!("\n### {filename}\n"));
            section.push_str(&file.content);
            if file.truncated {
                section.push_str(&format!(
                    "\n[... truncated at {BOOTSTRAP_MAX_CHARS} chars -- use file_read for full file]\n"
                ));
            }
            section.push('\n');
        }
        section
    }

    /// Scan workspace for bootstrap files, load and truncate them.
    pub fn discover_bootstrap_files(workspace_dir: &Path) -> Vec<BootstrapFile> {
        let mut files = Vec::new();
        let mut total_chars: usize = 0;

        for filename in BOOTSTRAP_FILENAMES {
            if files.len() >= BOOTSTRAP_MAX_FILES {
                break;
            }

            let path = workspace_dir.join(filename);
            if !path.exists() {
                continue;
            }

            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    let remaining = BOOTSTRAP_TOTAL_MAX_CHARS.saturating_sub(total_chars);
                    if remaining == 0 {
                        break;
                    }
                    let per_file_limit = remaining.min(BOOTSTRAP_MAX_CHARS);
                    let (truncated_content, was_truncated) =
                        Self::truncate_utf8(&content, per_file_limit);

                    total_chars += truncated_content.len();

                    debug!(
                        path = %path.display(),
                        original_len = content.len(),
                        truncated_len = truncated_content.len(),
                        was_truncated,
                        "Loaded bootstrap file"
                    );

                    files.push(BootstrapFile {
                        path,
                        content: truncated_content,
                        truncated: was_truncated,
                    });
                }
                Err(e) => {
                    debug!(path = %path.display(), error = %e, "Failed to read bootstrap file");
                }
            }
        }

        files
    }

    /// UTF-8 safe truncation with 70% head + 20% tail strategy.
    fn truncate_utf8(content: &str, max_chars: usize) -> (String, bool) {
        let char_count = content.chars().count();
        if char_count <= max_chars {
            return (content.to_string(), false);
        }

        let head_chars = (max_chars * 70) / 100;
        let tail_chars = (max_chars * 20) / 100;

        let head_end = content
            .char_indices()
            .nth(head_chars)
            .map(|(idx, _)| idx)
            .unwrap_or(content.len());

        let tail_start = {
            let skip = char_count.saturating_sub(tail_chars);
            content
                .char_indices()
                .nth(skip)
                .map(|(idx, _)| idx)
                .unwrap_or(content.len())
        };

        let omitted = char_count - head_chars - tail_chars;
        let result = format!(
            "{}\n\n[... omitted {} chars ...]\n\n{}",
            &content[..head_end],
            omitted,
            &content[tail_start..],
        );

        (result, true)
    }
}

impl Default for SystemPromptBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// Backward-compatible re-export: the old ContextBuilder interface
pub struct ContextBuilder {
    system_parts: Vec<String>,
}

impl ContextBuilder {
    pub fn new() -> Self {
        Self {
            system_parts: Vec::new(),
        }
    }

    pub fn with_memory(mut self, memory_xml: String) -> Self {
        if !memory_xml.is_empty() {
            self.system_parts.push(memory_xml);
        }
        self
    }

    pub fn with_instructions(mut self, instructions: String) -> Self {
        if !instructions.is_empty() {
            self.system_parts.push(instructions);
        }
        self
    }

    pub fn build_system_prompt(&self) -> String {
        self.system_parts.join("\n\n")
    }
}

impl Default for ContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Estimate total tokens used by messages (chars / 4 approximation)
pub fn estimate_messages_tokens(
    messages: &[octo_types::ChatMessage],
    tools: &[octo_types::ToolSpec],
) -> u32 {
    let msg_chars: usize = messages
        .iter()
        .map(|m| {
            m.content
                .iter()
                .map(|b| match b {
                    octo_types::ContentBlock::Text { text } => text.len(),
                    octo_types::ContentBlock::ToolUse { input, .. } => input.to_string().len(),
                    octo_types::ContentBlock::ToolResult { content, .. } => content.len(),
                })
                .sum::<usize>()
        })
        .sum();

    let tool_chars: usize = tools
        .iter()
        .map(|t| t.name.len() + t.description.len() + t.input_schema.to_string().len())
        .sum();

    ((msg_chars + tool_chars) / 4) as u32
}
