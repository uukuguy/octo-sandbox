//! Extracted helper functions from `AgentLoop::run()`.
//!
//! These are standalone pure/near-pure functions that encapsulate
//! discrete steps of the agent loop. They can be tested independently
//! without constructing a full `AgentLoop`.

use octo_types::{ChatMessage, ContentBlock, MessageRole};

/// Check whether a loop guard verdict should halt execution.
/// Extracted from run() lines ~704-718.
///
/// Returns `Some(error_message)` if blocked, `None` if allowed.
pub fn check_loop_guard_verdict(verdict: &super::loop_guard::LoopGuardVerdict) -> Option<String> {
    use super::loop_guard::LoopGuardVerdict;
    match verdict {
        LoopGuardVerdict::Block(msg) | LoopGuardVerdict::CircuitBreak(msg) => {
            Some(format!("Loop Guard: {}", msg))
        }
        LoopGuardVerdict::Warn(msg) => {
            tracing::warn!("Loop Guard warning: {}", msg);
            None
        }
        LoopGuardVerdict::Allow => None,
    }
}

/// Decide whether tools should execute in parallel.
/// ZeroClaw pattern: parallel when multiple tools and no approval needed.
///
/// Returns `true` if parallel execution is appropriate.
pub fn should_execute_parallel(tool_count: usize, parallel_enabled: bool) -> bool {
    parallel_enabled && tool_count > 1
}

/// Inject Zone B working memory into messages.
/// Handles both initial injection and replacement of existing Zone B block.
/// Extracted from run() lines ~259-277.
pub fn inject_zone_b(messages: &mut Vec<ChatMessage>, memory_xml: &str) {
    if memory_xml.is_empty() {
        return;
    }
    let zone_b = ChatMessage {
        role: MessageRole::User,
        content: vec![ContentBlock::Text {
            text: memory_xml.to_string(),
        }],
    };
    let first_is_context = messages
        .first()
        .and_then(|m| m.content.first())
        .map(|b| matches!(b, ContentBlock::Text { text } if text.starts_with("<context>")))
        .unwrap_or(false);
    if first_is_context {
        messages[0] = zone_b;
    } else {
        messages.insert(0, zone_b);
    }
}

/// Soft-trim a tool result if it exceeds the limit.
/// 67% head + 27% tail preservation (pi_agent_rust pattern).
/// Extracted from run() end of file.
pub fn maybe_trim_tool_result(result: &str, soft_limit: usize) -> String {
    if result.len() <= soft_limit {
        return result.to_string();
    }
    let head_size = soft_limit * 67 / 100;
    let tail_size = soft_limit * 27 / 100;

    let head: String = result.chars().take(head_size).collect();
    let tail: String = result
        .chars()
        .rev()
        .take(tail_size)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let omitted = result.len() - head.len() - tail.len();

    format!(
        "{}\n\n... ({} characters omitted) ...\n\n{}",
        head, omitted, tail
    )
}

/// Determine whether the current round should force a text-only response.
///
/// When `round >= max_iterations - 1` and `force_text_at_last` is `true`,
/// returns `true`. The caller should omit the `tools` parameter from the
/// LLM request so the model is forced to produce a summary text response
/// instead of issuing further tool calls.
///
/// Returns `false` when `max_iterations == 0` (unlimited mode) or when
/// `force_text_at_last` is disabled.
pub fn should_force_text_only(round: u32, max_iterations: u32, force_text_at_last: bool) -> bool {
    force_text_at_last && max_iterations > 0 && round >= max_iterations - 1
}

/// Compute the effective max rounds from config.
/// 0 means unlimited (maps to `u32::MAX`).
pub fn effective_max_rounds(configured: u32) -> u32 {
    if configured == 0 {
        u32::MAX
    } else {
        configured
    }
}

/// Generate a guidance hint after a tool execution failure.
///
/// When a tool returns an error, this produces a short prompt that helps
/// the LLM consider alternative strategies instead of blindly retrying.
pub fn generate_error_hint(tool_name: &str, error_message: &str) -> String {
    format!(
        "The tool '{}' failed with error: {}\n\n\
         Consider alternative approaches:\n\
         1. Try a different tool that can achieve the same goal\n\
         2. Modify the parameters and retry\n\
         3. Break the task into smaller steps\n\
         4. Ask the user for clarification if the task is unclear",
        tool_name, error_message
    )
}

/// Decide whether an error guidance hint should be appended.
///
/// Returns `true` when the tool reported an error (`is_error == true`)
/// **and** the number of consecutive errors has not exceeded 3.
/// After 3 consecutive failures the hint is suppressed to avoid
/// flooding the context with repetitive guidance.
pub fn should_append_error_hint(is_error: bool, consecutive_errors: u32) -> bool {
    is_error && consecutive_errors <= 3
}
