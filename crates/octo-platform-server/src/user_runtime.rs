//! User runtime management - per-user AgentRuntime lifecycle

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::UserRuntimeConfig;

/// User runtime - one per user, manages sessions
pub struct UserRuntime {
    pub user_id: String,
    pub config: Arc<UserRuntimeConfig>,
    pub sessions: DashMap<String, Session>,
    pub db_path: PathBuf,
    session_creation_lock: Mutex<()>, // Protects session creation to prevent TOCTOU race
}

impl std::fmt::Debug for UserRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserRuntime")
            .field("user_id", &self.user_id)
            .field("config", &self.config)
            .field("sessions", &self.sessions)
            .field("db_path", &self.db_path)
            .finish()
    }
}

/// Session - one per conversation
#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub user_id: String,
    pub name: Option<String>,
    pub status: SessionStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum SessionStatus {
    #[default]
    Active,
    Paused,
    Completed,
}

impl Session {
    pub fn new(user_id: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            user_id,
            name: None,
            status: SessionStatus::Active,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }
}

impl UserRuntime {
    pub fn new(user_id: String, config: Arc<UserRuntimeConfig>) -> Result<Self> {
        let db_path = PathBuf::from(config.db_path_template.replace("{user_id}", &user_id));

        // Ensure directory exists
        std::fs::create_dir_all(&db_path).context("create user data directory")?;

        tracing::info!("UserRuntime created for user: {} at {:?}", user_id, db_path);

        Ok(Self {
            user_id,
            config,
            sessions: DashMap::new(),
            db_path,
            session_creation_lock: Mutex::new(()),
        })
    }

    pub fn create_session(&self, name: Option<String>) -> Result<Session> {
        // Use lock to prevent TOCTOU race condition between check and insert
        // Handle poisoned mutex gracefully instead of panicking
        let _guard = self.session_creation_lock.lock().map_err(|_| {
            anyhow::anyhow!("Session creation is temporarily unavailable due to a concurrent error")
        })?;

        // Check concurrent limit while holding the lock
        let current = self.sessions.len() as u32;
        if current >= self.config.max_concurrent_agents {
            anyhow::bail!(
                "Concurrent session limit exceeded: max {}, current {}",
                self.config.max_concurrent_agents,
                current
            );
        }

        // Create session (still inside lock to ensure atomicity)
        let session = match name {
            Some(n) => Session::new(self.user_id.clone()).with_name(n),
            None => Session::new(self.user_id.clone()),
        };

        self.sessions.insert(session.id.clone(), session.clone());
        Ok(session)
    }

    pub fn get_session(&self, user_id: &str, session_id: &str) -> Option<Session> {
        self.sessions
            .get(session_id)
            .filter(|s| s.user_id == user_id)
            .map(|s| s.clone())
    }

    pub fn list_sessions(&self, user_id: &str) -> Vec<Session> {
        self.sessions
            .iter()
            .filter(|s| s.user_id == user_id)
            .map(|s| s.clone())
            .collect()
    }

    pub fn delete_session(&self, user_id: &str, session_id: &str) -> bool {
        // Get session and clone ownership info BEFORE removing (releases read lock)
        let is_owned = self
            .sessions
            .get(session_id)
            .map(|s| s.user_id == user_id)
            .unwrap_or(false);

        // Only attempt removal if the caller owns this session
        if is_owned {
            self.sessions.remove(session_id).is_some()
        } else {
            false
        }
    }
}
