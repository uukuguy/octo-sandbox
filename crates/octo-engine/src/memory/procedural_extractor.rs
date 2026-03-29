//! Procedural memory extractor — identifies workflow patterns from tool call sequences.
//!
//! Analyzes conversation messages for recurring tool usage patterns and extracts them
//! as procedural memories (MemoryType::Procedural). These represent "how to do X"
//! knowledge learned from agent behavior.

use anyhow::Result;
use tracing::{debug, warn};

use octo_types::{ChatMessage, CompletionRequest, ContentBlock};

use crate::providers::Provider;

/// Extracts procedural memories (workflow patterns) from conversation tool chains.
pub struct ProceduralExtractor;

/// A single procedural memory extracted from the conversation.
#[derive(Debug, Clone)]
pub struct ProceduralPattern {
    /// Human-readable description of the workflow pattern
    pub description: String,
    /// Tool sequence that forms this pattern
    pub tool_sequence: Vec<String>,
    /// What kind of task this pattern accomplishes
    pub task_type: String,
}

impl ProceduralExtractor {
    /// Extract workflow patterns from conversation messages using LLM analysis.
    ///
    /// Examines tool call chains for multi-step patterns that represent
    /// reusable workflows (e.g., "read file → modify → write → test").
    ///
    /// Returns empty Vec if fewer than 3 tool calls or LLM extraction fails.
    pub async fn extract_patterns(
        provider: &dyn Provider,
        messages: &[ChatMessage],
        model: &str,
    ) -> Result<Vec<ProceduralPattern>> {
        let tool_sequence = Self::build_tool_sequence(messages);
        if tool_sequence.len() < 3 {
            debug!(
                tool_count = tool_sequence.len(),
                "Too few tool calls for procedural pattern extraction"
            );
            return Ok(vec![]);
        }

        let sequence_text = tool_sequence
            .iter()
            .enumerate()
            .map(|(i, name)| format!("{}. {name}", i + 1))
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!(
            r#"Analyze the following tool call sequence from an agent session.
Identify distinct workflow patterns — multi-step sequences that accomplish a specific task.
A pattern must have at least 2 steps and represent a reusable procedure.

Examples of patterns:
- "Code modification": file_read → file_write → bash (to test)
- "Search and update": grep → file_read → file_write
- "Research and store": web_search → memory_store

Tool sequence:
{sequence_text}

Return a JSON array of patterns. Each pattern has:
- description: string (natural language description of what this workflow does)
- tool_sequence: array of tool names in order
- task_type: string (e.g. "code_modification", "research", "debugging", "configuration", "testing")

If no meaningful patterns found, return: []
Return ONLY the JSON array, no markdown fences or explanation."#
        );

        let request = CompletionRequest {
            model: model.to_string(),
            system: Some(
                "You are a workflow pattern analyzer. Identify reusable multi-step procedures from tool usage. Output valid JSON only."
                    .into(),
            ),
            messages: vec![ChatMessage::user(prompt)],
            max_tokens: 2048,
            temperature: Some(0.0),
            ..Default::default()
        };

        let response = provider.complete(request).await?;
        let text = response
            .content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<String>();

        Self::parse_patterns(&text)
    }

    /// Extract ordered tool names from conversation messages.
    fn build_tool_sequence(messages: &[ChatMessage]) -> Vec<String> {
        let mut sequence = Vec::new();
        for msg in messages {
            for block in &msg.content {
                if let ContentBlock::ToolUse { name, .. } = block {
                    sequence.push(name.clone());
                }
            }
        }
        sequence
    }

    /// Parse LLM response into ProceduralPattern records.
    fn parse_patterns(text: &str) -> Result<Vec<ProceduralPattern>> {
        let cleaned = text
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        if cleaned.is_empty() || cleaned == "[]" {
            return Ok(vec![]);
        }

        match serde_json::from_str::<Vec<RawPattern>>(cleaned) {
            Ok(raw) => {
                let patterns = raw
                    .into_iter()
                    .filter(|p| p.tool_sequence.len() >= 2)
                    .map(|p| ProceduralPattern {
                        description: p.description,
                        tool_sequence: p.tool_sequence,
                        task_type: p.task_type.unwrap_or_else(|| "general".to_string()),
                    })
                    .collect();
                Ok(patterns)
            }
            Err(e) => {
                warn!("Failed to parse procedural pattern response: {e}");
                debug!("Raw response: {cleaned}");
                Ok(vec![])
            }
        }
    }
}

#[derive(serde::Deserialize)]
struct RawPattern {
    description: String,
    tool_sequence: Vec<String>,
    task_type: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_patterns_valid() {
        let json = r#"[
            {
                "description": "Read a file, modify it, and run tests",
                "tool_sequence": ["file_read", "file_write", "bash"],
                "task_type": "code_modification"
            }
        ]"#;
        let patterns = ProceduralExtractor::parse_patterns(json).unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].task_type, "code_modification");
        assert_eq!(patterns[0].tool_sequence.len(), 3);
    }

    #[test]
    fn test_parse_patterns_empty() {
        let patterns = ProceduralExtractor::parse_patterns("[]").unwrap();
        assert!(patterns.is_empty());
    }

    #[test]
    fn test_parse_patterns_filters_short_sequences() {
        let json = r#"[
            {
                "description": "Single tool usage",
                "tool_sequence": ["bash"],
                "task_type": "one-off"
            },
            {
                "description": "Multi-step workflow",
                "tool_sequence": ["grep", "file_read", "file_write"],
                "task_type": "code_modification"
            }
        ]"#;
        let patterns = ProceduralExtractor::parse_patterns(json).unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].tool_sequence.len(), 3);
    }

    #[test]
    fn test_parse_patterns_with_code_fences() {
        let json = "```json\n[{\"description\": \"test\", \"tool_sequence\": [\"a\", \"b\"], \"task_type\": \"test\"}]\n```";
        let patterns = ProceduralExtractor::parse_patterns(json).unwrap();
        assert_eq!(patterns.len(), 1);
    }

    #[test]
    fn test_parse_patterns_invalid_json() {
        let patterns = ProceduralExtractor::parse_patterns("not json at all").unwrap();
        assert!(patterns.is_empty());
    }

    #[test]
    fn test_build_tool_sequence() {
        let messages = vec![
            ChatMessage {
                role: octo_types::MessageRole::Assistant,
                content: vec![
                    ContentBlock::ToolUse {
                        id: "1".into(),
                        name: "file_read".into(),
                        input: serde_json::json!({}),
                    },
                    ContentBlock::ToolUse {
                        id: "2".into(),
                        name: "file_write".into(),
                        input: serde_json::json!({}),
                    },
                ],
            },
            ChatMessage {
                role: octo_types::MessageRole::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "1".into(),
                    content: "ok".into(),
                    is_error: false,
                }],
            },
            ChatMessage {
                role: octo_types::MessageRole::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "3".into(),
                    name: "bash".into(),
                    input: serde_json::json!({}),
                }],
            },
        ];
        let seq = ProceduralExtractor::build_tool_sequence(&messages);
        assert_eq!(seq, vec!["file_read", "file_write", "bash"]);
    }
}
