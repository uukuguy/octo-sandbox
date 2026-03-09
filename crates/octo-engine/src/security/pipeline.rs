//! Composable safety check pipeline.
//!
//! Provides a layered safety pipeline that runs multiple [`SafetyLayer`] checks
//! in sequence. Each layer can Allow, Sanitize, Warn, or Block a message.

use async_trait::async_trait;

/// Decision returned by a safety check layer.
#[derive(Debug, Clone, PartialEq)]
pub enum SafetyDecision {
    /// Allow the content through unchanged.
    Allow,
    /// Allow after sanitizing — returns the cleaned content.
    Sanitize(String),
    /// Block the content entirely — returns the reason.
    Block(String),
    /// Warn but still allow — returns the warning message.
    Warn(String),
}

/// A single safety check layer in the pipeline.
#[async_trait]
pub trait SafetyLayer: Send + Sync {
    /// Human-readable name for this layer.
    fn name(&self) -> &str;

    /// Check an input message before it reaches the LLM.
    async fn check_input(&self, message: &str) -> SafetyDecision {
        let _ = message;
        SafetyDecision::Allow
    }

    /// Check an output response from the LLM.
    async fn check_output(&self, response: &str) -> SafetyDecision {
        let _ = response;
        SafetyDecision::Allow
    }

    /// Check a tool execution result.
    async fn check_tool_result(&self, tool_name: &str, result: &str) -> SafetyDecision {
        let _ = (tool_name, result);
        SafetyDecision::Allow
    }
}

/// Composable safety pipeline that executes layers in order.
///
/// Layers are checked sequentially. The pipeline short-circuits on [`SafetyDecision::Block`].
/// For non-blocking decisions the most restrictive result wins:
/// `Block > Sanitize > Warn > Allow`.
pub struct SafetyPipeline {
    layers: Vec<Box<dyn SafetyLayer>>,
}

impl SafetyPipeline {
    /// Create an empty pipeline (always allows).
    pub fn new() -> Self {
        Self { layers: vec![] }
    }

    /// Append a safety layer to the pipeline (builder pattern).
    pub fn add_layer(mut self, layer: Box<dyn SafetyLayer>) -> Self {
        self.layers.push(layer);
        self
    }

    /// Run all layers' input checks. Stops immediately on Block.
    pub async fn check_input(&self, message: &str) -> SafetyDecision {
        Self::run_checks(&self.layers, |layer| layer.check_input(message)).await
    }

    /// Run all layers' output checks. Stops immediately on Block.
    pub async fn check_output(&self, response: &str) -> SafetyDecision {
        Self::run_checks(&self.layers, |layer| layer.check_output(response)).await
    }

    /// Run all layers' tool-result checks. Stops immediately on Block.
    pub async fn check_tool_result(&self, tool_name: &str, result: &str) -> SafetyDecision {
        Self::run_checks(&self.layers, |layer| {
            layer.check_tool_result(tool_name, result)
        })
        .await
    }

    /// Generic runner: iterate layers, call `f` on each, merge decisions.
    async fn run_checks<'a, F, Fut>(layers: &'a [Box<dyn SafetyLayer>], f: F) -> SafetyDecision
    where
        F: Fn(&'a dyn SafetyLayer) -> Fut,
        Fut: std::future::Future<Output = SafetyDecision>,
    {
        let mut result = SafetyDecision::Allow;

        for layer in layers {
            let decision = f(layer.as_ref()).await;
            match &decision {
                SafetyDecision::Block(_) => return decision,
                SafetyDecision::Sanitize(_) => {
                    // Sanitize is more restrictive than Warn/Allow
                    if matches!(result, SafetyDecision::Allow | SafetyDecision::Warn(_)) {
                        result = decision;
                    }
                }
                SafetyDecision::Warn(_) => {
                    if matches!(result, SafetyDecision::Allow) {
                        result = decision;
                    }
                }
                SafetyDecision::Allow => {}
            }
        }

        result
    }
}

impl Default for SafetyPipeline {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Built-in layers
// ---------------------------------------------------------------------------

/// Credential scrubber layer — detects common API key and secret patterns.
pub struct CredentialScrubber;

impl CredentialScrubber {
    pub fn new() -> Self {
        Self
    }

    /// Check whether text contains a known credential pattern.
    fn contains_credential(text: &str) -> bool {
        const PATTERNS: &[&str] = &[
            "sk-ant-",
            "sk-",
            "AKIA",
            "ghp_",
            "gho_",
            "-----BEGIN PRIVATE KEY-----",
            "-----BEGIN RSA PRIVATE KEY-----",
        ];
        PATTERNS.iter().any(|p| text.contains(p))
    }
}

impl Default for CredentialScrubber {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SafetyLayer for CredentialScrubber {
    fn name(&self) -> &str {
        "credential-scrubber"
    }

    async fn check_input(&self, message: &str) -> SafetyDecision {
        if Self::contains_credential(message) {
            SafetyDecision::Block("Input contains potential credentials".into())
        } else {
            SafetyDecision::Allow
        }
    }

    async fn check_output(&self, response: &str) -> SafetyDecision {
        if Self::contains_credential(response) {
            SafetyDecision::Sanitize("[REDACTED: credential detected]".into())
        } else {
            SafetyDecision::Allow
        }
    }

    async fn check_tool_result(&self, _tool_name: &str, result: &str) -> SafetyDecision {
        if Self::contains_credential(result) {
            SafetyDecision::Sanitize("[REDACTED: credential detected]".into())
        } else {
            SafetyDecision::Allow
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_patterns_detected() {
        assert!(CredentialScrubber::contains_credential("key=sk-ant-abc123"));
        assert!(CredentialScrubber::contains_credential("token: ghp_xxxx"));
        assert!(CredentialScrubber::contains_credential(
            "AKIAIOSFODNN7EXAMPLE"
        ));
        assert!(CredentialScrubber::contains_credential(
            "-----BEGIN PRIVATE KEY-----\nMIIE..."
        ));
    }

    #[test]
    fn normal_text_not_flagged() {
        assert!(!CredentialScrubber::contains_credential("Hello world"));
        assert!(!CredentialScrubber::contains_credential(
            "The skeleton key opened the door"
        ));
    }
}
