use std::collections::HashMap;
use std::sync::LazyLock;

/// Known provider default configuration: base_url and API key environment variable.
pub struct ProviderDefaults {
    pub base_url: &'static str,
    pub api_key_env: &'static str,
}

static PROVIDER_DEFAULTS: LazyLock<HashMap<&str, ProviderDefaults>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert(
        "openai",
        ProviderDefaults {
            base_url: "https://api.openai.com/v1",
            api_key_env: "OPENAI_API_KEY",
        },
    );
    m.insert(
        "anthropic",
        ProviderDefaults {
            base_url: "https://api.anthropic.com",
            api_key_env: "ANTHROPIC_API_KEY",
        },
    );
    m.insert(
        "deepseek",
        ProviderDefaults {
            base_url: "https://api.deepseek.com/v1",
            api_key_env: "DEEPSEEK_API_KEY",
        },
    );
    m.insert(
        "ollama",
        ProviderDefaults {
            base_url: "http://localhost:11434/v1",
            api_key_env: "",
        },
    );
    m.insert(
        "azure",
        ProviderDefaults {
            base_url: "",
            api_key_env: "AZURE_OPENAI_API_KEY",
        },
    );
    m.insert(
        "together",
        ProviderDefaults {
            base_url: "https://api.together.xyz/v1",
            api_key_env: "TOGETHER_API_KEY",
        },
    );
    m.insert(
        "groq",
        ProviderDefaults {
            base_url: "https://api.groq.com/openai/v1",
            api_key_env: "GROQ_API_KEY",
        },
    );
    m.insert(
        "moonshot",
        ProviderDefaults {
            base_url: "https://api.moonshot.cn/v1",
            api_key_env: "MOONSHOT_API_KEY",
        },
    );
    m.insert(
        "zhipu",
        ProviderDefaults {
            base_url: "https://open.bigmodel.cn/api/paas/v4",
            api_key_env: "ZHIPU_API_KEY",
        },
    );
    m.insert(
        "qwen",
        ProviderDefaults {
            base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1",
            api_key_env: "DASHSCOPE_API_KEY",
        },
    );
    m.insert(
        "minimax",
        ProviderDefaults {
            base_url: "https://api.minimax.chat/v1",
            api_key_env: "MINIMAX_API_KEY",
        },
    );
    m.insert(
        "yi",
        ProviderDefaults {
            base_url: "https://api.lingyiwanwu.com/v1",
            api_key_env: "YI_API_KEY",
        },
    );
    m.insert(
        "baichuan",
        ProviderDefaults {
            base_url: "https://api.baichuan-ai.com/v1",
            api_key_env: "BAICHUAN_API_KEY",
        },
    );
    m.insert(
        "fireworks",
        ProviderDefaults {
            base_url: "https://api.fireworks.ai/inference/v1",
            api_key_env: "FIREWORKS_API_KEY",
        },
    );
    m.insert(
        "mistral",
        ProviderDefaults {
            base_url: "https://api.mistral.ai/v1",
            api_key_env: "MISTRAL_API_KEY",
        },
    );
    m
});

/// Resolve the base URL for a provider.
///
/// If `explicit_base_url` is provided and non-empty, it takes precedence.
/// Otherwise, the built-in mapping table is consulted.
/// Returns `None` if the provider is unknown or has no default base URL (e.g. Azure).
pub fn resolve_provider_url(name: &str, explicit_base_url: Option<&str>) -> Option<String> {
    if let Some(url) = explicit_base_url {
        if !url.is_empty() {
            return Some(url.to_string());
        }
    }
    PROVIDER_DEFAULTS
        .get(name.to_lowercase().as_str())
        .filter(|d| !d.base_url.is_empty())
        .map(|d| d.base_url.to_string())
}

/// Look up the default API key environment variable name for a provider.
///
/// Returns `None` for unknown providers or those without a key env (e.g. ollama).
pub fn resolve_api_key_env(name: &str) -> Option<&'static str> {
    PROVIDER_DEFAULTS
        .get(name.to_lowercase().as_str())
        .filter(|d| !d.api_key_env.is_empty())
        .map(|d| d.api_key_env)
}

/// Return the list of all known provider names.
pub fn known_providers() -> Vec<&'static str> {
    PROVIDER_DEFAULTS.keys().copied().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_known_provider() {
        let url = resolve_provider_url("openai", None);
        assert_eq!(url, Some("https://api.openai.com/v1".to_string()));

        let url = resolve_provider_url("anthropic", None);
        assert_eq!(url, Some("https://api.anthropic.com".to_string()));

        let url = resolve_provider_url("deepseek", None);
        assert_eq!(url, Some("https://api.deepseek.com/v1".to_string()));
    }

    #[test]
    fn test_resolve_with_explicit_url() {
        let explicit = "https://my-custom-endpoint.example.com/v1";
        let url = resolve_provider_url("openai", Some(explicit));
        assert_eq!(url, Some(explicit.to_string()));
    }

    #[test]
    fn test_resolve_explicit_empty_falls_back() {
        let url = resolve_provider_url("openai", Some(""));
        assert_eq!(url, Some("https://api.openai.com/v1".to_string()));
    }

    #[test]
    fn test_resolve_unknown_provider() {
        let url = resolve_provider_url("unknown-provider", None);
        assert_eq!(url, None);
    }

    #[test]
    fn test_resolve_azure_has_no_default_url() {
        let url = resolve_provider_url("azure", None);
        assert_eq!(url, None);
    }

    #[test]
    fn test_resolve_case_insensitive() {
        let url = resolve_provider_url("OpenAI", None);
        assert_eq!(url, Some("https://api.openai.com/v1".to_string()));

        let url = resolve_provider_url("ANTHROPIC", None);
        assert_eq!(url, Some("https://api.anthropic.com".to_string()));
    }

    #[test]
    fn test_resolve_api_key_env() {
        assert_eq!(resolve_api_key_env("openai"), Some("OPENAI_API_KEY"));
        assert_eq!(resolve_api_key_env("anthropic"), Some("ANTHROPIC_API_KEY"));
        assert_eq!(resolve_api_key_env("groq"), Some("GROQ_API_KEY"));
    }

    #[test]
    fn test_resolve_api_key_env_none_for_ollama() {
        assert_eq!(resolve_api_key_env("ollama"), None);
    }

    #[test]
    fn test_resolve_api_key_env_unknown() {
        assert_eq!(resolve_api_key_env("unknown"), None);
    }

    #[test]
    fn test_known_providers_not_empty() {
        let providers = known_providers();
        assert!(providers.len() >= 15);
        assert!(providers.contains(&"openai"));
        assert!(providers.contains(&"anthropic"));
    }
}
