//! JWT authentication.

use std::time::Duration;

use anyhow::{Context, Result};
use jsonwebtoken::{
    decode, encode, Algorithm, DecodingKey, EncodingKey, Header, TokenData, Validation,
};
use serde::{Deserialize, Serialize};

/// JWT claims
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,      // User ID
    pub email: String,    // User email
    pub role: String,     // User role
    pub tenant_id: String, // Tenant ID
    pub exp: i64,         // Expiration timestamp
    pub iat: i64,         // Issued at timestamp
}

/// JWT configuration
#[derive(Debug, Clone)]
pub struct JwtConfig {
    pub secret: String,
    pub access_token_expires: Duration,
    pub refresh_token_expires: Duration,
}

impl JwtConfig {
    /// Create from environment variable
    pub fn from_env() -> Result<Self> {
        let secret = std::env::var("OCTO_JWT_SECRET")
            .context("OCTO_JWT_SECRET environment variable is required")?;

        if secret.len() < 32 {
            anyhow::bail!("OCTO_JWT_SECRET must be at least 32 characters");
        }

        Ok(Self {
            secret,
            access_token_expires: Duration::from_secs(15 * 60), // 15 minutes
            refresh_token_expires: Duration::from_secs(7 * 24 * 60 * 60), // 7 days
        })
    }
}

/// JWT manager
#[derive(Debug)]
pub struct JwtManager {
    config: JwtConfig,
}

impl JwtManager {
    pub fn new(config: JwtConfig) -> Self {
        Self { config }
    }

    /// Generate access token
    pub fn generate_access_token(&self, user_id: &str, email: &str, role: &str, tenant_id: &str) -> Result<String> {
        let now = chrono::Utc::now();
        let claims = Claims {
            sub: user_id.to_string(),
            email: email.to_string(),
            role: role.to_string(),
            tenant_id: tenant_id.to_string(),
            exp: (now + self.config.access_token_expires).timestamp(),
            iat: now.timestamp(),
        };

        encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(self.config.secret.as_bytes()),
        )
        .context("generate access token")
    }

    /// Generate refresh token (longer lived)
    pub fn generate_refresh_token(&self, user_id: &str, email: &str, role: &str, tenant_id: &str) -> Result<String> {
        let now = chrono::Utc::now();
        let claims = Claims {
            sub: user_id.to_string(),
            email: email.to_string(),
            role: role.to_string(),
            tenant_id: tenant_id.to_string(),
            exp: (now + self.config.refresh_token_expires).timestamp(),
            iat: now.timestamp(),
        };

        encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(self.config.secret.as_bytes()),
        )
        .context("generate refresh token")
    }

    /// Verify and decode token
    pub fn verify_token(&self, token: &str) -> Result<TokenData<Claims>> {
        decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.config.secret.as_bytes()),
            &Validation::new(Algorithm::HS256),
        )
        .context("verify token")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jwt_flow() {
        let config = JwtConfig {
            secret: "test-secret-key-that-is-at-least-32-chars".to_string(),
            access_token_expires: Duration::from_secs(60),
            refresh_token_expires: Duration::from_secs(3600),
        };

        let manager = JwtManager::new(config);

        // Generate tokens
        let access = manager
            .generate_access_token("user1", "test@example.com", "member", "default")
            .unwrap();
        let refresh = manager
            .generate_refresh_token("user1", "test@example.com", "member", "default")
            .unwrap();

        // Verify access token
        let claims = manager.verify_token(&access).unwrap();
        assert_eq!(claims.claims.sub, "user1");
        assert_eq!(claims.claims.email, "test@example.com");
        assert_eq!(claims.claims.role, "member");
        assert_eq!(claims.claims.tenant_id, "default");

        // Verify refresh token
        let claims = manager.verify_token(&refresh).unwrap();
        assert_eq!(claims.claims.sub, "user1");
        assert_eq!(claims.claims.tenant_id, "default");
    }
}
