use octo_types::{ChatMessage, ToolSpec};

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
pub fn estimate_messages_tokens(messages: &[ChatMessage], tools: &[ToolSpec]) -> u32 {
    let msg_chars: usize = messages
        .iter()
        .map(|m| {
            m.content
                .iter()
                .map(|b| match b {
                    octo_types::ContentBlock::Text { text } => text.len(),
                    octo_types::ContentBlock::ToolUse { input, .. } => {
                        input.to_string().len()
                    }
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
