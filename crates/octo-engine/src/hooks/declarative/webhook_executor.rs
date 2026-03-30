//! Webhook executor for declarative hooks.
//!
//! Sends HookContext as HTTP POST JSON to an external URL and parses
//! the response as a HookDecision.

use tracing::{debug, warn};

use crate::hooks::HookContext;
use super::command_executor::HookDecision;

/// Execute a webhook-type hook by POSTing context JSON to the URL.
pub async fn execute_webhook(
    url: &str,
    method: &str,
    ctx: &HookContext,
    timeout_secs: u32,
) -> anyhow::Result<HookDecision> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs as u64))
        .build()?;

    let body = ctx.to_json();
    debug!(url, method, "Executing webhook hook");

    let response = match method.to_uppercase().as_str() {
        "POST" => client.post(url).json(&body).send().await?,
        "PUT" => client.put(url).json(&body).send().await?,
        _ => {
            return Err(anyhow::anyhow!("Unsupported webhook method: {}", method));
        }
    };

    let status = response.status();
    let text = response.text().await.unwrap_or_default();

    if status.is_success() {
        // Parse response as HookDecision
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Ok(HookDecision {
                decision: "allow".into(),
                reason: None,
                updated_input: None,
                system_message: None,
            });
        }
        serde_json::from_str(trimmed).map_err(|e| {
            anyhow::anyhow!(
                "Failed to parse webhook response as JSON: {e}\nBody: {}",
                &trimmed[..trimmed.len().min(200)]
            )
        })
    } else {
        warn!(url, status = %status, "Webhook returned non-success status");
        Err(anyhow::anyhow!(
            "Webhook returned HTTP {}: {}",
            status,
            &text[..text.len().min(200)]
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_decision_from_json() {
        let json = r#"{"decision": "deny", "reason": "blocked by webhook"}"#;
        let d: HookDecision = serde_json::from_str(json).unwrap();
        assert!(d.is_deny());
        assert_eq!(d.reason.as_deref(), Some("blocked by webhook"));
    }

    #[test]
    fn test_hook_decision_allow() {
        let json = r#"{"decision": "allow"}"#;
        let d: HookDecision = serde_json::from_str(json).unwrap();
        assert!(d.is_allow());
    }
}
