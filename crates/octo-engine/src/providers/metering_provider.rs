//! Metering provider wrapper that adds token usage tracking to any provider.

use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use async_trait::async_trait;

use crate::metering::Metering;
use crate::providers::traits::{CompletionStream, Provider};
use octo_types::message::ContentBlock;
use octo_types::CompletionRequest;
use octo_types::CompletionResponse;

/// A provider wrapper that adds metering to track token usage and request metrics.
pub struct MeteringProvider {
    inner: Box<dyn Provider>,
    metering: Arc<Metering>,
}

impl MeteringProvider {
    /// Create a new MeteringProvider wrapping the given provider.
    pub fn new(inner: Box<dyn Provider>, metering: Arc<Metering>) -> Self {
        Self { inner, metering }
    }

    /// Get a reference to the metering instance.
    pub fn metering_ref(&self) -> &Arc<Metering> {
        &self.metering
    }

    /// Get a snapshot of current metering values.
    pub fn snapshot(&self) -> crate::metering::MeteringSnapshot {
        self.metering.snapshot()
    }
}

#[async_trait]
impl Provider for MeteringProvider {
    fn id(&self) -> &str {
        self.inner.id()
    }

    fn metering(&self) -> Option<Arc<Metering>> {
        Some(Arc::clone(&self.metering))
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let start = Instant::now();

        // Get approximate input token count (this is an estimate - actual count would require a tokenizer)
        let input_tokens = estimate_token_count(&request);

        let result = self.inner.complete(request).await;
        let duration = start.elapsed().as_millis() as u64;

        match &result {
            Ok(response) => {
                let output_tokens = response.usage.output_tokens as usize;
                self.metering
                    .record_request(input_tokens, output_tokens, duration);
            }
            Err(_) => {
                self.metering.record_error();
            }
        }

        result
    }

    async fn stream(&self, request: CompletionRequest) -> Result<CompletionStream> {
        // For streaming, we track input tokens but output tokens are trickier
        // since they come incrementally. For simplicity, we'll track based on
        // the request and let the caller handle output tracking if needed.
        let input_tokens = estimate_token_count(&request);

        let start = Instant::now();
        let result = self.inner.stream(request).await;
        let duration = start.elapsed().as_millis() as u64;

        // For streaming, we record the request with 0 output tokens initially
        // The actual output would need to be tracked via a custom stream wrapper
        if result.is_ok() {
            self.metering.record_request(input_tokens, 0, duration);
        } else {
            self.metering.record_error();
        }

        result
    }

    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        self.inner.embed(texts).await
    }
}

/// Estimate token count from a request.
/// This is a rough estimate based on character count.
/// For accurate counting, a tokenizer would be needed.
fn estimate_token_count(request: &CompletionRequest) -> usize {
    let mut count = 0;

    // Estimate system prompt
    if let Some(ref system) = request.system {
        count += system.len();
    }

    // Estimate messages
    for msg in &request.messages {
        // Estimate from content
        for content in &msg.content {
            if let ContentBlock::Text { text } = content {
                count += text.len();
            }
        }
    }

    // Rough estimate: ~4 characters per token
    // This is a very rough approximation
    count / 4
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use octo_types::message::{ChatMessage, ContentBlock, MessageRole};
    use octo_types::provider::TokenUsage;
    use octo_types::StreamEvent;

    use super::*;

    struct MockProvider;

    #[async_trait]
    impl Provider for MockProvider {
        fn id(&self) -> &str {
            "mock"
        }

        async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse> {
            Ok(CompletionResponse {
                id: "test".into(),
                content: vec![ContentBlock::Text {
                    text: "Hello".into(),
                }],
                stop_reason: None,
                usage: TokenUsage {
                    input_tokens: 100,
                    output_tokens: 50,
                },
            })
        }

        async fn stream(
            &self,
            _request: CompletionRequest,
        ) -> Result<futures_util::stream::BoxStream<'static, Result<StreamEvent>>> {
            Err(anyhow::anyhow!("Not implemented"))
        }
    }

    #[tokio::test]
    async fn test_metering_provider_records_tokens() {
        let metering = Arc::new(Metering::new());
        let provider = MeteringProvider::new(Box::new(MockProvider), Arc::clone(&metering));

        let request = CompletionRequest {
            model: "test".into(),
            system: Some("You are a helpful assistant.".into()),
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: vec![ContentBlock::Text {
                    text: "Hello".into(),
                }],
            }],
            max_tokens: 100,
            temperature: None,
            tools: vec![],
            stream: false,
        };

        let _ = provider.complete(request).await;

        let snapshot = metering.snapshot();
        assert_eq!(snapshot.requests, 1);
        // We expect some token count from the estimation
        assert!(snapshot.input_tokens > 0);
        assert!(snapshot.output_tokens > 0);
    }

    #[tokio::test]
    async fn test_metering_provider_records_error() {
        struct ErrorProvider;

        #[async_trait]
        impl Provider for ErrorProvider {
            fn id(&self) -> &str {
                "error"
            }

            async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse> {
                Err(anyhow::anyhow!("Simulated error"))
            }

            async fn stream(
                &self,
                _request: CompletionRequest,
            ) -> Result<futures_util::stream::BoxStream<'static, Result<StreamEvent>>> {
                Err(anyhow::anyhow!("Not implemented"))
            }
        }

        let metering = Arc::new(Metering::new());
        let provider = MeteringProvider::new(Box::new(ErrorProvider), Arc::clone(&metering));

        let request = CompletionRequest::default();
        let _ = provider.complete(request).await;

        let snapshot = metering.snapshot();
        assert_eq!(snapshot.requests, 0);
        assert_eq!(snapshot.errors, 1);
    }

    #[tokio::test]
    async fn test_metering_provider_metering_method() {
        let metering = Arc::new(Metering::new());
        let provider = MeteringProvider::new(Box::new(MockProvider), Arc::clone(&metering));

        let metering_result = provider.metering();
        assert!(metering_result.is_some());
    }
}
