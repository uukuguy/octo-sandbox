pub mod anthropic;
pub mod chain;
pub mod config;
pub mod defaults;
pub mod metering_provider;
pub mod openai;
pub mod pipeline;
pub mod response_cache;
pub mod retry;
pub mod smart_router;
pub mod traits;
pub mod usage_recorder;

pub use anthropic::create_provider as create_anthropic_provider;
pub use chain::*;
pub use config::*;
pub use metering_provider::MeteringProvider;
pub use openai::create_openai_provider;
pub use pipeline::{CircuitBreakerConfig, CircuitState, CostBudget, ProviderPipelineBuilder};
pub use response_cache::ResponseCacheProvider;
pub use retry::{ErrorStrategy, LlmErrorKind, ProviderError, RetryInfo, RetryPolicy};
pub use smart_router::{
    AnalyzerThresholds, QueryAnalyzer, QueryComplexity, RouteDecision, SmartRouterProvider,
    SmartRoutingConfig, TierConfig,
};
pub use traits::{CompletionStream, Provider};
pub use usage_recorder::{UsageRecorderProvider, UsageStats};

/// Create a provider by name.
///
/// Supported providers: "anthropic", "openai".
/// Falls back to Anthropic if the name is unrecognized.
///
/// If `base_url` is `None`, the provider defaults table is consulted to
/// resolve a well-known base URL for the given `provider_name`.
pub fn create_provider(
    provider_name: &str,
    api_key: String,
    base_url: Option<String>,
) -> Box<dyn Provider> {
    let resolved_url = defaults::resolve_provider_url(
        provider_name,
        base_url.as_deref(),
    )
    .or(base_url);

    match provider_name {
        "openai" => create_openai_provider(api_key, resolved_url),
        _ => create_anthropic_provider(api_key, resolved_url),
    }
}
