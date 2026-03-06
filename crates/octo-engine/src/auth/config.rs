// crates/octo-engine/src/auth/config.rs

use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashMap;

type HmacSha256 = Hmac<Sha256>;

/// Compute HMAC-SHA256 of `key` using `secret`. Returns a lowercase hex string.
fn hash_api_key(key: &str, secret: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(key.as_bytes());
    format!("{:x}", mac.finalize().into_bytes())
}

/// Default HMAC secret used when `OCTO_HMAC_SECRET` is not set.
/// In production, always set the environment variable.
const DEFAULT_HMAC_SECRET: &str = "octo-default-hmac-secret-change-in-production";

use super::roles::Role;

/// JWT claims structure
#[derive(Debug, Serialize, Deserialize)]
pub struct JwtClaims {
    pub sub: String,   // User ID
    pub email: String,  // User email
    pub role: String,   // User role
    pub exp: i64,       // Expiration timestamp
    pub iat: i64,      // Issued at timestamp
}

/// 认证模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    /// 无认证
    None,
    /// API Key 模式（默认）
    #[default]
    ApiKey,
    /// 完整认证（保留给 octo-platform）
    Full,
}

/// 权限
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    Read,
    Write,
    Admin,
}

impl Permission {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "read" => Some(Permission::Read),
            "write" => Some(Permission::Write),
            "admin" => Some(Permission::Admin),
            _ => None,
        }
    }
}

/// API Key
#[derive(Debug, Clone)]
pub struct ApiKey {
    pub key_hash: String,        // HMAC-SHA256 哈希存储
    pub user_id: Option<String>, // 可选用户绑定
    pub permissions: Vec<Permission>,
    pub role: Option<Role>, // 角色信息（用于 RBAC）
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl ApiKey {
    pub fn new(key: &str, secret: &str, user_id: Option<String>, permissions: Vec<Permission>) -> Self {
        let key_hash = hash_api_key(key, secret);

        Self {
            key_hash,
            user_id,
            permissions,
            role: None,
            expires_at: None,
            created_at: Utc::now(),
        }
    }

    pub fn with_expiry(mut self, expires_at: DateTime<Utc>) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    pub fn with_role(mut self, role: Role) -> Self {
        self.role = Some(role);
        self
    }

    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            expires_at < Utc::now()
        } else {
            false
        }
    }
}

/// 认证配置
#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub mode: AuthMode,
    pub api_keys: HashMap<String, ApiKey>, // key_hash -> ApiKey
    pub require_user_id: bool,            // 是否要求用户隔离
    pub jwt_secret: Option<String>,        // JWT secret for Full mode
    /// HMAC secret used when hashing API keys. Loaded from `OCTO_HMAC_SECRET` env var.
    /// Falls back to a hardcoded default with a warning — always set this in production.
    pub hmac_secret: String,
}

impl Default for AuthConfig {
    fn default() -> Self {
        let hmac_secret = std::env::var("OCTO_HMAC_SECRET").unwrap_or_else(|_| {
            tracing::warn!(
                "OCTO_HMAC_SECRET is not set. Using insecure default HMAC secret. \
                 Set this environment variable in production."
            );
            DEFAULT_HMAC_SECRET.to_string()
        });
        Self {
            mode: AuthMode::default(),
            api_keys: HashMap::new(),
            require_user_id: false,
            jwt_secret: None,
            hmac_secret,
        }
    }
}

impl AuthConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_mode(mut self, mode: AuthMode) -> Self {
        self.mode = mode;
        self
    }

    /// Log a warning if auth mode is None (insecure)
    pub fn warn_if_insecure(&self) {
        if self.mode == AuthMode::None {
            tracing::warn!("Authentication is DISABLED (mode=none). All API endpoints are publicly accessible. Set auth.mode to 'api_key' for production use.");
        }
    }

    /// 添加 API Key（带角色）
    pub fn add_api_key(
        &mut self,
        key: &str,
        user_id: Option<String>,
        permissions: Vec<Permission>,
    ) {
        let api_key = ApiKey::new(key, &self.hmac_secret, user_id, permissions);
        self.api_keys.insert(api_key.key_hash.clone(), api_key);
    }

    /// 添加 API Key（带角色）
    pub fn add_api_key_with_role(
        &mut self,
        key: &str,
        user_id: Option<String>,
        permissions: Vec<Permission>,
        role: Option<Role>,
    ) {
        let mut api_key = ApiKey::new(key, &self.hmac_secret, user_id, permissions);
        if let Some(r) = role {
            api_key = api_key.with_role(r);
        }
        self.api_keys.insert(api_key.key_hash.clone(), api_key);
    }

    /// 验证 API Key
    pub fn validate_key(&self, key: &str) -> bool {
        if self.mode != AuthMode::ApiKey {
            return self.mode == AuthMode::None;
        }

        let key_hash = hash_api_key(key, &self.hmac_secret);

        if let Some(api_key) = self.api_keys.get(&key_hash) {
            !api_key.is_expired()
        } else {
            false
        }
    }

    /// 获取用户 ID
    pub fn get_user_id(&self, key: &str) -> Option<String> {
        let key_hash = hash_api_key(key, &self.hmac_secret);
        self.api_keys.get(&key_hash).and_then(|k| k.user_id.clone())
    }

    /// 获取权限
    pub fn get_permissions(&self, key: &str) -> Vec<Permission> {
        let key_hash = hash_api_key(key, &self.hmac_secret);
        self.api_keys
            .get(&key_hash)
            .map(|k| k.permissions.clone())
            .unwrap_or_default()
    }

    /// 获取角色
    pub fn get_role(&self, key: &str) -> Option<Role> {
        let key_hash = hash_api_key(key, &self.hmac_secret);
        self.api_keys.get(&key_hash).and_then(|k| k.role)
    }

    /// 验证 JWT token（用于 Full 模式）
    pub fn validate_jwt(&self, token: &str) -> Option<JwtClaims> {
        let secret = self.jwt_secret.as_ref()?;

        match decode::<JwtClaims>(
            token,
            &DecodingKey::from_secret(secret.as_bytes()),
            &Validation::new(Algorithm::HS256),
        ) {
            Ok(token_data) => Some(token_data.claims),
            Err(_) => None,
        }
    }
}

/// API Key 配置（用于配置文件）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyConfig {
    pub key: String, // 原始 key（加载时哈希）
    pub user_id: Option<String>,
    pub permissions: Vec<String>,
    pub role: Option<String>, // 角色: viewer, user, admin, owner
    pub expires_at: Option<String>,
}

/// 认证配置 - 可序列化
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuthConfigYaml {
    pub mode: Option<AuthMode>,
    pub require_user_id: Option<bool>,
    pub api_keys: Option<Vec<ApiKeyConfig>>,
}

impl AuthConfigYaml {
    /// 转换为运行时配置
    pub fn to_auth_config(&self) -> AuthConfig {
        let mut config = AuthConfig::new();

        if let Some(mode) = self.mode {
            config.mode = mode;
        }

        if let Some(require_user_id) = self.require_user_id {
            config.require_user_id = require_user_id;
        }

        if let Some(api_keys) = &self.api_keys {
            for key_config in api_keys {
                let permissions: Vec<Permission> = key_config
                    .permissions
                    .iter()
                    .filter_map(|p| Permission::from_str(p))
                    .collect();

                // 解析角色
                let role = key_config.role.as_ref().and_then(|r| Role::from_str(r));

                config.add_api_key_with_role(
                    &key_config.key,
                    key_config.user_id.clone(),
                    permissions,
                    role,
                );
            }
        }

        config
    }
}
