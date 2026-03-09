//! Integration tests for the SafetyPipeline.

use async_trait::async_trait;
use octo_engine::security::{CredentialScrubber, SafetyDecision, SafetyLayer, SafetyPipeline};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// A layer that always allows.
struct AllowLayer;

#[async_trait]
impl SafetyLayer for AllowLayer {
    fn name(&self) -> &str {
        "allow"
    }
}

/// A layer that always blocks with a fixed reason.
struct BlockLayer(&'static str);

#[async_trait]
impl SafetyLayer for BlockLayer {
    fn name(&self) -> &str {
        "block"
    }

    async fn check_input(&self, _message: &str) -> SafetyDecision {
        SafetyDecision::Block(self.0.into())
    }

    async fn check_output(&self, _response: &str) -> SafetyDecision {
        SafetyDecision::Block(self.0.into())
    }

    async fn check_tool_result(&self, _tool_name: &str, _result: &str) -> SafetyDecision {
        SafetyDecision::Block(self.0.into())
    }
}

/// A layer that always warns.
struct WarnLayer(&'static str);

#[async_trait]
impl SafetyLayer for WarnLayer {
    fn name(&self) -> &str {
        "warn"
    }

    async fn check_input(&self, _message: &str) -> SafetyDecision {
        SafetyDecision::Warn(self.0.into())
    }

    async fn check_output(&self, _response: &str) -> SafetyDecision {
        SafetyDecision::Warn(self.0.into())
    }
}

/// A layer that sanitizes input by replacing it.
struct SanitizeLayer(&'static str);

#[async_trait]
impl SafetyLayer for SanitizeLayer {
    fn name(&self) -> &str {
        "sanitize"
    }

    async fn check_input(&self, _message: &str) -> SafetyDecision {
        SafetyDecision::Sanitize(self.0.into())
    }

    async fn check_output(&self, _response: &str) -> SafetyDecision {
        SafetyDecision::Sanitize(self.0.into())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn empty_pipeline_always_allows() {
    let pipeline = SafetyPipeline::new();

    assert_eq!(
        pipeline.check_input("anything").await,
        SafetyDecision::Allow
    );
    assert_eq!(
        pipeline.check_output("anything").await,
        SafetyDecision::Allow
    );
    assert_eq!(
        pipeline.check_tool_result("bash", "output").await,
        SafetyDecision::Allow
    );
}

#[tokio::test]
async fn single_block_layer_blocks() {
    let pipeline = SafetyPipeline::new().add_layer(Box::new(BlockLayer("forbidden")));

    assert_eq!(
        pipeline.check_input("test").await,
        SafetyDecision::Block("forbidden".into())
    );
}

#[tokio::test]
async fn multiple_layers_execute_in_order() {
    // Warn first, then Block — Block should win.
    let pipeline = SafetyPipeline::new()
        .add_layer(Box::new(WarnLayer("be careful")))
        .add_layer(Box::new(BlockLayer("stopped")));

    assert_eq!(
        pipeline.check_input("test").await,
        SafetyDecision::Block("stopped".into())
    );
}

#[tokio::test]
async fn sanitize_replaces_content() {
    let pipeline = SafetyPipeline::new().add_layer(Box::new(SanitizeLayer("[cleaned]")));

    assert_eq!(
        pipeline.check_input("dirty content").await,
        SafetyDecision::Sanitize("[cleaned]".into())
    );
}

#[tokio::test]
async fn block_takes_priority_over_warn() {
    // Block layer first, Warn layer second — Block short-circuits.
    let pipeline = SafetyPipeline::new()
        .add_layer(Box::new(BlockLayer("nope")))
        .add_layer(Box::new(WarnLayer("careful")));

    assert_eq!(
        pipeline.check_input("test").await,
        SafetyDecision::Block("nope".into())
    );
}

#[tokio::test]
async fn sanitize_takes_priority_over_warn() {
    let pipeline = SafetyPipeline::new()
        .add_layer(Box::new(WarnLayer("hmm")))
        .add_layer(Box::new(SanitizeLayer("[safe]")));

    assert_eq!(
        pipeline.check_input("test").await,
        SafetyDecision::Sanitize("[safe]".into())
    );
}

#[tokio::test]
async fn credential_scrubber_detects_api_key() {
    let pipeline = SafetyPipeline::new().add_layer(Box::new(CredentialScrubber::new()));

    // Input with API key should be blocked.
    assert_eq!(
        pipeline.check_input("my key is sk-ant-abc123xyz").await,
        SafetyDecision::Block("Input contains potential credentials".into())
    );

    // Output with API key should be sanitized.
    assert_eq!(
        pipeline
            .check_output("Here is the key: ghp_abcdef1234")
            .await,
        SafetyDecision::Sanitize("[REDACTED: credential detected]".into())
    );

    // Tool result with API key should be sanitized.
    assert_eq!(
        pipeline
            .check_tool_result("bash", "export TOKEN=AKIAIOSFODNN7EXAMPLE")
            .await,
        SafetyDecision::Sanitize("[REDACTED: credential detected]".into())
    );
}

#[tokio::test]
async fn credential_scrubber_allows_normal_content() {
    let pipeline = SafetyPipeline::new().add_layer(Box::new(CredentialScrubber::new()));

    assert_eq!(
        pipeline.check_input("Hello, how are you?").await,
        SafetyDecision::Allow
    );
    assert_eq!(
        pipeline.check_output("The weather is sunny today.").await,
        SafetyDecision::Allow
    );
    assert_eq!(
        pipeline
            .check_tool_result("ls", "file1.txt file2.txt")
            .await,
        SafetyDecision::Allow
    );
}

#[tokio::test]
async fn allow_layer_does_not_affect_result() {
    let pipeline = SafetyPipeline::new()
        .add_layer(Box::new(AllowLayer))
        .add_layer(Box::new(AllowLayer));

    assert_eq!(pipeline.check_input("test").await, SafetyDecision::Allow);
}
