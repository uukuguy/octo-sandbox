// crates/octo-engine/src/auth/api_key.rs

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use subtle::ConstantTimeEq;
use uuid::Uuid;

use super::roles::Role;

/// API Key 实体（数据库存储版本）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredApiKey {
    pub id: String,
    pub key_hash: String, // 存储 hash
    pub user_id: String,
    pub role: Role,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub description: Option<String>,
}

impl StoredApiKey {
    /// 生成新的 API Key
    pub fn generate(user_id: &str, role: Role) -> (Self, String) {
        let id = Uuid::new_v4().to_string();
        let key = Uuid::new_v4().to_string(); // 原始 key，返回给用户
        let key_hash = Self::hash_key(&key);

        let api_key = Self {
            id,
            key_hash,
            user_id: user_id.to_string(),
            role,
            created_at: Utc::now(),
            expires_at: None,
            last_used_at: None,
            description: None,
        };

        (api_key, key)
    }

    /// 生成带过期时间的 API Key
    pub fn generate_with_expiry(
        user_id: &str,
        role: Role,
        expires_at: DateTime<Utc>,
    ) -> (Self, String) {
        let (mut api_key, key) = Self::generate(user_id, role);
        api_key.expires_at = Some(expires_at);
        (api_key, key)
    }

    /// 验证 key 是否匹配（使用常量时间比较防止时序攻击）
    pub fn verify(&self, key: &str) -> bool {
        let input_hash = Self::hash_key(key);
        // 使用常量时间比较防止时序攻击
        // ConstantTimeEq is implemented for &[u8]
        input_hash.as_bytes().ct_eq(self.key_hash.as_bytes()).into()
    }

    /// 检查是否过期
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            expires_at < Utc::now()
        } else {
            false
        }
    }

    /// Hash key 用于存储
    fn hash_key(key: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// 设置描述
    pub fn with_description(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }

    /// 设置过期时间
    pub fn with_expiry(mut self, expires_at: DateTime<Utc>) -> Self {
        self.expires_at = Some(expires_at);
        self
    }
}

/// API Key 存储
pub struct ApiKeyStorage {
    conn: Connection,
}

impl ApiKeyStorage {
    /// 创建新的存储
    pub fn new(db_path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(db_path)?;
        let storage = Self { conn };
        storage.init_schema()?;
        Ok(storage)
    }

    /// 从已有连接创建存储
    pub fn from_connection(conn: Connection) -> Self {
        Self { conn }
    }

    /// 初始化数据库 schema
    fn init_schema(&self) -> rusqlite::Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS api_keys (
                id TEXT PRIMARY KEY,
                key_hash TEXT NOT NULL UNIQUE,
                user_id TEXT NOT NULL,
                role TEXT NOT NULL,
                created_at TEXT NOT NULL,
                expires_at TEXT,
                last_used_at TEXT,
                description TEXT
            )",
            [],
        )?;

        // 创建索引
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_api_keys_user_id ON api_keys(user_id)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_api_keys_key_hash ON api_keys(key_hash)",
            [],
        )?;

        Ok(())
    }

    /// 创建 API Key
    pub fn create(&self, api_key: &StoredApiKey) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO api_keys (id, key_hash, user_id, role, created_at, expires_at, last_used_at, description)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                api_key.id,
                api_key.key_hash,
                api_key.user_id,
                serde_json::to_string(&api_key.role).unwrap_or_default(),
                api_key.created_at.to_rfc3339(),
                api_key.expires_at.map(|dt| dt.to_rfc3339()),
                api_key.last_used_at.map(|dt| dt.to_rfc3339()),
                api_key.description,
            ],
        )?;
        Ok(())
    }

    /// 验证 API Key
    /// 返回 (user_id, role) 如果验证成功
    /// 验证成功后会更新 last_used_at
    pub fn verify(&self, key: &str) -> rusqlite::Result<Option<(String, Role)>> {
        let key_hash = StoredApiKey::hash_key(key);

        let mut stmt = self
            .conn
            .prepare("SELECT user_id, role, expires_at FROM api_keys WHERE key_hash = ?1")?;

        let result = stmt.query_row([&key_hash], |row| {
            let user_id: String = row.get(0)?;
            let role_str: String = row.get(1)?;
            let expires_at: Option<String> = row.get(2)?;

            Ok((user_id, role_str, expires_at))
        });

        match result {
            Ok((user_id, role_str, expires_at)) => {
                // 检查是否过期
                if let Some(expires_at_str) = expires_at {
                    if let Ok(expires_at) = DateTime::parse_from_rfc3339(&expires_at_str) {
                        if expires_at.with_timezone(&Utc) < Utc::now() {
                            return Ok(None); // 已过期
                        }
                    }
                }

                // 验证成功，更新最后使用时间
                let _ = self.update_last_used(&key_hash);

                // 解析 role
                let role: Role = serde_json::from_str(&role_str).unwrap_or_default();
                Ok(Some((user_id, role)))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// 更新最后使用时间
    pub fn update_last_used(&self, key_hash: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE api_keys SET last_used_at = ?1 WHERE key_hash = ?2",
            params![Utc::now().to_rfc3339(), key_hash],
        )?;
        Ok(())
    }

    /// 获取用户的所有 API Key
    pub fn list_by_user(&self, user_id: &str) -> rusqlite::Result<Vec<StoredApiKey>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, key_hash, user_id, role, created_at, expires_at, last_used_at, description
             FROM api_keys WHERE user_id = ?1 ORDER BY created_at DESC",
        )?;

        let rows = stmt.query_map([user_id], |row| {
            let role_str: String = row.get(3)?;
            let expires_at: Option<String> = row.get(5)?;
            let last_used_at: Option<String> = row.get(6)?;

            Ok(StoredApiKey {
                id: row.get(0)?,
                key_hash: row.get(1)?,
                user_id: row.get(2)?,
                role: serde_json::from_str(&role_str).unwrap_or_default(),
                created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                expires_at: expires_at
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&Utc)),
                last_used_at: last_used_at
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&Utc)),
                description: row.get(7)?,
            })
        })?;

        rows.collect()
    }

    /// 获取单个 API Key
    pub fn get(&self, id: &str) -> rusqlite::Result<Option<StoredApiKey>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, key_hash, user_id, role, created_at, expires_at, last_used_at, description
             FROM api_keys WHERE id = ?1",
        )?;

        let result = stmt.query_row([id], |row| {
            let role_str: String = row.get(3)?;
            let expires_at: Option<String> = row.get(5)?;
            let last_used_at: Option<String> = row.get(6)?;

            Ok(StoredApiKey {
                id: row.get(0)?,
                key_hash: row.get(1)?,
                user_id: row.get(2)?,
                role: serde_json::from_str(&role_str).unwrap_or_default(),
                created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                expires_at: expires_at
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&Utc)),
                last_used_at: last_used_at
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&Utc)),
                description: row.get(7)?,
            })
        });

        match result {
            Ok(api_key) => Ok(Some(api_key)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// 删除 API Key
    pub fn delete(&self, id: &str) -> rusqlite::Result<bool> {
        let rows_affected = self
            .conn
            .execute("DELETE FROM api_keys WHERE id = ?1", [id])?;
        Ok(rows_affected > 0)
    }

    /// 删除用户的所有 API Key
    pub fn delete_by_user(&self, user_id: &str) -> rusqlite::Result<usize> {
        let rows_affected = self
            .conn
            .execute("DELETE FROM api_keys WHERE user_id = ?1", [user_id])?;
        Ok(rows_affected)
    }
}

/// 用于 API 响应的 API Key（不包含敏感信息）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyResponse {
    pub id: String,
    pub user_id: String,
    pub role: Role,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub description: Option<String>,
}

impl From<StoredApiKey> for ApiKeyResponse {
    fn from(api_key: StoredApiKey) -> Self {
        Self {
            id: api_key.id,
            user_id: api_key.user_id,
            role: api_key.role,
            created_at: api_key.created_at,
            expires_at: api_key.expires_at,
            last_used_at: api_key.last_used_at,
            description: api_key.description,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_api_key_generate() {
        let (api_key, raw_key) = StoredApiKey::generate("user1", Role::User);

        assert!(!api_key.id.is_empty());
        assert!(!api_key.key_hash.is_empty());
        assert_eq!(api_key.user_id, "user1");
        assert_eq!(api_key.role, Role::User);
        assert!(!api_key.is_expired());

        // 验证 key 正确
        assert!(api_key.verify(&raw_key));
        assert!(!api_key.verify("wrong_key"));
    }

    #[test]
    fn test_api_key_verify() {
        let (api_key, raw_key) = StoredApiKey::generate("user1", Role::Admin);

        assert!(api_key.verify(&raw_key));
        assert!(!api_key.verify("invalid_key"));
        assert!(!api_key.verify(""));
    }

    #[test]
    fn test_api_key_expiry() {
        let past = Utc::now() - chrono::Duration::hours(1);
        let (api_key, _) = StoredApiKey::generate("user1", Role::User);
        let api_key = api_key.with_expiry(past);

        assert!(api_key.is_expired());

        let future = Utc::now() + chrono::Duration::hours(1);
        let (api_key2, _) = StoredApiKey::generate("user1", Role::User);
        let api_key2 = api_key2.with_expiry(future);

        assert!(!api_key2.is_expired());
    }

    #[test]
    fn test_storage_crud() {
        let temp_file = NamedTempFile::new().unwrap();
        let storage = ApiKeyStorage::new(temp_file.path()).unwrap();

        // 创建
        let (api_key, raw_key) = StoredApiKey::generate("user1", Role::User);
        let api_key_id = api_key.id.clone();
        storage.create(&api_key).unwrap();

        // 验证
        let result = storage.verify(&raw_key).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "user1");

        // 验证错误 key
        let result = storage.verify("wrong_key").unwrap();
        assert!(result.is_none());

        // 查询
        let keys = storage.list_by_user("user1").unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].id, api_key_id);

        // 删除
        let deleted = storage.delete(&api_key_id).unwrap();
        assert!(deleted);

        let keys = storage.list_by_user("user1").unwrap();
        assert!(keys.is_empty());
    }

    #[test]
    fn test_storage_expiry() {
        let temp_file = NamedTempFile::new().unwrap();
        let storage = ApiKeyStorage::new(temp_file.path()).unwrap();

        // 创建过期的 key
        let past = Utc::now() - chrono::Duration::hours(1);
        let (api_key, raw_key) = StoredApiKey::generate("user1", Role::User);
        let api_key = api_key.with_expiry(past);
        storage.create(&api_key).unwrap();

        // 验证过期 key
        let result = storage.verify(&raw_key).unwrap();
        assert!(result.is_none());
    }
}
