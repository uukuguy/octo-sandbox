# Phase 2: Context Engineering + Built-in Tools Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement the context engineering architecture (three-zone model, progressive degradation, tool result defense) and 5 new built-in tools (file_write, file_edit, grep, glob, find) to give the agent full computer-operation capability.

**Architecture:** Refactor the existing flat memory/budget modules into a dedicated `context` module with three components: ContextBudgetManager (budget calculation + degradation decisions), ContextPruner (three-level degradation execution), and SystemPromptBuilder (zone A + B assembly). Add 5 new tools following the existing Tool trait pattern. Integrate everything through the AgentLoop.

**Tech Stack:** Rust (tokio, serde_json, anyhow, async-trait, tracing), existing octo-types + octo-engine crates

**Design Reference:** `docs/design/CONTEXT_ENGINEERING_DESIGN.md`

---

## Task 1: Add MemoryBlock priority and auto-expire fields to octo-types

**Files:**
- Modify: `crates/octo-types/src/memory.rs`

**Step 1: Update MemoryBlock and MemoryBlockKind**

```rust
// crates/octo-types/src/memory.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryBlockKind {
    SandboxContext,
    AgentPersona,
    UserProfile,
    TaskContext,
    AutoExtracted, // NEW: from Memory Flush
    Custom,        // NEW: agent-created blocks
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryBlock {
    pub id: String,
    pub kind: MemoryBlockKind,
    pub label: String,
    pub value: String,
    pub priority: u8,          // NEW: 0-255, higher = more important
    pub max_age_turns: Option<u32>, // NEW: auto-expire after N turns without update
    pub last_updated_turn: u32,     // NEW: track when last modified
}

impl MemoryBlock {
    pub fn new(kind: MemoryBlockKind, label: impl Into<String>, value: impl Into<String>) -> Self {
        let kind_str = match &kind {
            MemoryBlockKind::SandboxContext => "sandbox_context",
            MemoryBlockKind::AgentPersona => "agent_persona",
            MemoryBlockKind::UserProfile => "user_profile",
            MemoryBlockKind::TaskContext => "task_context",
            MemoryBlockKind::AutoExtracted => "auto_extracted",
            MemoryBlockKind::Custom => "custom",
        };
        Self {
            id: kind_str.to_string(),
            kind,
            label: label.into(),
            value: value.into(),
            priority: 128,           // default mid-priority
            max_age_turns: None,     // no expiry by default
            last_updated_turn: 0,
        }
    }

    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_max_age(mut self, turns: u32) -> Self {
        self.max_age_turns = Some(turns);
        self
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn char_count(&self) -> usize {
        self.value.len()
    }

    pub fn is_expired(&self, current_turn: u32) -> bool {
        match self.max_age_turns {
            Some(max) => current_turn.saturating_sub(self.last_updated_turn) > max,
            None => false,
        }
    }
}
```

**Step 2: Update TokenBudget to match new ContextBudgetManager design**

Keep the existing `TokenBudget` struct unchanged for now (backward compat). We will create the new `ContextBudgetManager` in Task 3.

**Step 3: Verify compilation**

Run: `cargo check -p octo-types`
Expected: PASS (with possible warnings about new unused fields)

**Step 4: Fix any downstream compilation errors in octo-engine**

The `InMemoryWorkingMemory::new()` in `crates/octo-engine/src/memory/working.rs` constructs `MemoryBlock` — update to include new fields. The `MemoryBlock::new()` constructor handles defaults, so existing calls should still compile. Verify with:

Run: `cargo check --workspace`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/octo-types/src/memory.rs
git commit -m "feat(types): add priority, max_age_turns, last_updated_turn to MemoryBlock"
```

---

## Task 2: Refactor ContextBuilder into SystemPromptBuilder with Bootstrap support

**Files:**
- Create: `crates/octo-engine/src/context/mod.rs`
- Create: `crates/octo-engine/src/context/builder.rs`
- Modify: `crates/octo-engine/src/agent/loop_.rs` (update import)
- Modify: `crates/octo-engine/src/lib.rs` (add context module)
- Delete content from: `crates/octo-engine/src/agent/context.rs` (re-export from context module)

**Step 1: Create `crates/octo-engine/src/context/mod.rs`**

```rust
pub mod builder;

pub use builder::{BootstrapFile, SystemPromptBuilder};
```

**Step 2: Create `crates/octo-engine/src/context/builder.rs`**

```rust
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
    /// This is for backward compat with the old ContextBuilder pattern.
    pub fn with_extra(mut self, part: String) -> Self {
        if !part.is_empty() {
            self.extra_parts.push(part);
        }
        self
    }

    /// Build the complete system prompt (Zone A).
    pub fn build_system_prompt(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();

        // 1. Core instructions
        parts.push(&self.core_instructions);

        // 2. Bootstrap files
        // (collected into a temp string below)

        // 3. Output guidelines
        parts.push(&self.output_guidelines);

        let mut output = parts.join("\n\n");

        // Insert bootstrap files after core instructions
        if !self.bootstrap_files.is_empty() {
            let bootstrap_section = self.format_bootstrap_section();
            output = format!(
                "{}\n\n{}\n\n{}",
                self.core_instructions, bootstrap_section, self.output_guidelines
            );
        }

        // Extra parts (working memory, etc.)
        for part in &self.extra_parts {
            output.push_str("\n\n");
            output.push_str(part);
        }

        output
    }

    /// Build Zone B dynamic context (date/time, session state, working memory).
    /// This is prepended to the user message, not in system prompt.
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
                    "\n[... truncated at {BOOTSTRAP_MAX_CHARS} chars — use file_read for full file]\n"
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
```

**Step 3: Update `agent/context.rs` to re-export from context module**

Replace the entire file content with:

```rust
// Backward compatibility: re-export from context module
pub use crate::context::builder::{ContextBuilder, estimate_messages_tokens};
```

**Step 4: Update `crates/octo-engine/src/lib.rs`**

```rust
pub mod agent;
pub mod context;  // NEW
pub mod memory;
pub mod providers;
pub mod tools;

pub use agent::{AgentEvent, AgentLoop};
pub use context::{BootstrapFile, SystemPromptBuilder};  // NEW
pub use memory::{InMemoryWorkingMemory, TokenBudgetManager, WorkingMemory};
pub use providers::{create_anthropic_provider, create_openai_provider, create_provider, Provider};
pub use tools::{default_tools, Tool, ToolRegistry};
```

**Step 5: Verify compilation**

Run: `cargo check --workspace`
Expected: PASS

**Step 6: Commit**

```bash
git add crates/octo-engine/src/context/ crates/octo-engine/src/agent/context.rs crates/octo-engine/src/lib.rs
git commit -m "refactor(engine): extract SystemPromptBuilder with bootstrap file support"
```

---

## Task 3: Implement ContextBudgetManager with dual-track estimation

**Files:**
- Create: `crates/octo-engine/src/context/budget.rs`
- Modify: `crates/octo-engine/src/context/mod.rs`

**Step 1: Create `crates/octo-engine/src/context/budget.rs`**

```rust
use octo_types::{ChatMessage, ContentBlock, ToolSpec};

const CHARS_PER_TOKEN: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DegradationLevel {
    /// < 60% usage: no pruning needed
    None,
    /// 60%-80%: soft-trim old tool results (head+tail)
    SoftTrim,
    /// 80%-90%: hard-clear old tool results (placeholder only)
    HardClear,
    /// > 90%: full compaction (memory flush + summarize + replace)
    Compact,
}

pub struct ContextBudgetManager {
    /// Model context window in tokens
    context_window: u32,
    /// Reserved for model output (default: 8192)
    output_reserve: u32,
    /// Safety margin (default: 2048)
    safety_margin: u32,
    /// Last actual input_tokens from API response (if available)
    last_actual_usage: Option<u64>,
    /// Message count when last_actual_usage was recorded
    last_usage_msg_count: usize,
}

impl ContextBudgetManager {
    pub fn new(context_window: u32) -> Self {
        Self {
            context_window,
            output_reserve: 8192,
            safety_margin: 2048,
            last_actual_usage: None,
            last_usage_msg_count: 0,
        }
    }

    pub fn with_output_reserve(mut self, reserve: u32) -> Self {
        self.output_reserve = reserve;
        self
    }

    /// Update with actual token usage from API response.
    /// Called after each successful API call.
    pub fn update_actual_usage(&mut self, input_tokens: u32, msg_count: usize) {
        self.last_actual_usage = Some(input_tokens as u64);
        self.last_usage_msg_count = msg_count;
    }

    /// Estimate tokens for a string using chars/4 approximation.
    pub fn estimate_tokens(text: &str) -> u32 {
        (text.len() / CHARS_PER_TOKEN) as u32
    }

    /// Estimate tokens for all messages.
    pub fn estimate_messages_tokens(messages: &[ChatMessage]) -> u64 {
        let chars: usize = messages
            .iter()
            .map(|m| {
                m.content
                    .iter()
                    .map(|b| match b {
                        ContentBlock::Text { text } => text.len(),
                        ContentBlock::ToolUse { input, name, id } => {
                            name.len() + id.len() + input.to_string().len()
                        }
                        ContentBlock::ToolResult { content, .. } => content.len(),
                    })
                    .sum::<usize>()
            })
            .sum();
        (chars / CHARS_PER_TOKEN) as u64
    }

    /// Estimate tokens for tool specs (they count against context window).
    pub fn estimate_tool_specs_tokens(tools: &[ToolSpec]) -> u64 {
        let chars: usize = tools
            .iter()
            .map(|t| t.name.len() + t.description.len() + t.input_schema.to_string().len())
            .sum();
        (chars / CHARS_PER_TOKEN) as u64
    }

    /// Compute total estimated context usage using dual-track estimation.
    ///
    /// Track 1 (preferred): Use last actual API usage + estimate for new messages since then.
    /// Track 2 (fallback): Pure chars/4 estimation for everything.
    pub fn estimate_total_usage(
        &self,
        system_prompt: &str,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
    ) -> u64 {
        // If we have actual usage data, use it as baseline
        if let Some(actual) = self.last_actual_usage {
            // actual covers: system_prompt + tools + messages[0..last_usage_msg_count]
            // We need to add estimate for messages added since then
            if messages.len() > self.last_usage_msg_count {
                let new_messages = &messages[self.last_usage_msg_count..];
                let new_tokens = Self::estimate_messages_tokens(new_messages);
                return actual + new_tokens;
            }
            return actual;
        }

        // Fallback: estimate everything
        let system_tokens = Self::estimate_tokens(system_prompt) as u64;
        let msg_tokens = Self::estimate_messages_tokens(messages);
        let tool_tokens = Self::estimate_tool_specs_tokens(tools);

        system_tokens + msg_tokens + tool_tokens
    }

    /// Available space for content (total - output_reserve - safety_margin).
    pub fn available_space(&self) -> u64 {
        (self.context_window as u64)
            .saturating_sub(self.output_reserve as u64)
            .saturating_sub(self.safety_margin as u64)
    }

    /// Compute usage ratio (0.0 - 1.0+).
    pub fn usage_ratio(
        &self,
        system_prompt: &str,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
    ) -> f64 {
        let used = self.estimate_total_usage(system_prompt, messages, tools);
        let available = self.available_space();
        if available == 0 {
            return 1.0;
        }
        used as f64 / available as f64
    }

    /// Determine the degradation level based on current usage.
    pub fn compute_degradation_level(
        &self,
        system_prompt: &str,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
    ) -> DegradationLevel {
        let ratio = self.usage_ratio(system_prompt, messages, tools);
        match ratio {
            r if r < 0.60 => DegradationLevel::None,
            r if r < 0.80 => DegradationLevel::SoftTrim,
            r if r < 0.90 => DegradationLevel::HardClear,
            _ => DegradationLevel::Compact,
        }
    }

    pub fn context_window(&self) -> u32 {
        self.context_window
    }
}

impl Default for ContextBudgetManager {
    fn default() -> Self {
        Self::new(200_000)
    }
}
```

**Step 2: Update `context/mod.rs`**

```rust
pub mod budget;
pub mod builder;

pub use budget::{ContextBudgetManager, DegradationLevel};
pub use builder::{BootstrapFile, ContextBuilder, SystemPromptBuilder, estimate_messages_tokens};
```

**Step 3: Update `lib.rs` to export new types**

Add to the re-exports in `crates/octo-engine/src/lib.rs`:
```rust
pub use context::{BootstrapFile, ContextBudgetManager, DegradationLevel, SystemPromptBuilder};
```

**Step 4: Verify compilation**

Run: `cargo check --workspace`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/octo-engine/src/context/budget.rs crates/octo-engine/src/context/mod.rs crates/octo-engine/src/lib.rs
git commit -m "feat(engine): add ContextBudgetManager with dual-track token estimation"
```

---

## Task 4: Implement ContextPruner with three-level degradation

**Files:**
- Create: `crates/octo-engine/src/context/pruner.rs`
- Modify: `crates/octo-engine/src/context/mod.rs`

**Step 1: Create `crates/octo-engine/src/context/pruner.rs`**

```rust
use octo_types::{ChatMessage, ContentBlock, MessageRole};
use tracing::{debug, info};

use super::budget::DegradationLevel;

const SOFT_TRIM_HEAD: usize = 1_500;
const SOFT_TRIM_TAIL: usize = 500;
const TOOL_INPUT_SUMMARY_MAX: usize = 200;

/// Prunes conversation history based on degradation level.
/// Does NOT modify the most recent `protect_recent_rounds` rounds.
pub struct ContextPruner {
    /// Number of recent agent rounds to protect from pruning.
    /// A "round" = user message + assistant response (possibly with tool calls).
    protect_recent_rounds: usize,
}

impl ContextPruner {
    pub fn new() -> Self {
        Self {
            protect_recent_rounds: 2,
        }
    }

    /// Apply degradation to messages in-place.
    /// Returns the number of content blocks modified.
    pub fn apply(&self, messages: &mut Vec<ChatMessage>, level: DegradationLevel) -> usize {
        match level {
            DegradationLevel::None => 0,
            DegradationLevel::SoftTrim => self.soft_trim(messages),
            DegradationLevel::HardClear => self.hard_clear(messages),
            DegradationLevel::Compact => {
                // Compact is handled externally (requires LLM call for summarization).
                // Here we just do HardClear as a pre-step.
                self.hard_clear(messages)
            }
        }
    }

    /// Level 1: Soft-trim old tool results (keep head + tail).
    fn soft_trim(&self, messages: &mut Vec<ChatMessage>) -> usize {
        let boundary = self.find_protection_boundary(messages);
        let mut modified = 0;

        for msg in messages[..boundary].iter_mut() {
            for block in msg.content.iter_mut() {
                if let ContentBlock::ToolResult { content, .. } = block {
                    if content.len() > (SOFT_TRIM_HEAD + SOFT_TRIM_TAIL + 100) {
                        let (head, tail) = Self::head_tail_utf8(content, SOFT_TRIM_HEAD, SOFT_TRIM_TAIL);
                        let omitted = content.len() - SOFT_TRIM_HEAD - SOFT_TRIM_TAIL;
                        *content = format!(
                            "{}\n\n[... omitted {} chars ...]\n\n{}",
                            head, omitted, tail
                        );
                        modified += 1;
                    }
                }
            }
        }

        if modified > 0 {
            debug!(modified, "Soft-trimmed tool results");
        }
        modified
    }

    /// Level 2: Hard-clear old tool results (replace with placeholder).
    fn hard_clear(&self, messages: &mut Vec<ChatMessage>) -> usize {
        let boundary = self.find_protection_boundary(messages);
        let mut modified = 0;

        for msg in messages[..boundary].iter_mut() {
            for block in msg.content.iter_mut() {
                if let ContentBlock::ToolResult {
                    content,
                    tool_use_id,
                    ..
                } = block
                {
                    if content.len() > 100 {
                        // Find the matching ToolUse to get name + input summary
                        *content = format!(
                            "[Tool result omitted, tool_use_id={}]",
                            tool_use_id
                        );
                        modified += 1;
                    }
                }
            }
        }

        if modified > 0 {
            info!(modified, "Hard-cleared tool results");
        }
        modified
    }

    /// Find the message index before which we can prune.
    /// Protects the last N "rounds" (user+assistant pairs).
    fn find_protection_boundary(&self, messages: &[ChatMessage]) -> usize {
        if messages.is_empty() {
            return 0;
        }

        // Count rounds backwards from the end.
        // A round boundary is a User message that is NOT a tool result.
        let mut rounds_found = 0;
        let mut boundary = messages.len();

        for (i, msg) in messages.iter().enumerate().rev() {
            if msg.role == MessageRole::User {
                // Check if this is a "real" user message (not tool results)
                let is_tool_result_msg = msg.content.iter().all(|b| {
                    matches!(b, ContentBlock::ToolResult { .. })
                });
                if !is_tool_result_msg {
                    rounds_found += 1;
                    if rounds_found > self.protect_recent_rounds {
                        boundary = i;
                        break;
                    }
                }
            }
        }

        if rounds_found <= self.protect_recent_rounds {
            // Not enough rounds to prune anything
            return 0;
        }

        boundary
    }

    /// Find safe compaction boundary (not in the middle of a tool chain).
    /// Returns the index at which to split: messages[..index] will be summarized.
    pub fn find_compaction_boundary(messages: &[ChatMessage], min_keep_chars: usize) -> usize {
        if messages.is_empty() {
            return 0;
        }

        // Accumulate chars backwards to find where min_keep_chars is reached.
        let mut kept_chars: usize = 0;
        let mut candidate_boundary = 0;

        for (i, msg) in messages.iter().enumerate().rev() {
            let msg_chars: usize = msg.content.iter().map(|b| match b {
                ContentBlock::Text { text } => text.len(),
                ContentBlock::ToolUse { input, .. } => input.to_string().len(),
                ContentBlock::ToolResult { content, .. } => content.len(),
            }).sum();

            kept_chars += msg_chars;

            if kept_chars >= min_keep_chars {
                // Found a candidate. Now find the nearest safe boundary.
                candidate_boundary = i;
                break;
            }
        }

        // Walk forward from candidate to find a safe boundary:
        // Safe = right after an Assistant message that contains Text (not just ToolUse).
        for i in candidate_boundary..messages.len() {
            if messages[i].role == MessageRole::Assistant {
                let has_text = messages[i].content.iter().any(|b| matches!(b, ContentBlock::Text { .. }));
                let has_only_tool_use = messages[i].content.iter().all(|b| matches!(b, ContentBlock::ToolUse { .. }));
                if has_text && !has_only_tool_use {
                    // Safe to cut after this message
                    return (i + 1).min(messages.len());
                }
            }
        }

        // Fallback: cut at candidate
        candidate_boundary
    }

    /// UTF-8 safe head+tail extraction.
    fn head_tail_utf8(s: &str, head_chars: usize, tail_chars: usize) -> (String, String) {
        let head_end = s
            .char_indices()
            .nth(head_chars)
            .map(|(idx, _)| idx)
            .unwrap_or(s.len());

        let char_count = s.chars().count();
        let tail_start_char = char_count.saturating_sub(tail_chars);
        let tail_start = s
            .char_indices()
            .nth(tail_start_char)
            .map(|(idx, _)| idx)
            .unwrap_or(s.len());

        (s[..head_end].to_string(), s[tail_start..].to_string())
    }
}

impl Default for ContextPruner {
    fn default() -> Self {
        Self::new()
    }
}
```

**Step 2: Update `context/mod.rs`**

```rust
pub mod budget;
pub mod builder;
pub mod pruner;

pub use budget::{ContextBudgetManager, DegradationLevel};
pub use builder::{BootstrapFile, ContextBuilder, SystemPromptBuilder, estimate_messages_tokens};
pub use pruner::ContextPruner;
```

**Step 3: Update `lib.rs` exports**

Add `ContextPruner` to the re-exports.

**Step 4: Verify compilation**

Run: `cargo check --workspace`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/octo-engine/src/context/pruner.rs crates/octo-engine/src/context/mod.rs crates/octo-engine/src/lib.rs
git commit -m "feat(engine): add ContextPruner with three-level progressive degradation"
```

---

## Task 5: Integrate ContextBudgetManager + ContextPruner into AgentLoop

**Files:**
- Modify: `crates/octo-engine/src/agent/loop_.rs`
- Modify: `crates/octo-types/src/provider.rs` (TokenUsage needs to be accessible)

**Step 1: Update AgentLoop to use ContextBudgetManager**

Key changes to `loop_.rs`:

1. Add `ContextBudgetManager` and `ContextPruner` as fields of `AgentLoop`
2. Before each round, call `budget.compute_degradation_level()` and `pruner.apply()`
3. After each `MessageStop`, call `budget.update_actual_usage()`
4. Use `SystemPromptBuilder` instead of raw `ContextBuilder`

The modified `AgentLoop` struct:

```rust
use crate::context::{ContextBudgetManager, ContextPruner, DegradationLevel, SystemPromptBuilder};

pub struct AgentLoop {
    provider: Arc<dyn Provider>,
    tools: Arc<ToolRegistry>,
    memory: Arc<dyn WorkingMemory>,
    model: String,
    max_tokens: u32,
    budget: ContextBudgetManager,   // NEW
    pruner: ContextPruner,          // NEW
}
```

In `AgentLoop::new()`, initialize budget and pruner with defaults.

In the `run()` method, add before the provider call each round:

```rust
// Apply context pruning based on budget
let level = self.budget.compute_degradation_level(
    &system_prompt,
    messages,
    &tool_specs,
);
if level != DegradationLevel::None {
    debug!(?level, "Applying context degradation");
    self.pruner.apply(messages, level);
}
```

After receiving `MessageStop`:

```rust
Ok(StreamEvent::MessageStop { stop_reason, usage }) => {
    // Update budget with actual usage
    self.budget.update_actual_usage(usage.input_tokens, messages.len());
    // ... rest of existing handling
}
```

**Step 2: Verify compilation**

Run: `cargo check --workspace`
Expected: PASS

**Step 3: Commit**

```bash
git add crates/octo-engine/src/agent/loop_.rs
git commit -m "feat(engine): integrate ContextBudgetManager and ContextPruner into AgentLoop"
```

---

## Task 6: Implement FileWriteTool

**Files:**
- Create: `crates/octo-engine/src/tools/file_write.rs`
- Modify: `crates/octo-engine/src/tools/mod.rs`

**Step 1: Implement FileWriteTool**

```rust
// crates/octo-engine/src/tools/file_write.rs
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::debug;

use octo_types::{ToolContext, ToolResult, ToolSource};
use super::traits::Tool;

pub struct FileWriteTool;

impl FileWriteTool {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl Tool for FileWriteTool {
    fn name(&self) -> &str { "file_write" }

    fn description(&self) -> &str {
        "Write content to a file. Creates the file if it doesn't exist, or overwrites it if it does. Creates parent directories as needed."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File path (absolute or relative to working directory)"
                },
                "content": {
                    "type": "string",
                    "description": "The full content to write to the file"
                }
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let path_str = params["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'path' parameter"))?;
        let content = params["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'content' parameter"))?;

        let path = if std::path::Path::new(path_str).is_absolute() {
            std::path::PathBuf::from(path_str)
        } else {
            ctx.working_dir.join(path_str)
        };

        debug!(?path, content_len = content.len(), "writing file");

        // Create parent directories
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }

        match tokio::fs::write(&path, content).await {
            Ok(()) => Ok(ToolResult::success(format!(
                "Wrote {} bytes to {}",
                content.len(),
                path.display()
            ))),
            Err(e) => Ok(ToolResult::error(format!("Failed to write file: {e}"))),
        }
    }

    fn source(&self) -> ToolSource { ToolSource::BuiltIn }
}
```

**Step 2: Register in `tools/mod.rs`**

Add `pub mod file_write;` and register `FileWriteTool::new()` in `default_tools()`.

**Step 3: Verify compilation**

Run: `cargo check --workspace`
Expected: PASS

**Step 4: Commit**

```bash
git add crates/octo-engine/src/tools/file_write.rs crates/octo-engine/src/tools/mod.rs
git commit -m "feat(tools): add FileWriteTool for creating and overwriting files"
```

---

## Task 7: Implement FileEditTool

**Files:**
- Create: `crates/octo-engine/src/tools/file_edit.rs`
- Modify: `crates/octo-engine/src/tools/mod.rs`

**Step 1: Implement FileEditTool**

The tool performs exact string replacement in files (like Claude Code's Edit tool):

```rust
// crates/octo-engine/src/tools/file_edit.rs
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::debug;

use octo_types::{ToolContext, ToolResult, ToolSource};
use super::traits::Tool;

pub struct FileEditTool;

impl FileEditTool {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl Tool for FileEditTool {
    fn name(&self) -> &str { "file_edit" }

    fn description(&self) -> &str {
        "Edit a file by replacing an exact string match with new content. The old_string must appear exactly once in the file (unless replace_all is true)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File path (absolute or relative to working directory)"
                },
                "old_string": {
                    "type": "string",
                    "description": "The exact string to find and replace"
                },
                "new_string": {
                    "type": "string",
                    "description": "The replacement string"
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "Replace all occurrences (default: false)"
                }
            },
            "required": ["path", "old_string", "new_string"]
        })
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let path_str = params["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'path' parameter"))?;
        let old_string = params["old_string"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'old_string' parameter"))?;
        let new_string = params["new_string"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'new_string' parameter"))?;
        let replace_all = params["replace_all"].as_bool().unwrap_or(false);

        let path = if std::path::Path::new(path_str).is_absolute() {
            std::path::PathBuf::from(path_str)
        } else {
            ctx.working_dir.join(path_str)
        };

        debug!(?path, old_len = old_string.len(), new_len = new_string.len(), "editing file");

        if !path.exists() {
            return Ok(ToolResult::error(format!("File not found: {}", path.display())));
        }

        let content = match tokio::fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => return Ok(ToolResult::error(format!("Failed to read file: {e}"))),
        };

        let count = content.matches(old_string).count();
        if count == 0 {
            return Ok(ToolResult::error(
                "old_string not found in file. Make sure it matches exactly (including whitespace and indentation)."
                .to_string()
            ));
        }

        if !replace_all && count > 1 {
            return Ok(ToolResult::error(format!(
                "old_string found {count} times. Provide more context to make it unique, or set replace_all=true."
            )));
        }

        let new_content = if replace_all {
            content.replace(old_string, new_string)
        } else {
            content.replacen(old_string, new_string, 1)
        };

        match tokio::fs::write(&path, &new_content).await {
            Ok(()) => Ok(ToolResult::success(format!(
                "Replaced {} occurrence(s) in {}",
                if replace_all { count } else { 1 },
                path.display()
            ))),
            Err(e) => Ok(ToolResult::error(format!("Failed to write file: {e}"))),
        }
    }

    fn source(&self) -> ToolSource { ToolSource::BuiltIn }
}
```

**Step 2: Register in `tools/mod.rs`**

**Step 3: Verify compilation, commit**

```bash
git add crates/octo-engine/src/tools/file_edit.rs crates/octo-engine/src/tools/mod.rs
git commit -m "feat(tools): add FileEditTool for exact string replacement in files"
```

---

## Task 8: Implement GrepTool

**Files:**
- Create: `crates/octo-engine/src/tools/grep.rs`
- Modify: `crates/octo-engine/src/tools/mod.rs`

**Step 1: Implement GrepTool**

Uses `tokio::process::Command` to run `grep -rn` (no external crate dependency):

```rust
// crates/octo-engine/src/tools/grep.rs
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::debug;

use octo_types::{ToolContext, ToolResult, ToolSource};
use super::traits::Tool;

const MAX_RESULTS: usize = 100;

pub struct GrepTool;

impl GrepTool {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str { "grep" }

    fn description(&self) -> &str {
        "Search for a pattern in files using regex. Returns matching lines with file paths and line numbers. Searches recursively in the working directory by default."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The regex pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "Directory or file to search in (default: working directory)"
                },
                "include": {
                    "type": "string",
                    "description": "Glob pattern to filter files (e.g. '*.rs', '*.py')"
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let pattern = params["pattern"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'pattern' parameter"))?;

        let search_path = params["path"]
            .as_str()
            .map(|p| {
                if std::path::Path::new(p).is_absolute() {
                    std::path::PathBuf::from(p)
                } else {
                    ctx.working_dir.join(p)
                }
            })
            .unwrap_or_else(|| ctx.working_dir.clone());

        let include = params["include"].as_str();

        debug!(?pattern, ?search_path, ?include, "running grep");

        let mut cmd = tokio::process::Command::new("grep");
        cmd.arg("-rn")  // recursive + line numbers
           .arg("-E")   // extended regex
           .arg("--color=never");

        if let Some(glob) = include {
            cmd.arg("--include").arg(glob);
        }

        cmd.arg("--").arg(pattern).arg(&search_path);

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            cmd.output(),
        )
        .await;

        match output {
            Ok(Ok(out)) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let lines: Vec<&str> = stdout.lines().collect();
                let total = lines.len();

                let result_text = if total > MAX_RESULTS {
                    let truncated: String = lines[..MAX_RESULTS].join("\n");
                    format!("{truncated}\n\n[... {total} total matches, showing first {MAX_RESULTS}]")
                } else if total == 0 {
                    "No matches found.".to_string()
                } else {
                    lines.join("\n")
                };

                Ok(ToolResult::success(result_text))
            }
            Ok(Err(e)) => Ok(ToolResult::error(format!("grep failed: {e}"))),
            Err(_) => Ok(ToolResult::error("grep timed out after 30 seconds".to_string())),
        }
    }

    fn source(&self) -> ToolSource { ToolSource::BuiltIn }
}
```

**Step 2: Register in `tools/mod.rs`**

**Step 3: Verify compilation, commit**

```bash
git add crates/octo-engine/src/tools/grep.rs crates/octo-engine/src/tools/mod.rs
git commit -m "feat(tools): add GrepTool for recursive regex search in files"
```

---

## Task 9: Implement GlobTool

**Files:**
- Create: `crates/octo-engine/src/tools/glob.rs`
- Modify: `crates/octo-engine/src/tools/mod.rs`
- Modify: `crates/octo-engine/Cargo.toml` (add `glob` dependency)

**Step 1: Add glob dependency**

Add to `crates/octo-engine/Cargo.toml`:
```toml
glob = "0.3"
```

**Step 2: Implement GlobTool**

```rust
// crates/octo-engine/src/tools/glob.rs
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::debug;

use octo_types::{ToolContext, ToolResult, ToolSource};
use super::traits::Tool;

const MAX_RESULTS: usize = 200;

pub struct GlobTool;

impl GlobTool {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str { "glob" }

    fn description(&self) -> &str {
        "Find files matching a glob pattern. Returns file paths sorted by modification time (newest first). Useful for discovering files by name or extension."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern (e.g. '**/*.rs', 'src/**/*.ts', '*.json')"
                },
                "path": {
                    "type": "string",
                    "description": "Base directory for the pattern (default: working directory)"
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let pattern = params["pattern"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'pattern' parameter"))?;

        let base_dir = params["path"]
            .as_str()
            .map(|p| {
                if std::path::Path::new(p).is_absolute() {
                    std::path::PathBuf::from(p)
                } else {
                    ctx.working_dir.join(p)
                }
            })
            .unwrap_or_else(|| ctx.working_dir.clone());

        let full_pattern = base_dir.join(pattern);
        let pattern_str = full_pattern.to_string_lossy().to_string();

        debug!(?pattern_str, "running glob");

        // Run in blocking task since glob is synchronous
        let result = tokio::task::spawn_blocking(move || {
            let mut entries: Vec<(std::path::PathBuf, std::time::SystemTime)> = Vec::new();

            match glob::glob(&pattern_str) {
                Ok(paths) => {
                    for entry in paths.flatten() {
                        let mtime = entry
                            .metadata()
                            .and_then(|m| m.modified())
                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                        entries.push((entry, mtime));
                    }
                }
                Err(e) => return Err(format!("Invalid glob pattern: {e}")),
            }

            // Sort by modification time, newest first
            entries.sort_by(|a, b| b.1.cmp(&a.1));

            Ok(entries)
        })
        .await
        .map_err(|e| anyhow::anyhow!("glob task failed: {e}"))?;

        match result {
            Ok(entries) => {
                let total = entries.len();
                let display_entries = &entries[..total.min(MAX_RESULTS)];

                let output: String = display_entries
                    .iter()
                    .map(|(p, _)| p.to_string_lossy().to_string())
                    .collect::<Vec<_>>()
                    .join("\n");

                let result_text = if total > MAX_RESULTS {
                    format!("{output}\n\n[... {total} total matches, showing first {MAX_RESULTS}]")
                } else if total == 0 {
                    "No files found matching pattern.".to_string()
                } else {
                    format!("{output}\n\n[{total} files found]")
                };

                Ok(ToolResult::success(result_text))
            }
            Err(e) => Ok(ToolResult::error(e)),
        }
    }

    fn source(&self) -> ToolSource { ToolSource::BuiltIn }
}
```

**Step 3: Register in `tools/mod.rs`**

**Step 4: Verify compilation, commit**

```bash
git add crates/octo-engine/src/tools/glob.rs crates/octo-engine/src/tools/mod.rs crates/octo-engine/Cargo.toml
git commit -m "feat(tools): add GlobTool for file pattern matching"
```

---

## Task 10: Implement FindTool

**Files:**
- Create: `crates/octo-engine/src/tools/find.rs`
- Modify: `crates/octo-engine/src/tools/mod.rs`

**Step 1: Implement FindTool**

Wraps the system `find` command:

```rust
// crates/octo-engine/src/tools/find.rs
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::debug;

use octo_types::{ToolContext, ToolResult, ToolSource};
use super::traits::Tool;

const MAX_RESULTS: usize = 200;

pub struct FindTool;

impl FindTool {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl Tool for FindTool {
    fn name(&self) -> &str { "find" }

    fn description(&self) -> &str {
        "Search for files and directories by name pattern. Uses the system find command. Good for locating files when you know part of the name."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory to search in (default: working directory)"
                },
                "name": {
                    "type": "string",
                    "description": "File name pattern (supports wildcards: *.rs, test_*)"
                },
                "type": {
                    "type": "string",
                    "enum": ["f", "d"],
                    "description": "Type filter: 'f' for files, 'd' for directories"
                }
            },
            "required": ["name"]
        })
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let name = params["name"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'name' parameter"))?;

        let search_path = params["path"]
            .as_str()
            .map(|p| {
                if std::path::Path::new(p).is_absolute() {
                    std::path::PathBuf::from(p)
                } else {
                    ctx.working_dir.join(p)
                }
            })
            .unwrap_or_else(|| ctx.working_dir.clone());

        let type_filter = params["type"].as_str();

        debug!(?name, ?search_path, ?type_filter, "running find");

        let mut cmd = tokio::process::Command::new("find");
        cmd.arg(&search_path);

        // Exclude common directories
        cmd.args([
            "-not", "-path", "*/node_modules/*",
            "-not", "-path", "*/.git/*",
            "-not", "-path", "*/target/*",
            "-not", "-path", "*/__pycache__/*",
        ]);

        if let Some(t) = type_filter {
            cmd.arg("-type").arg(t);
        }

        cmd.arg("-name").arg(name);

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            cmd.output(),
        )
        .await;

        match output {
            Ok(Ok(out)) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let lines: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
                let total = lines.len();

                let result_text = if total > MAX_RESULTS {
                    let truncated: String = lines[..MAX_RESULTS].join("\n");
                    format!("{truncated}\n\n[... {total} total results, showing first {MAX_RESULTS}]")
                } else if total == 0 {
                    "No files found.".to_string()
                } else {
                    format!("{}\n\n[{total} results]", lines.join("\n"))
                };

                Ok(ToolResult::success(result_text))
            }
            Ok(Err(e)) => Ok(ToolResult::error(format!("find failed: {e}"))),
            Err(_) => Ok(ToolResult::error("find timed out after 30 seconds".to_string())),
        }
    }

    fn source(&self) -> ToolSource { ToolSource::BuiltIn }
}
```

**Step 2: Register in `tools/mod.rs`**

**Step 3: Verify compilation, commit**

```bash
git add crates/octo-engine/src/tools/find.rs crates/octo-engine/src/tools/mod.rs
git commit -m "feat(tools): add FindTool for file/directory name search"
```

---

## Task 11: Update tools/mod.rs with all new tools registered

**Files:**
- Modify: `crates/octo-engine/src/tools/mod.rs`

**Step 1: Final `tools/mod.rs`**

```rust
pub mod bash;
pub mod file_edit;
pub mod file_read;
pub mod file_write;
pub mod find;
pub mod glob;
pub mod grep;
pub mod traits;

use std::collections::HashMap;
use std::sync::Arc;

pub use traits::Tool;

use self::bash::BashTool;
use self::file_edit::FileEditTool;
use self::file_read::FileReadTool;
use self::file_write::FileWriteTool;
use self::find::FindTool;
use self::glob::GlobTool;
use self::grep::GrepTool;
use octo_types::ToolSpec;

pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, tool: impl Tool + 'static) {
        let name = tool.name().to_string();
        self.tools.insert(name, Arc::new(tool));
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn specs(&self) -> Vec<ToolSpec> {
        self.tools.values().map(|t| t.spec()).collect()
    }

    pub fn names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub fn default_tools() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(BashTool::new());
    registry.register(FileReadTool::new());
    registry.register(FileWriteTool::new());
    registry.register(FileEditTool::new());
    registry.register(GrepTool::new());
    registry.register(GlobTool::new());
    registry.register(FindTool::new());
    registry
}
```

**Step 2: Update working memory default to reflect new tools**

In `crates/octo-engine/src/memory/working.rs`, update the `SandboxContext` block:

```rust
MemoryBlock::new(
    MemoryBlockKind::SandboxContext,
    "Sandbox Context",
    "Runtime: Native | Tools: bash, file_read, file_write, file_edit, grep, glob, find",
),
```

**Step 3: Verify full workspace compilation**

Run: `cargo check --workspace`
Expected: PASS

**Step 4: Commit**

```bash
git add crates/octo-engine/src/tools/ crates/octo-engine/src/memory/working.rs
git commit -m "feat(tools): register all 7 built-in tools (bash, file_read, file_write, file_edit, grep, glob, find)"
```

---

## Task 12: Update tool result soft-trimming in AgentLoop

**Files:**
- Modify: `crates/octo-engine/src/agent/loop_.rs`

**Step 1: Add tool result soft-trimming before injecting into messages**

Add a constant and helper function:

```rust
const TOOL_RESULT_SOFT_LIMIT: usize = 30_000;

fn maybe_trim_tool_result(result: &str) -> String {
    if result.len() <= TOOL_RESULT_SOFT_LIMIT {
        return result.to_string();
    }
    let head_end = result
        .char_indices()
        .nth(20_000)
        .map(|(idx, _)| idx)
        .unwrap_or(result.len());
    let char_count = result.chars().count();
    let tail_start = result
        .char_indices()
        .nth(char_count.saturating_sub(8_000))
        .map(|(idx, _)| idx)
        .unwrap_or(result.len());
    let omitted = result.len().saturating_sub(20_000 + 8_000);
    format!(
        "{}\n\n[... omitted {} chars ...]\n\n{}",
        &result[..head_end],
        omitted,
        &result[tail_start..]
    )
}
```

Apply this to tool results before pushing to messages in the tool execution loop:

```rust
// In the tool execution section, after getting result:
let trimmed_output = maybe_trim_tool_result(&result.output);

tool_results.push(ContentBlock::ToolResult {
    tool_use_id: tu.id.clone(),
    content: trimmed_output,
    is_error: result.is_error,
});
```

**Step 2: Verify compilation, commit**

```bash
git add crates/octo-engine/src/agent/loop_.rs
git commit -m "feat(engine): add tool result soft-trimming (30K chars, 67% head + 27% tail)"
```

---

## Task 13: Update Working Memory with priority sorting and expiry

**Files:**
- Modify: `crates/octo-engine/src/memory/working.rs`
- Modify: `crates/octo-engine/src/memory/traits.rs`
- Modify: `crates/octo-engine/src/memory/injector.rs`

**Step 1: Update WorkingMemory trait with new methods**

```rust
// memory/traits.rs - add methods
#[async_trait]
pub trait WorkingMemory: Send + Sync {
    async fn get_blocks(
        &self,
        user_id: &UserId,
        sandbox_id: &SandboxId,
    ) -> Result<Vec<MemoryBlock>>;

    async fn update_block(&self, block_id: &str, value: &str) -> Result<()>;

    async fn add_block(&self, block: MemoryBlock) -> Result<()>;

    async fn remove_block(&self, block_id: &str) -> Result<bool>;

    /// Expire blocks that have exceeded max_age_turns. Returns removed count.
    async fn expire_blocks(&self, current_turn: u32) -> Result<usize>;

    async fn compile(
        &self,
        user_id: &UserId,
        sandbox_id: &SandboxId,
    ) -> Result<String>;
}
```

**Step 2: Update InMemoryWorkingMemory implementation**

Add `add_block`, `remove_block`, `expire_blocks` implementations.

**Step 3: Update ContextInjector to sort by priority and enforce budget**

```rust
// memory/injector.rs
const WORKING_MEMORY_BUDGET_CHARS: usize = 12_000; // ~3K tokens

impl ContextInjector {
    pub fn compile(blocks: &[MemoryBlock]) -> String {
        let mut sorted: Vec<&MemoryBlock> = blocks.iter()
            .filter(|b| !b.value.is_empty())
            .collect();
        sorted.sort_by(|a, b| b.priority.cmp(&a.priority));

        let mut output = String::from("<working_memory>\n");
        let mut total_chars = 0;

        for block in sorted {
            let entry = format!(
                "<block kind=\"{}\" priority=\"{}\">{}</block>\n",
                block.id, block.priority, block.value
            );
            if total_chars + entry.len() > WORKING_MEMORY_BUDGET_CHARS {
                break;  // Budget exceeded, drop lower priority blocks
            }
            total_chars += entry.len();
            output.push_str(&entry);
        }

        output.push_str("</working_memory>");
        output
    }
}
```

**Step 4: Verify compilation, commit**

```bash
git add crates/octo-engine/src/memory/
git commit -m "feat(memory): add priority sorting, budget enforcement, block add/remove/expire"
```

---

## Task 14: Full workspace build and verification

**Files:**
- None (verification only)

**Step 1: Clean build check**

Run: `cargo check --workspace`
Expected: PASS (may have some dead_code warnings, acceptable)

**Step 2: Full build**

Run: `cargo build`
Expected: PASS

**Step 3: Frontend type check**

Run: `cd web && npx tsc --noEmit`
Expected: PASS (no frontend changes in this plan)

**Step 4: Commit any fixes needed**

If any compilation issues found, fix and commit:
```bash
git commit -m "fix: resolve compilation issues from Phase 2 batch 1"
```

---

## Summary: Files Created/Modified

| File | Operation | Task |
|------|-----------|------|
| `crates/octo-types/src/memory.rs` | Modified | Task 1 |
| `crates/octo-engine/src/context/mod.rs` | Created | Task 2 |
| `crates/octo-engine/src/context/builder.rs` | Created | Task 2 |
| `crates/octo-engine/src/context/budget.rs` | Created | Task 3 |
| `crates/octo-engine/src/context/pruner.rs` | Created | Task 4 |
| `crates/octo-engine/src/agent/context.rs` | Modified | Task 2 |
| `crates/octo-engine/src/agent/loop_.rs` | Modified | Task 5, 12 |
| `crates/octo-engine/src/lib.rs` | Modified | Task 2, 3, 4 |
| `crates/octo-engine/src/tools/file_write.rs` | Created | Task 6 |
| `crates/octo-engine/src/tools/file_edit.rs` | Created | Task 7 |
| `crates/octo-engine/src/tools/grep.rs` | Created | Task 8 |
| `crates/octo-engine/src/tools/glob.rs` | Created | Task 9 |
| `crates/octo-engine/src/tools/find.rs` | Created | Task 10 |
| `crates/octo-engine/src/tools/mod.rs` | Modified | Task 6-11 |
| `crates/octo-engine/src/memory/traits.rs` | Modified | Task 13 |
| `crates/octo-engine/src/memory/working.rs` | Modified | Task 1, 11, 13 |
| `crates/octo-engine/src/memory/injector.rs` | Modified | Task 13 |
| `crates/octo-engine/Cargo.toml` | Modified | Task 9 |
