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
}

/// User response (without password hash)
#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub role: UserRole,
    pub created_at: DateTime<Utc>,
}

impl From<User> for UserResponse {
    fn from(user: User) -> Self {
        Self {
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
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "CREATE TABLE IF NOT EXISTS users (
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

        tracing::info!("User database schema initialized");
        Ok(())
    }

    /// Register a new user
    pub fn register(&self, req: &RegisterRequest) -> Result<UserResponse> {
        let conn = self.conn.lock().unwrap();

        // Check if email already exists
        let email_exists = conn.query_row(
            "SELECT 1 FROM users WHERE email = ?1",
            [&req.email],
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
            "INSERT INTO users (id, email, password_hash, display_name, role, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
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
        let conn = self.conn.lock().unwrap();
        let user: User = conn.query_row(
            "SELECT id, email, password_hash, display_name, role, created_at
             FROM users WHERE email = ?1",
            [&req.email],
            |row| {
                Ok(User {
                    id: row.get(0)?,
                    email: row.get(1)?,
                    password_hash: row.get(2)?,
                    display_name: row.get(3)?,
                    role: UserRole::from(row.get::<_, String>(4)?.as_str()),
                    created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
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
    pub fn get_user(&self, user_id: &str) -> Result<Option<UserResponse>> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            "SELECT id, email, password_hash, display_name, role, created_at
             FROM users WHERE id = ?1",
            [user_id],
            |row| {
                Ok(User {
                    id: row.get(0)?,
                    email: row.get(1)?,
                    password_hash: row.get(2)?,
                    display_name: row.get(3)?,
                    role: UserRole::from(row.get::<_, String>(4)?.as_str()),
                    created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
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

    /// List all users (admin only)
    pub fn list_users(&self) -> Result<Vec<UserResponse>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, email, password_hash, display_name, role, created_at FROM users ORDER BY created_at DESC",
        )?;

        let users = stmt.query_map([], |row| {
            Ok(User {
                id: row.get(0)?,
                email: row.get(1)?,
                password_hash: row.get(2)?,
                display_name: row.get(3)?,
                role: UserRole::from(row.get::<_, String>(4)?.as_str()),
                created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            })
        })?;

        let mut result = Vec::new();
        for user in users {
            result.push(user?.into());
        }
        Ok(result)
    }
}
