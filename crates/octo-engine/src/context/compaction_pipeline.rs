//! LLM-based conversation compaction pipeline (AP-T6).
//!
//! When the context window fills up and a prompt-too-long error occurs,
//! this pipeline summarizes older messages using an LLM call, then rebuilds
//! essential state (memory zones, active skill, hooks) so the conversation
//! can continue without losing critical context.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use octo_types::skill::SkillDefinition;
use octo_types::{ChatMessage, CompletionRequest, ContentBlock, MessageRole, SandboxId, UserId};
use tracing::{debug, info, warn};

use crate::hooks::{HookContext, HookPoint, HookRegistry};
use crate::memory::store_traits::MemoryStore;
use crate::memory::{MemoryInjector, WorkingMemory};
use crate::providers::Provider;

use super::budget::ContextBudgetManager;
use super::compact_prompt;
use crate::agent::harness::is_prompt_too_long;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the compaction pipeline.
#[derive(Debug, Clone)]
pub struct CompactionPipelineConfig {
    /// Model to use for the summary LLM call. `None` reuses the session model.
    pub compact_model: Option<String>,
    /// Maximum output tokens for the summary response.
    pub summary_max_tokens: u32,
    /// Number of most-recent messages to keep uncompacted.
    pub keep_recent_messages: usize,
    /// Maximum PTL retries when the summary call itself overflows.
    pub max_ptl_retries: u32,
}

impl Default for CompactionPipelineConfig {
    fn default() -> Self {
        Self {
            compact_model: None,
            summary_max_tokens: 2000,
            keep_recent_messages: 6,
            max_ptl_retries: 3,
        }
    }
}

// ---------------------------------------------------------------------------
// Result
// ---------------------------------------------------------------------------

/// Output of a successful compaction.
#[derive(Debug)]
pub struct CompactionResult {
    /// Boundary marker indicating where compaction occurred.
    pub boundary_marker: ChatMessage,
    /// LLM-generated summary of the compacted portion.
    pub summary_messages: Vec<ChatMessage>,
    /// Recent messages kept verbatim (not compacted).
    pub kept_messages: Vec<ChatMessage>,
    /// Re-injected state messages (Zone B, skill context).
    pub reinjections: Vec<ChatMessage>,
    /// Text to append to system prompt (cross-session memory, pinned memories).
    /// Kept in system prompt so LLM treats them as background context
    /// and does not repeat them in tool results or responses.
    pub system_prompt_additions: String,
    /// Estimated token count before compaction.
    pub pre_compact_tokens: usize,
    /// Estimated token count after compaction.
    pub post_compact_tokens: usize,
}

// ---------------------------------------------------------------------------
// Context for state rebuild
// ---------------------------------------------------------------------------

/// Everything needed to rebuild agent state after compaction.
pub struct CompactionContext {
    pub memory: Option<Arc<dyn WorkingMemory>>,
    pub memory_store: Option<Arc<dyn MemoryStore>>,
    pub active_skill: Option<SkillDefinition>,
    pub hook_registry: Option<Arc<HookRegistry>>,
    pub session_summary_store: Option<Arc<crate::memory::SessionSummaryStore>>,
    pub user_id: UserId,
    pub sandbox_id: SandboxId,
    /// Custom instructions from the system prompt (used to guide summarization).
    pub custom_instructions: Option<String>,
}

// ---------------------------------------------------------------------------
// Pipeline
// ---------------------------------------------------------------------------

/// LLM-based compaction pipeline.
///
/// Replaces older conversation messages with a concise LLM-generated summary,
/// then re-injects essential state (working memory, cross-session memory,
/// active skill, hooks) so the agent can continue seamlessly.
#[derive(Debug, Clone)]
pub struct CompactionPipeline {
    config: CompactionPipelineConfig,
}

impl CompactionPipeline {
    pub fn new(config: CompactionPipelineConfig) -> Self {
        Self { config }
    }

    /// Run the full compaction pipeline.
    ///
    /// 1. Determine compaction boundary (keep N recent messages).
    /// 2. Preprocess older messages (replace images, truncate long results).
    /// 3. Call LLM to generate a 9-section summary.
    /// 4. Format the summary (strip `<analysis>`, extract `<summary>`).
    /// 5. Rebuild state (Zone B, Zone B+, Zone B++, active skill, hooks).
    /// 6. Return the replacement message sequence.
    pub async fn compact(
        &self,
        messages: &[ChatMessage],
        provider: &dyn Provider,
        model: &str,
        context: &CompactionContext,
    ) -> Result<CompactionResult> {
        let keep_count = self.config.keep_recent_messages;
        let boundary = messages.len().saturating_sub(keep_count);
        if boundary < 2 {
            return Err(anyhow!("Not enough messages to compact ({} total)", messages.len()));
        }

        let to_summarize = &messages[..boundary];
        let to_keep = &messages[boundary..];

        info!(
            total = messages.len(),
            boundary,
            kept = to_keep.len(),
            "Starting LLM compaction"
        );

        // Pre-compact token estimate
        let pre_tokens = ContextBudgetManager::estimate_messages_tokens(messages) as usize;

        // 1. Preprocess
        let preprocessed = Self::preprocess_for_summary(to_summarize);

        // 2. Build prompt
        let prompt = match context.custom_instructions.as_deref() {
            Some(instr) => compact_prompt::with_custom_instructions(instr),
            None => compact_prompt::COMPACT_PROMPT.to_string(),
        };

        // 3. Generate summary via LLM (with PTL self-retry)
        let compact_model = self.config.compact_model.as_deref().unwrap_or(model);
        let summary_text = self
            .generate_summary(provider, compact_model, &preprocessed, &prompt)
            .await?;

        // 4. Format
        let formatted = Self::format_summary(&summary_text);

        // 5. Rebuild state
        let (reinjections, sys_additions) = Self::rebuild_state(context).await;

        // 6. Assemble result
        let boundary_marker = ChatMessage {
            role: MessageRole::User,
            content: vec![ContentBlock::Text {
                text: "[Context compacted: earlier conversation summarized below]".into(),
            }],
        };

        let summary_msg = ChatMessage::assistant(&formatted);
        let kept = to_keep.to_vec();

        // Post-compact token estimate
        let post_messages: Vec<&ChatMessage> = std::iter::once(&boundary_marker)
            .chain(std::iter::once(&summary_msg))
            .chain(kept.iter())
            .chain(reinjections.iter())
            .collect();
        let post_tokens =
            ContextBudgetManager::estimate_messages_tokens(&post_messages.into_iter().cloned().collect::<Vec<_>>())
                as usize;

        info!(
            pre_tokens,
            post_tokens,
            saved = pre_tokens.saturating_sub(post_tokens),
            "Compaction complete"
        );

        Ok(CompactionResult {
            boundary_marker,
            summary_messages: vec![summary_msg],
            kept_messages: kept,
            reinjections,
            system_prompt_additions: sys_additions,
            pre_compact_tokens: pre_tokens,
            post_compact_tokens: post_tokens,
        })
    }

    // -----------------------------------------------------------------------
    // Preprocessing
    // -----------------------------------------------------------------------

    /// Replace images with placeholders and truncate oversized tool results
    /// to reduce the token cost of the summary LLM call.
    fn preprocess_for_summary(messages: &[ChatMessage]) -> Vec<ChatMessage> {
        messages
            .iter()
            .map(|m| {
                let content = m
                    .content
                    .iter()
                    .map(|block| match block {
                        // Images → lightweight placeholder
                        ContentBlock::Image { .. } | ContentBlock::Document { .. } => {
                            ContentBlock::Text {
                                text: "[image]".into(),
                            }
                        }
                        // Truncate long tool results
                        ContentBlock::ToolResult {
                            tool_use_id,
                            content,
                            is_error,
                        } if content.len() > 2000 => {
                            let truncated = if content.is_char_boundary(2000) {
                                &content[..2000]
                            } else {
                                // Find the last valid char boundary before 2000
                                let end = content
                                    .char_indices()
                                    .take_while(|(i, _)| *i < 2000)
                                    .last()
                                    .map(|(i, c)| i + c.len_utf8())
                                    .unwrap_or(0);
                                &content[..end]
                            };
                            ContentBlock::ToolResult {
                                tool_use_id: tool_use_id.clone(),
                                content: format!(
                                    "{}... [truncated, {} chars total]",
                                    truncated,
                                    content.len()
                                ),
                                is_error: *is_error,
                            }
                        }
                        other => other.clone(),
                    })
                    .collect();
                ChatMessage {
                    role: m.role.clone(),
                    content,
                }
            })
            .collect()
    }

    // -----------------------------------------------------------------------
    // Summary generation with PTL self-retry
    // -----------------------------------------------------------------------

    async fn generate_summary(
        &self,
        provider: &dyn Provider,
        model: &str,
        messages: &[ChatMessage],
        prompt: &str,
    ) -> Result<String> {
        let mut to_summarize = messages.to_vec();

        for attempt in 0..self.config.max_ptl_retries {
            let request = CompletionRequest {
                model: model.to_string(),
                system: Some(prompt.to_string()),
                messages: to_summarize.clone(),
                max_tokens: self.config.summary_max_tokens,
                tools: vec![],
                stream: false,
                temperature: None,
            };

            match provider.complete(request).await {
                Ok(response) => {
                    let text: String = response
                        .content
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::Text { text } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect();
                    if text.is_empty() {
                        return Err(anyhow!("LLM returned empty summary"));
                    }
                    return Ok(text);
                }
                Err(e) if is_prompt_too_long(&e) => {
                    let drop_count = (to_summarize.len() / 5).max(1);
                    warn!(
                        attempt,
                        drop_count,
                        remaining = to_summarize.len() - drop_count,
                        "Summary LLM hit PTL, dropping oldest messages"
                    );
                    to_summarize = to_summarize[drop_count..].to_vec();
                    if to_summarize.len() < 2 {
                        return Err(anyhow!("Not enough messages left after PTL retry"));
                    }
                }
                Err(e) => return Err(e),
            }
        }

        Err(anyhow!(
            "Compact summary failed after {} PTL retries",
            self.config.max_ptl_retries
        ))
    }

    // -----------------------------------------------------------------------
    // Summary formatting
    // -----------------------------------------------------------------------

    /// Strip the `<analysis>` scratchpad and extract `<summary>` content.
    pub fn format_summary(raw: &str) -> String {
        let mut result = raw.to_string();

        // Strip <analysis>...</analysis> block
        if let (Some(start), Some(end)) = (result.find("<analysis>"), result.find("</analysis>")) {
            if end > start {
                result = format!(
                    "{}{}",
                    &result[..start],
                    &result[end + "</analysis>".len()..]
                );
            }
        }

        // Extract <summary>...</summary> content
        if let (Some(start), Some(end)) = (result.find("<summary>"), result.find("</summary>")) {
            if end > start {
                let inner = &result[start + "<summary>".len()..end];
                result = inner.trim().to_string();
            }
        }

        format!(
            "This session is being continued from a previous conversation that hit the \
             context limit. The summary below captures the key points.\n\n{}",
            result.trim()
        )
    }

    // -----------------------------------------------------------------------
    // State rebuild
    // -----------------------------------------------------------------------

    /// Re-inject Zone B (working memory), Zone B++ (session summaries),
    /// active skill context, and fire SessionStart hooks.
    /// Returns (reinjection_messages, system_prompt_additions).
    async fn rebuild_state(ctx: &CompactionContext) -> (Vec<ChatMessage>, String) {
        let mut reinjections = Vec::new();

        // Zone B: working memory
        if let Some(ref memory) = ctx.memory {
            if let Ok(xml) = memory.compile(&ctx.user_id, &ctx.sandbox_id).await {
                if !xml.is_empty() {
                    reinjections.push(ChatMessage {
                        role: MessageRole::User,
                        content: vec![ContentBlock::Text {
                            text: format!("<working_memory>\n{}\n</working_memory>", xml),
                        }],
                    });
                    debug!("Reinjected Zone B working memory");
                }
            }
        }

        // Zone B+: cross-session memory + pinned memories → system prompt additions
        // (NOT user messages, so LLM treats them as background context)
        let mut sys_additions = String::new();
        if let Some(ref store) = ctx.memory_store {
            let injector = MemoryInjector::with_defaults();
            let cross = injector
                .build_memory_context(store.as_ref(), ctx.user_id.as_str(), "")
                .await;
            if !cross.is_empty() {
                sys_additions.push_str(&cross);
                debug!("System prompt addition: cross-session memory {} chars", cross.len());
            }

            // Phase AS: Importance-based pinned memories (safety net for high-importance entries)
            let pinned = injector
                .build_pinned_memories(store.as_ref(), ctx.user_id.as_str(), 0.8, 5, &[])
                .await;
            if !pinned.is_empty() {
                sys_additions.push_str(&pinned);
                debug!("System prompt addition: pinned memories {} chars", pinned.len());
            }
        }

        // Zone B++: session summaries
        if let Some(ref summary_store) = ctx.session_summary_store {
            if let Ok(summaries) = summary_store.recent(5).await {
                if !summaries.is_empty() {
                    let text = summaries
                        .iter()
                        .map(|s| {
                            format!(
                                "<session_summary session_id=\"{}\">\n{}\n</session_summary>",
                                s.session_id, s.summary
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    reinjections.push(ChatMessage {
                        role: MessageRole::User,
                        content: vec![ContentBlock::Text { text }],
                    });
                    debug!(count = summaries.len(), "Reinjected Zone B++ session summaries");
                }
            }
        }

        // Active skill context
        if let Some(ref skill) = ctx.active_skill {
            if !skill.body.is_empty() {
                reinjections.push(ChatMessage {
                    role: MessageRole::User,
                    content: vec![ContentBlock::Text {
                        text: format!("[Active skill: {}]\n{}", skill.name, skill.body),
                    }],
                });
                debug!(skill = %skill.name, "Reinjected active skill context");
            }
        }

        // Fire SessionStart hooks (non-blocking, best-effort)
        if let Some(ref hooks) = ctx.hook_registry {
            let hook_ctx = HookContext::new();
            let _ = hooks.execute(HookPoint::SessionStart, &hook_ctx).await;
            debug!("Fired SessionStart hooks for state rebuild");
        }

        (reinjections, sys_additions)
    }
}

// ---------------------------------------------------------------------------
// Snip Compact (AP-T10)
// ---------------------------------------------------------------------------

/// Marker that users can insert to request context truncation at that point.
pub const SNIP_MARKER: &str = "[SNIP]";

impl CompactionPipeline {
    /// Detect and process a snip marker in the message history.
    ///
    /// If `[SNIP]` is found, messages before (and including) the marker are
    /// either summarized (if a provider is available) or simply truncated.
    ///
    /// Returns the number of messages removed.
    pub async fn snip_compact(
        messages: &mut Vec<ChatMessage>,
        provider: Option<&dyn Provider>,
        model: &str,
        pipeline: Option<&CompactionPipeline>,
        context: Option<&CompactionContext>,
    ) -> Result<usize> {
        // Find the last SNIP marker
        let pos = messages.iter().rposition(|m| {
            m.content.iter().any(|b| {
                if let ContentBlock::Text { text } = b {
                    text.contains(SNIP_MARKER)
                } else {
                    false
                }
            })
        });

        let pos = match pos {
            Some(p) => p,
            None => return Ok(0),
        };

        info!(snip_position = pos, total = messages.len(), "Snip marker found");

        // If we have a pipeline + provider, try to summarize before truncating
        if let (Some(pipeline), Some(provider), Some(ctx)) = (pipeline, provider, context) {
            let to_summarize = &messages[..pos];
            if to_summarize.len() >= 2 {
                match pipeline.compact(to_summarize, provider, model, ctx).await {
                    Ok(result) => {
                        let removed = pos + 1;
                        messages.drain(..=pos);
                        // Insert boundary marker + summary at the front
                        messages.insert(0, result.boundary_marker);
                        for (i, msg) in result.summary_messages.into_iter().enumerate() {
                            messages.insert(1 + i, msg);
                        }
                        info!(removed, "Snip compact with summary");
                        return Ok(removed);
                    }
                    Err(e) => {
                        warn!(error = %e, "Snip summary failed, falling back to truncation");
                        // Fall through to simple truncation
                    }
                }
            }
        }

        // Simple truncation: remove everything up to and including the snip marker
        let removed = pos + 1;
        messages.drain(..=pos);
        info!(removed, "Snip compact (truncation only)");
        Ok(removed)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preprocess_replaces_images() {
        let msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: vec![ContentBlock::Image {
                source_type: octo_types::ImageSourceType::Base64,
                media_type: "image/png".into(),
                data: "huge-base64-data".into(),
            }],
        }];
        let result = CompactionPipeline::preprocess_for_summary(&msgs);
        assert_eq!(result.len(), 1);
        match &result[0].content[0] {
            ContentBlock::Text { text } => assert_eq!(text, "[image]"),
            other => panic!("Expected Text, got {:?}", other),
        }
    }

    #[test]
    fn test_preprocess_truncates_long_tool_results() {
        let long_content = "x".repeat(5000);
        let msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: "t1".into(),
                content: long_content.clone(),
                is_error: false,
            }],
        }];
        let result = CompactionPipeline::preprocess_for_summary(&msgs);
        match &result[0].content[0] {
            ContentBlock::ToolResult { content, .. } => {
                assert!(content.len() < long_content.len());
                assert!(content.contains("[truncated, 5000 chars total]"));
            }
            other => panic!("Expected ToolResult, got {:?}", other),
        }
    }

    #[test]
    fn test_preprocess_keeps_short_tool_results() {
        let short = "ok";
        let msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: "t1".into(),
                content: short.into(),
                is_error: false,
            }],
        }];
        let result = CompactionPipeline::preprocess_for_summary(&msgs);
        match &result[0].content[0] {
            ContentBlock::ToolResult { content, .. } => assert_eq!(content, short),
            other => panic!("Expected ToolResult, got {:?}", other),
        }
    }

    #[test]
    fn test_format_summary_strips_analysis() {
        let raw = "<analysis>thinking...</analysis>\n<summary>\n1. Intent: foo\n</summary>";
        let result = CompactionPipeline::format_summary(raw);
        assert!(!result.contains("<analysis>"));
        assert!(!result.contains("thinking..."));
        assert!(result.contains("1. Intent: foo"));
        assert!(result.contains("continued from a previous conversation"));
    }

    #[test]
    fn test_format_summary_no_tags() {
        let raw = "Just plain summary text";
        let result = CompactionPipeline::format_summary(raw);
        assert!(result.contains("Just plain summary text"));
        assert!(result.contains("continued from a previous conversation"));
    }

    #[test]
    fn test_format_summary_analysis_only() {
        let raw = "<analysis>deep thoughts</analysis>\nSome remaining text";
        let result = CompactionPipeline::format_summary(raw);
        assert!(!result.contains("deep thoughts"));
        assert!(result.contains("Some remaining text"));
    }

    #[tokio::test]
    async fn test_snip_compact_truncation() {
        let mut messages = vec![
            ChatMessage {
                role: MessageRole::User,
                content: vec![ContentBlock::Text {
                    text: "old stuff".into(),
                }],
            },
            ChatMessage {
                role: MessageRole::Assistant,
                content: vec![ContentBlock::Text {
                    text: "old response".into(),
                }],
            },
            ChatMessage {
                role: MessageRole::User,
                content: vec![ContentBlock::Text {
                    text: "[SNIP]".into(),
                }],
            },
            ChatMessage {
                role: MessageRole::User,
                content: vec![ContentBlock::Text {
                    text: "new stuff".into(),
                }],
            },
        ];

        let removed = CompactionPipeline::snip_compact(&mut messages, None, "test", None, None)
            .await
            .unwrap();
        assert_eq!(removed, 3, "Should remove everything up to and including SNIP");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].text_content(), "new stuff");
    }

    #[tokio::test]
    async fn test_snip_compact_no_marker() {
        let mut messages = vec![ChatMessage {
            role: MessageRole::User,
            content: vec![ContentBlock::Text {
                text: "no snip here".into(),
            }],
        }];

        let removed = CompactionPipeline::snip_compact(&mut messages, None, "test", None, None)
            .await
            .unwrap();
        assert_eq!(removed, 0);
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn test_preprocess_replaces_documents() {
        let msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: vec![ContentBlock::Document {
                source_type: "base64".into(),
                media_type: "application/pdf".into(),
                data: "pdf-data".into(),
            }],
        }];
        let result = CompactionPipeline::preprocess_for_summary(&msgs);
        match &result[0].content[0] {
            ContentBlock::Text { text } => assert_eq!(text, "[image]"),
            other => panic!("Expected Text, got {:?}", other),
        }
    }
}
