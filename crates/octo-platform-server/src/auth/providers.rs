//! OAuth2 provider abstraction with Google and GitHub implementations.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[async_trait]
pub trait OAuthProvider: Send + Sync {
    fn name(&self) -> &str;
    fn auth_url(&self, state: &str, redirect_uri: &str) -> String;
    async fn exchange_code(&self, code: &str, redirect_uri: &str) -> Result<OAuthUser, OAuthError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthUser {
    pub provider: String,
    pub provider_user_id: String,
    pub email: String,
    pub name: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum OAuthError {
    #[error("Invalid authorization code")]
    InvalidCode,
    #[error("Network error: {0}")]
    NetworkError(String),
    #[error("Token exchange failed: {0}")]
    TokenExchangeFailed(String),
    #[error("User info request failed: {0}")]
    UserInfoFailed(String),
    #[error("Missing required field: {0}")]
    MissingField(String),
}

// ============== Google OAuth ==============

pub struct GoogleOAuthProvider {
    client_id: String,
    client_secret: String,
}

impl GoogleOAuthProvider {
    pub fn new(client_id: String, client_secret: String) -> Self {
        Self {
            client_id,
            client_secret,
        }
    }
}

#[async_trait]
impl OAuthProvider for GoogleOAuthProvider {
    fn name(&self) -> &str {
        "google"
    }

    fn auth_url(&self, state: &str, redirect_uri: &str) -> String {
        format!(
            "https://accounts.google.com/o/oauth2/v2/auth?\
             client_id={}&\
             redirect_uri={}&\
             response_type=code&\
             scope=openid%20email%20profile&\
             state={}",
            self.client_id,
            urlencoding::encode(redirect_uri),
            urlencoding::encode(state)
        )
    }

    async fn exchange_code(&self, code: &str, redirect_uri: &str) -> Result<OAuthUser, OAuthError> {
        // Exchange code for tokens
        let params = [
            ("client_id", self.client_id.as_str()),
            ("client_secret", self.client_secret.as_str()),
            ("code", code),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect_uri),
        ];

        let client = reqwest::Client::new();
        let response = client
            .post("https://oauth2.googleapis.com/token")
            .form(&params)
            .send()
            .await
            .map_err(|e| OAuthError::NetworkError(e.to_string()))?;

        let token_response: GoogleTokenResponse = response
            .json()
            .await
            .map_err(|e| OAuthError::TokenExchangeFailed(e.to_string()))?;

        // Get user info
        let user_response = client
            .get("https://www.googleapis.com/oauth2/v2/userinfo")
            .header(
                "Authorization",
                format!("Bearer {}", token_response.access_token),
            )
            .send()
            .await
            .map_err(|e| OAuthError::UserInfoFailed(e.to_string()))?;

        let user_info: GoogleUserInfo = user_response
            .json()
            .await
            .map_err(|e| OAuthError::UserInfoFailed(e.to_string()))?;

        Ok(OAuthUser {
            provider: "google".to_string(),
            provider_user_id: user_info.id,
            email: user_info.email,
            name: user_info.name,
        })
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GoogleTokenResponse {
    access_token: String,
    id_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GoogleUserInfo {
    id: String,
    email: String,
    name: Option<String>,
}

// ============== GitHub OAuth ==============

pub struct GitHubOAuthProvider {
    client_id: String,
    client_secret: String,
}

impl GitHubOAuthProvider {
    pub fn new(client_id: String, client_secret: String) -> Self {
        Self {
            client_id,
            client_secret,
        }
    }
}

#[async_trait]
impl OAuthProvider for GitHubOAuthProvider {
    fn name(&self) -> &str {
        "github"
    }

    fn auth_url(&self, state: &str, redirect_uri: &str) -> String {
        format!(
            "https://github.com/login/oauth/authorize?\
             client_id={}&\
             redirect_uri={}&\
             scope=read:user&\
             state={}",
            self.client_id,
            urlencoding::encode(redirect_uri),
            urlencoding::encode(state)
        )
    }

    async fn exchange_code(&self, code: &str, redirect_uri: &str) -> Result<OAuthUser, OAuthError> {
        // Exchange code for access token
        let params = [
            ("client_id", self.client_id.as_str()),
            ("client_secret", self.client_secret.as_str()),
            ("code", code),
            ("redirect_uri", redirect_uri),
        ];

        let client = reqwest::Client::new();
        let response = client
            .post("https://github.com/login/oauth/access_token")
            .header("Accept", "application/json")
            .form(&params)
            .send()
            .await
            .map_err(|e| OAuthError::NetworkError(e.to_string()))?;

        let token_response: GitHubTokenResponse = response
            .json()
            .await
            .map_err(|e| OAuthError::TokenExchangeFailed(e.to_string()))?;

        let access_token = token_response
            .access_token
            .ok_or(OAuthError::TokenExchangeFailed(
                "No access token in response".to_string(),
            ))?;

        // Get user info
        let user_response = client
            .get("https://api.github.com/user")
            .header("Authorization", format!("Bearer {}", access_token))
            .header("User-Agent", "octo-platform")
            .send()
            .await
            .map_err(|e| OAuthError::UserInfoFailed(e.to_string()))?;

        let user_info: GitHubUserInfo = user_response
            .json()
            .await
            .map_err(|e| OAuthError::UserInfoFailed(e.to_string()))?;

        // Get user email (if not public)
        let email = if let Some(email) = user_info.email {
            email
        } else {
            // Fetch email from separate endpoint
            let email_response = client
                .get("https://api.github.com/user/emails")
                .header("Authorization", format!("Bearer {}", access_token))
                .header("User-Agent", "octo-platform")
                .send()
                .await
                .map_err(|e| OAuthError::UserInfoFailed(e.to_string()))?;

            let emails: Vec<GitHubEmail> = email_response
                .json()
                .await
                .map_err(|e| OAuthError::UserInfoFailed(e.to_string()))?;

            emails
                .into_iter()
                .find(|e| e.primary)
                .map(|e| e.email)
                .ok_or_else(|| OAuthError::UserInfoFailed("No primary email found".to_string()))?
        };

        Ok(OAuthUser {
            provider: "github".to_string(),
            provider_user_id: user_info.id.to_string(),
            email,
            name: user_info.name,
        })
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GitHubTokenResponse {
    access_token: Option<String>,
    token_type: Option<String>,
    scope: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GitHubUserInfo {
    id: u64,
    name: Option<String>,
    email: Option<String>,
    login: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GitHubEmail {
    email: String,
    primary: bool,
    verified: bool,
}
