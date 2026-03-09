use anyhow::Result;
use octo_types::{ChatMessage, CompletionRequest, ContentBlock, MemoryCategory, MessageRole};
use tracing::{debug, warn};

use crate::providers::Provider;

#[derive(Debug, Clone)]
pub struct ExtractedFact {
    pub fact: String,
    pub category: MemoryCategory,
    pub importance: f32,
}

const EXTRACTION_PROMPT: &str = r#"Analyze the following conversation and extract important facts that should be remembered for future interactions.

For each fact, categorize it as one of: profile, preferences, tools, debug, patterns.
Rate importance from 0.0 to 1.0.

Output ONLY a JSON array, no other text:
[{"fact": "...", "category": "profile|preferences|tools|debug|patterns", "importance": 0.0-1.0}]

If no facts worth remembering, output: []

Conversation:
"#;

const MAX_INPUT_CHARS: usize = 4000;

pub struct FactExtractor;

impl FactExtractor {
    pub async fn extract(
        provider: &dyn Provider,
        messages: &[ChatMessage],
        model: &str,
    ) -> Result<Vec<ExtractedFact>> {
        if messages.is_empty() {
            return Ok(Vec::new());
        }

        // Build conversation text from messages (truncated)
        let mut conversation = String::new();
        for msg in messages {
            let role = match msg.role {
                MessageRole::User => "User",
                MessageRole::Assistant => "Assistant",
                MessageRole::System => continue,
            };
            for block in &msg.content {
                if let ContentBlock::Text { text } = block {
                    conversation.push_str(&format!("{role}: {text}\n"));
                }
            }
            if conversation.len() > MAX_INPUT_CHARS {
                break;
            }
        }

        if conversation.len() > MAX_INPUT_CHARS {
            conversation.truncate(MAX_INPUT_CHARS);
        }

        let prompt = format!("{EXTRACTION_PROMPT}{conversation}");

        let request = CompletionRequest {
            model: model.to_string(),
            system: None,
            messages: vec![ChatMessage::user(&prompt)],
            max_tokens: 2048,
            temperature: Some(0.1),
            tools: Vec::new(),
            stream: false,
        };

        let response = provider.complete(request).await?;

        // Extract text from response
        let response_text = response
            .content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("");

        // Parse JSON
        let facts = parse_facts(&response_text);
        debug!(count = facts.len(), "Extracted facts from conversation");
        Ok(facts)
    }
}

fn parse_facts(text: &str) -> Vec<ExtractedFact> {
    // Try to find JSON array in the response
    let trimmed = text.trim();

    // Try direct parse first
    if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(trimmed) {
        return convert_facts(arr);
    }

    // Try to find [...] in the text
    if let Some(start) = trimmed.find('[') {
        if let Some(end) = trimmed.rfind(']') {
            if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&trimmed[start..=end]) {
                return convert_facts(arr);
            }
        }
    }

    warn!("Failed to parse facts JSON from LLM response");
    Vec::new()
}

fn convert_facts(arr: Vec<serde_json::Value>) -> Vec<ExtractedFact> {
    arr.into_iter()
        .filter_map(|item| {
            let fact = item["fact"].as_str()?.to_string();
            let category_str = item["category"].as_str().unwrap_or("patterns");
            let category = MemoryCategory::parse(category_str).unwrap_or(MemoryCategory::Patterns);
            let importance = item["importance"]
                .as_f64()
                .map(|f| f as f32)
                .unwrap_or(0.5)
                .clamp(0.0, 1.0);

            if fact.is_empty() {
                return None;
            }

            Some(ExtractedFact {
                fact,
                category,
                importance,
            })
        })
        .collect()
}
