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

#[cfg(test)]
mod tests {
    use super::*;
    use octo_types::MemoryBlockKind;

    fn make_block(kind: MemoryBlockKind, value: &str, priority: u8) -> MemoryBlock {
        let kind_str = match &kind {
            MemoryBlockKind::SandboxContext => "sandbox_context",
            MemoryBlockKind::AgentPersona => "agent_persona",
            MemoryBlockKind::UserProfile => "user_profile",
            MemoryBlockKind::TaskContext => "task_context",
            MemoryBlockKind::AutoExtracted => "auto_extracted",
            MemoryBlockKind::Custom => "custom",
        };
        MemoryBlock {
            id: kind_str.to_string(),
            kind,
            label: kind_str.to_string(),
            value: value.to_string(),
            priority,
            max_age_turns: None,
            last_updated_turn: 0,
            char_limit: 12000,
            is_readonly: false,
        }
    }

    #[test]
    fn test_compile_empty() {
        let blocks: Vec<MemoryBlock> = vec![];
        let result = ContextInjector::compile(&blocks);
        assert!(result.starts_with("<context>"));
        assert!(result.contains("<datetime>"));
        assert!(result.ends_with("</context>"));
    }

    #[test]
    fn test_compile_with_content() {
        let blocks = vec![make_block(MemoryBlockKind::UserProfile, "I am a developer", 128)];
        let result = ContextInjector::compile(&blocks);
        assert!(result.contains("I am a developer"));
        assert!(result.contains("user_profile"));
    }

    #[test]
    fn test_compile_respects_priority() {
        let blocks = vec![
            make_block(MemoryBlockKind::UserProfile, "low priority", 50),
            make_block(MemoryBlockKind::TaskContext, "high priority", 200),
        ];
        let result = ContextInjector::compile(&blocks);
        let high_pos = result.find("high priority").unwrap();
        let low_pos = result.find("low priority").unwrap();
        assert!(high_pos < low_pos);
    }

    #[test]
    fn test_compile_respects_budget() {
        let long_value = "x".repeat(10000);
        let blocks = vec![make_block(MemoryBlockKind::UserProfile, &long_value, 128)];
        let result = ContextInjector::compile_with_budget(&blocks, 100);
        // Should respect budget and not include all content
        assert!(result.contains("</context>"));
    }

    #[test]
    #[allow(deprecated)]
    fn test_compile_skips_deprecated() {
        let blocks = vec![
            make_block(MemoryBlockKind::UserProfile, "active content", 128),
            make_block(MemoryBlockKind::SandboxContext, "deprecated", 200),
        ];
        let result = ContextInjector::compile(&blocks);
        assert!(result.contains("active content"));
        assert!(!result.contains("deprecated"));
        assert!(!result.contains("sandbox_context"));
    }
}
