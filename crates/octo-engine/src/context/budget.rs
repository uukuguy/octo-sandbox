use octo_types::{ChatMessage, ContentBlock, ToolSpec};

const CHARS_PER_TOKEN: usize = 4;

/// 上下文降级级别（4+1 阶段，参考 CONTEXT_ENGINEERING_DESIGN.md §7.1）
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DegradationLevel {
    /// 使用率 < 60%：无需降级
    None,
    /// 使用率 60%-70%：工具结果头尾裁剪（预警性轻度干预）
    SoftTrim,
    /// 使用率 70%-90%：保留最近 10 条消息
    AutoCompaction,
    /// 使用率 > 90%：保留最近 4 条消息，触发 Memory Flush
    OverflowCompaction,
    /// 压缩后仍超限：截断当前工具结果至 8000 chars
    ToolResultTruncation,
    /// 全部手段失效：返回结构化错误，终止 Agent Loop
    FinalError,
}

#[derive(Clone)]
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
    ///
    /// 注意：ToolResultTruncation 和 FinalError 是升级触发的，不在此函数中返回。
    pub fn compute_degradation_level(
        &self,
        system_prompt: &str,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
    ) -> DegradationLevel {
        let ratio = self.usage_ratio(system_prompt, messages, tools);
        match ratio {
            r if r < 0.60 => DegradationLevel::None,
            r if r < 0.70 => DegradationLevel::SoftTrim,
            r if r < 0.90 => DegradationLevel::AutoCompaction,
            _ => DegradationLevel::OverflowCompaction,
        }
    }

    pub fn context_window(&self) -> u32 {
        self.context_window
    }

    /// Produce a snapshot of the current token budget state.
    pub fn snapshot(
        &self,
        system_prompt: &str,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
    ) -> octo_types::TokenBudgetSnapshot {
        let sys_tokens = Self::estimate_tokens(system_prompt) as usize;
        let history_tokens = Self::estimate_messages_tokens(messages) as usize;
        let tool_tokens = Self::estimate_tool_specs_tokens(tools) as usize;
        let total = self.context_window as usize;
        let used = sys_tokens + history_tokens + tool_tokens;
        let free = total.saturating_sub(used);
        let usage_pct = if total > 0 {
            (used as f32 / total as f32) * 100.0
        } else {
            0.0
        };

        let degradation = match self.compute_degradation_level(system_prompt, messages, tools) {
            DegradationLevel::None => 0,
            DegradationLevel::SoftTrim => 1,
            DegradationLevel::AutoCompaction => 2,
            DegradationLevel::OverflowCompaction => 3,
            DegradationLevel::ToolResultTruncation => 4,
            DegradationLevel::FinalError => 5,
        };

        octo_types::TokenBudgetSnapshot {
            total,
            system_prompt: sys_tokens,
            dynamic_context: tool_tokens,
            history: history_tokens,
            free,
            usage_percent: usage_pct,
            degradation_level: degradation,
        }
    }
}

impl Default for ContextBudgetManager {
    fn default() -> Self {
        Self::new(200_000)
    }
}
