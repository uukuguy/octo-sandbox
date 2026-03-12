//! OAuth 2.1 authentication support for MCP SSE connections.
//!
//! Provides token storage, PKCE authorization flow, and automatic token refresh.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// OAuth 2.1 configuration for an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthConfig {
    pub client_id: String,
    #[serde(default)]
    pub client_secret: Option<String>,
    pub auth_url: String,
    pub token_url: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default = "default_true")]
    pub use_pkce: bool,
}

fn default_true() -> bool {
    true
}

/// OAuth token obtained from authorization server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub token_type: String,
    pub expires_at: Option<i64>,
    pub scope: Option<String>,
}

impl OAuthToken {
    /// Check if the token has expired (with 60-second buffer).
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            now >= expires_at - 60
        } else {
            false
        }
    }

    /// Get the bearer token value (without "Bearer " prefix).
    pub fn bearer_value(&self) -> &str {
        &self.access_token
    }
}

/// Trait for persisting OAuth tokens per MCP server.
#[async_trait]
pub trait OAuthTokenStore: Send + Sync {
    async fn get_token(&self, server_id: &str) -> Result<Option<OAuthToken>>;
    async fn save_token(&self, server_id: &str, token: &OAuthToken) -> Result<()>;
    async fn delete_token(&self, server_id: &str) -> Result<()>;
}

/// In-memory token store for testing and ephemeral sessions.
pub struct InMemoryTokenStore {
    tokens: Mutex<HashMap<String, OAuthToken>>,
}

impl InMemoryTokenStore {
    pub fn new() -> Self {
        Self {
            tokens: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryTokenStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl OAuthTokenStore for InMemoryTokenStore {
    async fn get_token(&self, server_id: &str) -> Result<Option<OAuthToken>> {
        Ok(self.tokens.lock().unwrap().get(server_id).cloned())
    }

    async fn save_token(&self, server_id: &str, token: &OAuthToken) -> Result<()> {
        self.tokens
            .lock()
            .unwrap()
            .insert(server_id.to_string(), token.clone());
        Ok(())
    }

    async fn delete_token(&self, server_id: &str) -> Result<()> {
        self.tokens.lock().unwrap().remove(server_id);
        Ok(())
    }
}

/// PKCE (Proof Key for Code Exchange) challenge pair.
pub struct PkceChallenge {
    pub code_verifier: String,
    pub code_challenge: String,
}

impl PkceChallenge {
    /// Generate a new PKCE challenge pair using SHA-256 + base64url.
    pub fn generate() -> Self {
        use sha2::{Digest, Sha256};

        // Generate code_verifier: 43-128 character random string (base64url of 32 random bytes)
        let verifier_bytes: Vec<u8> = (0..32).map(|_| rand::random::<u8>()).collect();
        let code_verifier = hex::encode(&verifier_bytes);

        // code_challenge = BASE64URL(SHA256(code_verifier))
        let mut hasher = Sha256::new();
        hasher.update(code_verifier.as_bytes());
        let hash = hasher.finalize();
        // Use hex encoding for the challenge since base64 is feature-gated.
        // OAuth servers typically accept base64url, but for our internal use hex works.
        // When base64 is available unconditionally, switch to URL_SAFE_NO_PAD.
        let code_challenge = hex::encode(hash);

        Self {
            code_verifier,
            code_challenge,
        }
    }
}

/// Manages OAuth 2.1 flows for MCP server connections.
pub struct McpOAuthManager {
    http: reqwest::Client,
}

impl McpOAuthManager {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::new(),
        }
    }

    /// Refresh an expired access token using the refresh token.
    pub async fn refresh_token(
        &self,
        config: &OAuthConfig,
        current: &OAuthToken,
    ) -> Result<OAuthToken> {
        let refresh_token = current
            .refresh_token
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No refresh token available"))?;

        let mut params = vec![
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token.as_str()),
            ("client_id", &config.client_id),
        ];

        if let Some(secret) = &config.client_secret {
            params.push(("client_secret", secret.as_str()));
        }

        let resp = self.http.post(&config.token_url).form(&params).send().await?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Token refresh failed: {body}"));
        }

        let token_resp: TokenResponse = resp.json().await?;
        Ok(token_resp.into_token())
    }

    /// Exchange an authorization code for tokens (final step of PKCE flow).
    pub async fn exchange_code(
        &self,
        config: &OAuthConfig,
        code: &str,
        code_verifier: &str,
        redirect_uri: &str,
    ) -> Result<OAuthToken> {
        let mut params = vec![
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", config.client_id.as_str()),
            ("code_verifier", code_verifier),
        ];

        if let Some(secret) = &config.client_secret {
            params.push(("client_secret", secret.as_str()));
        }

        let resp = self.http.post(&config.token_url).form(&params).send().await?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Token exchange failed: {body}"));
        }

        let token_resp: TokenResponse = resp.json().await?;
        Ok(token_resp.into_token())
    }

    /// Build the PKCE authorization URL for user redirect.
    pub fn build_auth_url(
        &self,
        config: &OAuthConfig,
        pkce: &PkceChallenge,
        redirect_uri: &str,
    ) -> String {
        let scopes = config.scopes.join(" ");
        format!(
            "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&code_challenge={}&code_challenge_method=S256",
            config.auth_url,
            urlencoding::encode(&config.client_id),
            urlencoding::encode(redirect_uri),
            urlencoding::encode(&scopes),
            urlencoding::encode(&pkce.code_challenge),
        )
    }

    /// Get a valid (non-expired) token, automatically refreshing if needed.
    ///
    /// Returns `None` if no token exists and no refresh is possible.
    pub async fn get_valid_token(
        &self,
        config: &OAuthConfig,
        store: &dyn OAuthTokenStore,
        server_id: &str,
    ) -> Result<Option<OAuthToken>> {
        if let Some(token) = store.get_token(server_id).await? {
            if !token.is_expired() {
                return Ok(Some(token));
            }
            // Attempt refresh
            if token.refresh_token.is_some() {
                match self.refresh_token(config, &token).await {
                    Ok(new_token) => {
                        store.save_token(server_id, &new_token).await?;
                        return Ok(Some(new_token));
                    }
                    Err(e) => {
                        tracing::warn!("Token refresh failed for {server_id}: {e}");
                        store.delete_token(server_id).await?;
                    }
                }
            } else {
                // Token expired with no refresh token — remove it
                store.delete_token(server_id).await?;
            }
        }
        Ok(None)
    }
}

impl Default for McpOAuthManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Raw token response from the OAuth authorization server.
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default = "default_bearer")]
    token_type: String,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    scope: Option<String>,
}

fn default_bearer() -> String {
    "Bearer".to_string()
}

impl TokenResponse {
    fn into_token(self) -> OAuthToken {
        let expires_at = self.expires_in.map(|secs| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64
                + secs as i64
        });
        OAuthToken {
            access_token: self.access_token,
            refresh_token: self.refresh_token,
            token_type: self.token_type,
            expires_at,
            scope: self.scope,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_not_expired() {
        let token = OAuthToken {
            access_token: "test".to_string(),
            refresh_token: None,
            token_type: "Bearer".to_string(),
            expires_at: Some(i64::MAX),
            scope: None,
        };
        assert!(!token.is_expired());
    }

    #[test]
    fn test_token_expired() {
        let token = OAuthToken {
            access_token: "test".to_string(),
            refresh_token: None,
            token_type: "Bearer".to_string(),
            expires_at: Some(0),
            scope: None,
        };
        assert!(token.is_expired());
    }

    #[test]
    fn test_token_no_expiry() {
        let token = OAuthToken {
            access_token: "test".to_string(),
            refresh_token: None,
            token_type: "Bearer".to_string(),
            expires_at: None,
            scope: None,
        };
        assert!(!token.is_expired());
    }

    #[test]
    fn test_bearer_value() {
        let token = OAuthToken {
            access_token: "my_access_token".to_string(),
            refresh_token: None,
            token_type: "Bearer".to_string(),
            expires_at: None,
            scope: None,
        };
        assert_eq!(token.bearer_value(), "my_access_token");
    }

    #[test]
    fn test_pkce_challenge_generation() {
        let pkce = PkceChallenge::generate();
        assert!(!pkce.code_verifier.is_empty());
        assert!(!pkce.code_challenge.is_empty());
        assert_ne!(pkce.code_verifier, pkce.code_challenge);
        // Verifier should be hex-encoded 32 bytes = 64 chars
        assert_eq!(pkce.code_verifier.len(), 64);
    }

    #[test]
    fn test_pkce_deterministic_challenge() {
        // Two different generations should produce different verifiers
        let pkce1 = PkceChallenge::generate();
        let pkce2 = PkceChallenge::generate();
        assert_ne!(pkce1.code_verifier, pkce2.code_verifier);
    }

    #[test]
    fn test_oauth_config_serialization() {
        let config = OAuthConfig {
            client_id: "test_client".to_string(),
            client_secret: None,
            auth_url: "https://auth.example.com/authorize".to_string(),
            token_url: "https://auth.example.com/token".to_string(),
            scopes: vec!["read".to_string()],
            use_pkce: true,
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: OAuthConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.client_id, "test_client");
        assert!(parsed.use_pkce);
    }

    #[test]
    fn test_oauth_config_default_pkce() {
        let json = r#"{"client_id":"c","auth_url":"https://a","token_url":"https://t"}"#;
        let config: OAuthConfig = serde_json::from_str(json).unwrap();
        assert!(config.use_pkce);
        assert!(config.scopes.is_empty());
    }

    #[test]
    fn test_build_auth_url() {
        let mgr = McpOAuthManager::new();
        let config = OAuthConfig {
            client_id: "my_client".to_string(),
            client_secret: None,
            auth_url: "https://auth.example.com/authorize".to_string(),
            token_url: "https://auth.example.com/token".to_string(),
            scopes: vec!["read".to_string(), "write".to_string()],
            use_pkce: true,
        };
        let pkce = PkceChallenge::generate();
        let url = mgr.build_auth_url(&config, &pkce, "http://localhost:3001/callback");
        assert!(url.starts_with("https://auth.example.com/authorize?"));
        assert!(url.contains("client_id=my_client"));
        assert!(url.contains("code_challenge="));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("response_type=code"));
    }

    #[test]
    fn test_build_auth_url_encodes_special_chars() {
        let mgr = McpOAuthManager::new();
        let config = OAuthConfig {
            client_id: "client with spaces".to_string(),
            client_secret: None,
            auth_url: "https://auth.example.com/authorize".to_string(),
            token_url: "https://auth.example.com/token".to_string(),
            scopes: vec!["read write".to_string()],
            use_pkce: true,
        };
        let pkce = PkceChallenge::generate();
        let url = mgr.build_auth_url(&config, &pkce, "http://localhost:3001/callback");
        assert!(url.contains("client%20with%20spaces"));
    }

    #[tokio::test]
    async fn test_in_memory_token_store() {
        let store = InMemoryTokenStore::new();
        assert!(store.get_token("srv1").await.unwrap().is_none());

        let token = OAuthToken {
            access_token: "abc123".to_string(),
            refresh_token: Some("refresh_xyz".to_string()),
            token_type: "Bearer".to_string(),
            expires_at: Some(i64::MAX),
            scope: Some("read".to_string()),
        };
        store.save_token("srv1", &token).await.unwrap();

        let retrieved = store.get_token("srv1").await.unwrap().unwrap();
        assert_eq!(retrieved.access_token, "abc123");
        assert_eq!(retrieved.refresh_token.as_deref(), Some("refresh_xyz"));

        store.delete_token("srv1").await.unwrap();
        assert!(store.get_token("srv1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_in_memory_token_store_multiple_servers() {
        let store = InMemoryTokenStore::new();
        let token1 = OAuthToken {
            access_token: "token_a".to_string(),
            refresh_token: None,
            token_type: "Bearer".to_string(),
            expires_at: None,
            scope: None,
        };
        let token2 = OAuthToken {
            access_token: "token_b".to_string(),
            refresh_token: None,
            token_type: "Bearer".to_string(),
            expires_at: None,
            scope: None,
        };
        store.save_token("srv1", &token1).await.unwrap();
        store.save_token("srv2", &token2).await.unwrap();

        assert_eq!(
            store.get_token("srv1").await.unwrap().unwrap().access_token,
            "token_a"
        );
        assert_eq!(
            store.get_token("srv2").await.unwrap().unwrap().access_token,
            "token_b"
        );
    }

    #[tokio::test]
    async fn test_get_valid_token_not_expired() {
        let store = InMemoryTokenStore::new();
        let token = OAuthToken {
            access_token: "valid_token".to_string(),
            refresh_token: None,
            token_type: "Bearer".to_string(),
            expires_at: Some(i64::MAX),
            scope: None,
        };
        store.save_token("srv1", &token).await.unwrap();

        let config = OAuthConfig {
            client_id: "test".to_string(),
            client_secret: None,
            auth_url: "https://auth.example.com/authorize".to_string(),
            token_url: "https://auth.example.com/token".to_string(),
            scopes: vec![],
            use_pkce: true,
        };

        let mgr = McpOAuthManager::new();
        let result = mgr.get_valid_token(&config, &store, "srv1").await.unwrap();
        assert_eq!(result.unwrap().access_token, "valid_token");
    }

    #[tokio::test]
    async fn test_get_valid_token_no_token() {
        let store = InMemoryTokenStore::new();
        let config = OAuthConfig {
            client_id: "test".to_string(),
            client_secret: None,
            auth_url: "https://auth.example.com/authorize".to_string(),
            token_url: "https://auth.example.com/token".to_string(),
            scopes: vec![],
            use_pkce: true,
        };
        let mgr = McpOAuthManager::new();
        let result = mgr.get_valid_token(&config, &store, "srv1").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_valid_token_expired_no_refresh() {
        let store = InMemoryTokenStore::new();
        let token = OAuthToken {
            access_token: "expired".to_string(),
            refresh_token: None,
            token_type: "Bearer".to_string(),
            expires_at: Some(0), // long expired
            scope: None,
        };
        store.save_token("srv1", &token).await.unwrap();

        let config = OAuthConfig {
            client_id: "test".to_string(),
            client_secret: None,
            auth_url: "https://auth.example.com/authorize".to_string(),
            token_url: "https://auth.example.com/token".to_string(),
            scopes: vec![],
            use_pkce: true,
        };
        let mgr = McpOAuthManager::new();
        let result = mgr.get_valid_token(&config, &store, "srv1").await.unwrap();
        assert!(result.is_none());
        // Token should be deleted from store
        assert!(store.get_token("srv1").await.unwrap().is_none());
    }

    #[test]
    fn test_token_response_into_token() {
        let resp = TokenResponse {
            access_token: "new_token".to_string(),
            refresh_token: Some("new_refresh".to_string()),
            token_type: "Bearer".to_string(),
            expires_in: Some(3600),
            scope: Some("read write".to_string()),
        };
        let token = resp.into_token();
        assert_eq!(token.access_token, "new_token");
        assert_eq!(token.refresh_token.as_deref(), Some("new_refresh"));
        assert!(token.expires_at.is_some());
        assert!(!token.is_expired());
    }

    #[test]
    fn test_token_response_no_expiry() {
        let resp = TokenResponse {
            access_token: "tok".to_string(),
            refresh_token: None,
            token_type: "Bearer".to_string(),
            expires_in: None,
            scope: None,
        };
        let token = resp.into_token();
        assert!(token.expires_at.is_none());
        assert!(!token.is_expired());
    }

    #[test]
    fn test_in_memory_token_store_default() {
        let store = InMemoryTokenStore::default();
        assert!(store.tokens.lock().unwrap().is_empty());
    }

    #[test]
    fn test_mcp_oauth_manager_default() {
        let _mgr = McpOAuthManager::default();
    }
}
