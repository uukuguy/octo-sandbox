/// Prompt template for LLM-based conversation compaction.
///
/// The 9-section structure captures all critical context to ensure
/// continuity after compaction. The `<analysis>` scratchpad improves
/// summary quality but is stripped from the final output.
pub const COMPACT_PROMPT: &str = r#"Your task is to create a detailed summary of the conversation so far. This summary will replace the earlier messages to free up context space while preserving all important information.

Before generating the summary, organize your thoughts inside <analysis> tags:
1. Chronologically analyze each message, identifying user intent, technical approaches, code changes, and error fixes.
2. Pay special attention to user feedback and corrections — these represent validated decisions.

The summary MUST include these 9 sections:

1. **Primary Requests & Intent**: The user's explicit goals and what they are trying to accomplish.
2. **Key Technical Concepts**: Frameworks, patterns, tools, and technologies discussed.
3. **Files & Code**: Files viewed, modified, or created — include key code snippets and file paths.
4. **Errors & Fixes**: Errors encountered and how they were resolved, especially user-guided corrections.
5. **Problem Solving**: Problems that were solved and any ongoing investigation.
6. **User Messages**: List all non-tool-result user messages (preserves user intent trail).
7. **Pending Tasks**: Explicitly requested TODO items that have not been completed.
8. **Current Work**: What was being worked on immediately before compaction, including file names and code snippets.
9. **Next Steps** (optional): Only include if directly related to recent work — quote the original request to prevent task drift.

<example>
<analysis>
[Analysis of the conversation...]
</analysis>

<summary>
1. **Primary Requests & Intent**: [Detailed description]
2. **Key Technical Concepts**: - [Concept 1] - [Concept 2]
3. **Files & Code**: - `path/to/file.rs` — [what was done]
4. **Errors & Fixes**: - [Error] → [Fix]
5. **Problem Solving**: - [Problem] → [Resolution]
6. **User Messages**: - [Message 1] - [Message 2]
7. **Pending Tasks**: - [Task 1] - [Task 2]
8. **Current Work**: Working on [description] in `file.rs`
9. **Next Steps**: [Only if applicable]
</summary>
</example>

Output ONLY the analysis and summary. Do NOT call any tools. Do NOT output anything else."#;

/// Format a custom instructions addendum for the compaction prompt.
///
/// When the agent has custom system instructions, they are appended to remind
/// the LLM to preserve instruction-relevant context during summarization.
pub fn with_custom_instructions(custom: &str) -> String {
    format!(
        "{}\n\nIMPORTANT: The assistant has these custom instructions that should inform \
         what context to preserve in the summary:\n\n{}",
        COMPACT_PROMPT, custom
    )
}
