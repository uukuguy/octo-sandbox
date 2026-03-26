#[cfg(test)]
mod tests {
    use crate::secret::{CredentialResolver, CredentialVault};

    #[test]
    fn test_resolver_priority() {
        // Priority: Vault -> Env -> Dotenv
        let resolver = CredentialResolver::new();

        // Without vault, should return None
        let result = resolver.resolve("api_key");
        assert!(result.is_none());
    }

    #[test]
    fn test_resolve_secret_syntax() {
        let resolver = CredentialResolver::new();

        let config = "api_key=${SECRET:openai_key}&other=value";
        let resolved = resolver.resolve_config(config);

        // Should replace ${SECRET:xxx} syntax
        assert!(!resolved.contains("${SECRET:"));
    }

    #[test]
    fn test_resolver_with_vault() {
        let vault = CredentialVault::new_for_testing("test_password".to_string());
        vault.set("openai_key", "sk-test-123").unwrap();

        let resolver = CredentialResolver::new().with_vault(vault);

        let result = resolver.resolve("openai_key");
        assert!(result.is_some());
        assert_eq!(result.unwrap().as_str(), "sk-test-123");
    }

    #[test]
    fn test_resolver_priority_order() {
        // Create vault with one value
        let vault = CredentialVault::new_for_testing("test_password".to_string());
        vault.set("test_key", "vault_value").unwrap();

        // Test priority: Vault > Env > Dotenv
        let resolver = CredentialResolver::new().with_vault(vault);

        // Vault should have the value
        let result = resolver.resolve("test_key");
        assert!(result.is_some());
        assert_eq!(result.unwrap().as_str(), "vault_value");
    }

    #[test]
    fn test_resolve_config_multiple_secrets() {
        let resolver = CredentialResolver::new();

        let config = "api_key=${SECRET:key1}&secret=${SECRET:key2}&other=value";
        let resolved = resolver.resolve_config(config);

        // Should handle multiple secret placeholders
        assert!(resolved.contains("api_key="));
        assert!(resolved.contains("&secret="));
        assert!(resolved.contains("&other=value"));
    }

    #[test]
    fn test_resolver_default() {
        let resolver = CredentialResolver::default();

        // Default resolver should have no vault or dotenv
        let result = resolver.resolve("nonexistent_key");
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_dotenv() {
        let content = "
# Comment line
export DB_HOST=localhost
API_KEY=\"secret-value\"
SINGLE_QUOTED='hello world'
";
        let vars = CredentialResolver::parse_dotenv(content);

        assert_eq!(vars.get("DB_HOST"), Some(&"localhost".to_string()));
        assert_eq!(vars.get("API_KEY"), Some(&"secret-value".to_string()));
        assert_eq!(vars.get("SINGLE_QUOTED"), Some(&"hello world".to_string()));
    }

    #[test]
    fn test_parse_dotenv_edge_cases() {
        // Empty values, whitespace, etc.
        let content = "
EMPTY=
SPACED_KEY = spaced_value
# This is a comment
NOSPACE=works
";
        let vars = CredentialResolver::parse_dotenv(content);

        assert_eq!(vars.get("EMPTY"), Some(&"".to_string()));
        assert_eq!(vars.get("SPACED_KEY"), Some(&"spaced_value".to_string()));
        assert_eq!(vars.get("NOSPACE"), Some(&"works".to_string()));
    }
}
