use octo_types::{MemoryBlock, MemoryBlockKind, DEFAULT_CONTEXT_BUDGET_CHARS};

/// Default budget for context compilation, aligned with TokenBudget::default().context.
/// This is the character budget (12,000 chars ≈ 3,000 tokens).
const DEFAULT_BUDGET: usize = DEFAULT_CONTEXT_BUDGET_CHARS;

pub struct ContextInjector;

impl ContextInjector {
    /// Build Zone B dynamic context message content with default budget.
    ///
    /// Outputs a `<context>` XML block containing the current datetime and
    /// non-empty memory blocks sorted by priority (highest first).
    /// Deprecated block kinds (SandboxContext, AgentPersona) are skipped.
    pub fn compile(blocks: &[MemoryBlock]) -> String {
        Self::compile_with_budget(blocks, DEFAULT_BUDGET)
    }

    /// Build Zone B dynamic context message content with an explicit character budget.
    ///
    /// Outputs a `<context>` XML block containing the current datetime and
    /// non-empty memory blocks sorted by priority (highest first).
    /// Deprecated block kinds (SandboxContext, AgentPersona) are skipped.
    #[allow(deprecated)]
    pub fn compile_with_budget(blocks: &[MemoryBlock], char_budget: usize) -> String {
        let datetime = chrono::Local::now().format("%Y-%m-%d %H:%M %Z").to_string();

        let mut output = format!("<context>\n<datetime>{datetime}</datetime>\n");

        let mut sorted: Vec<&MemoryBlock> = blocks.iter().filter(|b| !b.value.is_empty()).collect();
        sorted.sort_by(|a, b| b.priority.cmp(&a.priority));

        let mut used = output.len();

        for block in sorted {
            let tag = match &block.kind {
                MemoryBlockKind::UserProfile => "user_profile",
                MemoryBlockKind::TaskContext => "task_context",
                MemoryBlockKind::AutoExtracted => "memory",
                MemoryBlockKind::Custom => "custom",
                // Skip deprecated kinds — their content now belongs in Zone A (SystemPromptBuilder)
                MemoryBlockKind::SandboxContext | MemoryBlockKind::AgentPersona => continue,
            };
            let entry = format!(
                "<{tag} priority=\"{}\">{}</{tag}>\n",
                block.priority, block.value
            );
            if used + entry.len() > char_budget {
                break;
            }
            used += entry.len();
            output.push_str(&entry);
        }

        output.push_str("</context>");
        output
    }
}
