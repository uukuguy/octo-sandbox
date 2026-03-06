//! User database operations.

use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result};
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// User role
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    Admin,
    Member,
    Viewer,
}

impl Default for UserRole {
    fn default() -> Self {
        UserRole::Member
    }
}

impl std::fmt::Display for UserRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserRole::Admin => write!(f, "admin"),
            UserRole::Member => write!(f, "member"),
            UserRole::Viewer => write!(f, "viewer"),
        }
    }
}

impl From<&str> for UserRole {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "admin" => UserRole::Admin,
            "viewer" => UserRole::Viewer,
            _ => UserRole::Member,
        }
    }
}

/// Platform user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub tenant_id: String,
    pub id: String,
    pub email: String,
    pub password_hash: String,
    pub display_name: String,
    pub role: UserRole,
    pub created_at: DateTime<Utc>,
}

/// User registration request
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub display_name: Option<String>,
}

/// User login request
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
    pub tenant_id: Option<String>,
}

/// User update request
#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub role: Option<String>,
}

/// Paginated user list response
#[derive(Debug, Serialize)]
pub struct PaginatedUsersResponse {
    pub users: Vec<UserResponse>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
    pub total_pages: i64,
}

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub tenant_id: String,
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub role: UserRole,
    pub created_at: DateTime<Utc>,
}

impl From<User> for UserResponse {
    fn from(user: User) -> Self {
        Self {
            tenant_id: user.tenant_id,
            id: user.id,
            email: user.email,
            display_name: user.display_name,
            role: user.role,
            created_at: user.created_at,
        }
    }
}

/// User database manager
pub struct UserDatabase {
    conn: Mutex<Connection>,
}

impl std::fmt::Debug for UserDatabase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserDatabase").finish()
    }
}

impl UserDatabase {
    /// Open or create user database
    pub fn open(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir).context("create data dir")?;
        let db_path = data_dir.join("users.db");
        let conn = Connection::open(&db_path).context("open users.db")?;

        let db = Self {
            conn: Mutex::new(conn),
        };
        db.init_schema()?;
        Ok(db)
    }

    /// Initialize database schema
    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "CREATE TABLE IF NOT EXISTS users (
                tenant_id TEXT NOT NULL DEFAULT 'default',
                id TEXT PRIMARY KEY,
                email TEXT UNIQUE NOT NULL,
                password_hash TEXT NOT NULL,
                display_name TEXT NOT NULL,
                role TEXT NOT NULL DEFAULT 'member',
                created_at TEXT NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_users_email ON users(email)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_users_tenant ON users(tenant_id)",
            [],
        )?;

        tracing::info!("User database schema initialized");
        Ok(())
    }

    /// Register a new user
    pub fn register(&self, req: &RegisterRequest, tenant_id: Option<&str>) -> Result<UserResponse> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());

        let tenant = tenant_id.unwrap_or("default").to_string();

        // Check if email already exists within the same tenant
        let email_exists = conn.query_row(
            "SELECT 1 FROM users WHERE email = ?1 AND tenant_id = ?2",
            params![req.email, tenant],
            |row| row.get::<_, i32>(0),
        );

        if email_exists.is_ok() {
            anyhow::bail!("Email already registered");
        }

        // Hash password
        let salt = SaltString::generate(&mut rand::thread_rng());
        let argon2 = Argon2::default();
        let password_hash = argon2
            .hash_password(req.password.as_bytes(), &salt)
            .map_err(|e| anyhow::anyhow!("Password hashing failed: {}", e))?
            .to_string();

        // Create user
        let user = User {
            tenant_id: tenant,
            id: Uuid::new_v4().to_string(),
            email: req.email.clone(),
            password_hash,
            display_name: req
                .display_name
                .clone()
                .unwrap_or_else(|| req.email.clone()),
            role: UserRole::default(),
            created_at: Utc::now(),
        };

        conn.execute(
            "INSERT INTO users (tenant_id, id, email, password_hash, display_name, role, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                user.tenant_id,
                user.id,
                user.email,
                user.password_hash,
                user.display_name,
                user.role.to_string(),
                user.created_at.to_rfc3339(),
            ],
        )?;

        tracing::info!("User registered: {}", user.email);
        Ok(user.into())
    }

    /// Authenticate user
    pub fn authenticate(&self, req: &LoginRequest) -> Result<UserResponse> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());

        let tenant_id = req.tenant_id.as_deref().unwrap_or("default");

        let user: User = conn.query_row(
            "SELECT tenant_id, id, email, password_hash, display_name, role, created_at
             FROM users WHERE email = ?1 AND tenant_id = ?2",
            params![req.email, tenant_id],
            |row| {
                Ok(User {
                    tenant_id: row.get(0)?,
                    id: row.get(1)?,
                    email: row.get(2)?,
                    password_hash: row.get(3)?,
                    display_name: row.get(4)?,
                    role: UserRole::from(row.get::<_, String>(5)?.as_str()),
                    created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                })
            },
        )?;

        // Verify password
        let parsed_hash = PasswordHash::new(&user.password_hash)
            .map_err(|e| anyhow::anyhow!("Invalid password hash: {}", e))?;
        argon2::Argon2::default()
            .verify_password(req.password.as_bytes(), &parsed_hash)
            .map_err(|_| anyhow::anyhow!("Invalid password"))?;

        tracing::info!("User authenticated: {}", user.email);
        Ok(user.into())
    }

    /// Get user by ID
    pub fn get_user(&self, tenant_id: &str, user_id: &str) -> Result<Option<UserResponse>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let result = conn.query_row(
            "SELECT tenant_id, id, email, password_hash, display_name, role, created_at
             FROM users WHERE id = ?1 AND tenant_id = ?2",
            params![user_id, tenant_id],
            |row| {
                Ok(User {
                    tenant_id: row.get(0)?,
                    id: row.get(1)?,
                    email: row.get(2)?,
                    password_hash: row.get(3)?,
                    display_name: row.get(4)?,
                    role: UserRole::from(row.get::<_, String>(5)?.as_str()),
                    created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                })
            },
        );

        match result {
            Ok(user) => Ok(Some(user.into())),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// List users with pagination (admin only)
    pub fn list_users(&self, tenant_id: &str, page: i64, per_page: i64) -> Result<PaginatedUsersResponse> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());

        // Get total count for this tenant
        let total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM users WHERE tenant_id = ?1",
            [tenant_id],
            |row| row.get(0),
        )?;

        let offset = (page - 1) * per_page;
        let total_pages = (total as f64 / per_page as f64).ceil() as i64;

        let mut stmt = conn.prepare(
            "SELECT tenant_id, id, email, password_hash, display_name, role, created_at
             FROM users WHERE tenant_id = ?1 ORDER BY created_at DESC LIMIT ?2 OFFSET ?3",
        )?;

        let users = stmt.query_map(params![tenant_id, per_page, offset], |row| {
            Ok(User {
                tenant_id: row.get(0)?,
                id: row.get(1)?,
                email: row.get(2)?,
                password_hash: row.get(3)?,
                display_name: row.get(4)?,
                role: UserRole::from(row.get::<_, String>(5)?.as_str()),
                created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            })
        })?;

        let mut result = Vec::new();
        for user in users {
            result.push(user?.into());
        }

        Ok(PaginatedUsersResponse {
            users: result,
            total,
            page,
            per_page,
            total_pages,
        })
    }

    /// Update a user
    pub fn update_user(&self, tenant_id: &str, user_id: &str, req: &UpdateUserRequest) -> Result<Option<UserResponse>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());

        // Check if user exists in this tenant
        let exists: bool = conn.query_row(
            "SELECT 1 FROM users WHERE id = ?1 AND tenant_id = ?2",
            params![user_id, tenant_id],
            |row| row.get::<_, i32>(0),
        ).is_ok();

        if !exists {
            return Ok(None);
        }

        // Build dynamic update query - handle each field separately for simplicity
        if req.email.is_none() && req.display_name.is_none() && req.role.is_none() {
            // No fields to update, just return current user
            drop(conn);
            return self.get_user(tenant_id, user_id);
        }

        // Check and update email if provided
        if let Some(ref email) = req.email {
            let email_taken: bool = conn.query_row(
                "SELECT 1 FROM users WHERE email = ?1 AND id != ?2 AND tenant_id = ?3",
                params![email, user_id, tenant_id],
                |row| row.get::<_, i32>(0),
            ).is_ok();

            if email_taken {
                anyhow::bail!("Email already in use");
            }

            conn.execute(
                "UPDATE users SET email = ?1 WHERE id = ?2 AND tenant_id = ?3",
                params![email, user_id, tenant_id],
            )?;
        }

        // Update display_name if provided
        if let Some(ref display_name) = req.display_name {
            conn.execute(
                "UPDATE users SET display_name = ?1 WHERE id = ?2 AND tenant_id = ?3",
                params![display_name, user_id, tenant_id],
            )?;
        }

        // Update role if provided (validate role first)
        if let Some(ref role) = req.role {
            // Validate role - only accept explicitly valid roles
            let role_str = role.to_lowercase();
            if !["admin", "member", "viewer"].contains(&role_str.as_str()) {
                anyhow::bail!("Invalid role: must be admin, member, or viewer");
            }

            conn.execute(
                "UPDATE users SET role = ?1 WHERE id = ?2 AND tenant_id = ?3",
                params![role_str, user_id, tenant_id],
            )?;
        }

        tracing::info!("User updated: {}", user_id);
        drop(conn);
        self.get_user(tenant_id, user_id)
    }

    /// Delete a user
    pub fn delete_user(&self, tenant_id: &str, user_id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());

        let deleted = conn.execute(
            "DELETE FROM users WHERE id = ?1 AND tenant_id = ?2",
            params![user_id, tenant_id],
        )?;

        if deleted > 0 {
            tracing::info!("User deleted: {}", user_id);
        }

        Ok(deleted > 0)
    }
}
