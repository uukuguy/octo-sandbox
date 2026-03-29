//! Prompt executor for declarative prompt-type hooks.
//!
//! Calls LLM provider to evaluate a rendered prompt, then parses the JSON
//! decision from the response. Uses a separate short-context call (not the
//! agent's main conversation context).
//!
//! Token budget: single evaluation ≤ 500 output tokens. Default timeout: 10s.

use tracing::{debug, warn};

use crate::hooks::HookContext;
use super::command_executor::HookDecision;
use super::prompt_renderer;
use crate::providers::Provider;

/// Execute a prompt-type hook by rendering the template and calling the LLM.
///
/// Returns a `HookDecision` parsed from the LLM's JSON response.
pub async fn execute_prompt(
    template: &str,
    ctx: &HookContext,
    provider: &dyn Provider,
    model: &str,
    timeout_secs: u32,
) -> anyhow::Result<HookDecision> {
    // 1. Render the prompt template
    let rendered = prompt_renderer::render_prompt(template, ctx);
    debug!(
        rendered_len = rendered.len(),
        "Prompt hook: rendered template"
    );

    // 2. Build a minimal CompletionRequest for evaluation
    let request = octo_types::CompletionRequest {
        model: model.to_string(),
        system: Some(
            "You are a security evaluator. Analyze the request and return a JSON object with \
             \"decision\" (\"allow\" or \"deny\") and \"reason\" fields. Only return valid JSON."
                .to_string(),
        ),
        messages: vec![octo_types::ChatMessage {
            role: octo_types::MessageRole::User,
            content: vec![octo_types::ContentBlock::Text { text: rendered }],
        }],
        max_tokens: 500,
        temperature: Some(0.0),
        tools: vec![],
        stream: false,
    };

    // 3. Call provider with timeout
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs as u64),
        provider.complete(request),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Prompt hook timed out after {}s", timeout_secs))??;

    // 4. Extract text from response content blocks
    let text: String = response
        .content
        .iter()
        .filter_map(|block| {
            if let octo_types::ContentBlock::Text { text } = block {
                Some(text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("");
    debug!(response_len = text.len(), "Prompt hook: received LLM response");

    // 5. Parse JSON decision from response
    parse_decision_from_text(&text)
}

/// Parse a HookDecision from LLM text output.
///
/// Tries to find JSON in the text, handling cases where the LLM wraps it
/// in markdown code blocks or adds explanatory text.
fn parse_decision_from_text(text: &str) -> anyhow::Result<HookDecision> {
    let trimmed = text.trim();

    // Try direct parse first
    if let Ok(decision) = serde_json::from_str::<HookDecision>(trimmed) {
        return Ok(decision);
    }

    // Try to extract JSON from fenced code blocks
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            let json_str = &trimmed[start..=end];
            if let Ok(decision) = serde_json::from_str::<HookDecision>(json_str) {
                return Ok(decision);
            }
        }
    }

    // Fallback: if text contains "deny" keyword, treat as deny
    if trimmed.to_lowercase().contains("deny") || trimmed.to_lowercase().contains("block") {
        warn!("Prompt hook: could not parse JSON, falling back to keyword detection (deny)");
        return Ok(HookDecision {
            decision: "deny".into(),
            reason: Some(format!("LLM evaluation (unparsed): {}", &trimmed[..trimmed.len().min(200)])),
            updated_input: None,
            system_message: None,
        });
    }

    // Default: allow if no clear deny signal
    warn!("Prompt hook: could not parse decision, defaulting to allow");
    Ok(HookDecision {
        decision: "allow".into(),
        reason: Some("LLM response could not be parsed as decision JSON".into()),
        updated_input: None,
        system_message: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_clean_json() {
        let text = r#"{"decision": "deny", "reason": "dangerous command"}"#;
        let d = parse_decision_from_text(text).unwrap();
        assert!(d.is_deny());
        assert_eq!(d.reason.as_deref(), Some("dangerous command"));
    }

    #[test]
    fn test_parse_json_in_code_block() {
        let text = r#"Here's my analysis:
```json
{"decision": "allow", "reason": "safe operation"}
```"#;
        let d = parse_decision_from_text(text).unwrap();
        assert!(d.is_allow());
    }

    #[test]
    fn test_parse_json_with_surrounding_text() {
        let text = "After careful analysis, I conclude: {\"decision\": \"deny\", \"reason\": \"path traversal\"} is the result.";
        let d = parse_decision_from_text(text).unwrap();
        assert!(d.is_deny());
    }

    #[test]
    fn test_parse_keyword_fallback_deny() {
        let text = "I would deny this operation because it accesses system files.";
        let d = parse_decision_from_text(text).unwrap();
        assert!(
            d.is_deny(),
            "expected deny, got decision='{}', reason={:?}",
            d.decision,
            d.reason
        );
    }

    #[test]
    fn test_parse_keyword_fallback_block() {
        let text = "I would block this request due to security concerns.";
        let d = parse_decision_from_text(text).unwrap();
        assert!(d.is_deny());
    }

    #[test]
    fn test_parse_unparseable_defaults_allow() {
        let text = "This looks fine to me, proceed with the operation.";
        let d = parse_decision_from_text(text).unwrap();
        assert!(d.is_allow());
    }

    #[test]
    fn test_parse_empty_defaults_allow() {
        let d = parse_decision_from_text("").unwrap();
        assert!(d.is_allow());
    }
}
