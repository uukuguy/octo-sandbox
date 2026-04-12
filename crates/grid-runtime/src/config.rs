//! grid-runtime configuration.
//!
//! Layered: environment variables > defaults.

use std::net::SocketAddr;

/// grid-runtime server configuration.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// gRPC listen address (default: 0.0.0.0:50051).
    pub grpc_addr: SocketAddr,
    /// Runtime instance identifier.
    pub runtime_id: String,
    /// LLM provider API key.
    pub api_key: Option<String>,
    /// LLM provider base URL (e.g. "https://openrouter.ai/api/v1").
    pub base_url: Option<String>,
    /// LLM provider (default: "openai").
    pub provider: String,
    /// LLM model (default: "gpt-4o").
    pub model: String,
}

impl RuntimeConfig {
    /// Load configuration from environment variables.
    pub fn from_env() -> Self {
        let _ = dotenvy::dotenv();

        let grpc_addr: SocketAddr = std::env::var("GRID_RUNTIME_ADDR")
            .unwrap_or_else(|_| "0.0.0.0:50051".into())
            .parse()
            .expect("Invalid GRID_RUNTIME_ADDR");

        let runtime_id =
            std::env::var("GRID_RUNTIME_ID").unwrap_or_else(|_| "grid-harness".into());

        // LLM provider configuration — follows .env conventions.
        // Env vars: LLM_PROVIDER, OPENAI_API_KEY, OPENAI_BASE_URL, OPENAI_MODEL_NAME,
        //           ANTHROPIC_API_KEY, ANTHROPIC_BASE_URL, ANTHROPIC_MODEL_NAME.
        // Missing required vars → panic with a clear message. No fallback.
        let provider = std::env::var("LLM_PROVIDER")
            .expect("LLM_PROVIDER is required (e.g. \"openai\" or \"anthropic\")");

        let (api_key_var, base_url_var, model_var) = match provider.as_str() {
            "anthropic" => ("ANTHROPIC_API_KEY", "ANTHROPIC_BASE_URL", "ANTHROPIC_MODEL_NAME"),
            _ => ("OPENAI_API_KEY", "OPENAI_BASE_URL", "OPENAI_MODEL_NAME"),
        };

        let api_key = std::env::var(api_key_var).ok();
        if api_key.is_none() {
            panic!(
                "{api_key_var} is required for LLM_PROVIDER={provider}. \
                 Set it in .env or shell environment."
            );
        }

        let base_url = std::env::var(base_url_var).ok();

        let model = std::env::var(model_var).unwrap_or_else(|_| {
            panic!(
                "{model_var} is required for LLM_PROVIDER={provider}. \
                 Set it in .env or shell environment."
            )
        });

        Self {
            grpc_addr,
            runtime_id,
            api_key,
            base_url,
            provider,
            model,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_with_explicit_vars() {
        std::env::remove_var("GRID_RUNTIME_ADDR");
        std::env::remove_var("GRID_RUNTIME_ID");
        std::env::set_var("LLM_PROVIDER", "openai");
        std::env::set_var("OPENAI_API_KEY", "test-key");
        std::env::set_var("OPENAI_MODEL_NAME", "gpt-4o");
        let config = RuntimeConfig::from_env();
        assert_eq!(config.grpc_addr.port(), 50051);
        assert_eq!(config.runtime_id, "grid-harness");
        assert_eq!(config.provider, "openai");
        assert_eq!(config.model, "gpt-4o");
        assert_eq!(config.api_key.as_deref(), Some("test-key"));
        // Cleanup
        std::env::remove_var("LLM_PROVIDER");
        std::env::remove_var("OPENAI_API_KEY");
        std::env::remove_var("OPENAI_MODEL_NAME");
    }
}
