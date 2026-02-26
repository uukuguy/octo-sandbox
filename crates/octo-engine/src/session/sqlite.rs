use anyhow::Result;
use async_trait::async_trait;
use dashmap::DashMap;
use octo_types::{ChatMessage, SandboxId, SessionId, UserId};
use tracing::debug;

use super::{SessionData, SessionStore, SessionSummary};

/// SQLite-backed SessionStore with DashMap hot-cache and write-through.
pub struct SqliteSessionStore {
    conn: tokio_rusqlite::Connection,
    sessions: DashMap<String, SessionData>,
    messages: DashMap<String, Vec<ChatMessage>>,
}

impl SqliteSessionStore {
    pub async fn new(conn: tokio_rusqlite::Connection) -> Result<Self> {
        Ok(Self {
            conn,
            sessions: DashMap::new(),
            messages: DashMap::new(),
        })
    }
}

#[async_trait]
impl SessionStore for SqliteSessionStore {
    async fn create_session(&self) -> SessionData {
        let data = SessionData {
            session_id: SessionId::new(),
            user_id: UserId::from_string("default"),
            sandbox_id: SandboxId::new(),
        };
        let sid = data.session_id.as_str().to_string();
        self.sessions.insert(sid.clone(), data.clone());
        self.messages.insert(sid.clone(), Vec::new());

        // Write-through to DB (best-effort)
        let uid = data.user_id.as_str().to_string();
        let sbid = data.sandbox_id.as_str().to_string();
        let sid_db = sid;
        let _ = self
            .conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR IGNORE INTO sessions (session_id, user_id, sandbox_id) VALUES (?1, ?2, ?3)",
                    rusqlite::params![sid_db, uid, sbid],
                )?;
                Ok(())
            })
            .await;

        debug!(session_id = %data.session_id, "Created session");
        data
    }

    async fn get_session(&self, session_id: &SessionId) -> Option<SessionData> {
        // Check cache
        if let Some(data) = self.sessions.get(session_id.as_str()) {
            return Some(data.clone());
        }

        // Miss: load from DB
        let sid = session_id.as_str().to_string();
        let result = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT session_id, user_id, sandbox_id FROM sessions WHERE session_id = ?1",
                )?;
                let data = stmt
                    .query_row(rusqlite::params![sid], |row| {
                        let session_id: String = row.get(0)?;
                        let user_id: String = row.get(1)?;
                        let sandbox_id: String = row.get(2)?;
                        Ok(SessionData {
                            session_id: SessionId::from_string(&session_id),
                            user_id: UserId::from_string(&user_id),
                            sandbox_id: SandboxId::from_string(&sandbox_id),
                        })
                    })
                    .ok();
                Ok(data)
            })
            .await;

        if let Ok(Some(data)) = result {
            self.sessions
                .insert(data.session_id.as_str().to_string(), data.clone());
            Some(data)
        } else {
            None
        }
    }

    async fn get_messages(&self, session_id: &SessionId) -> Option<Vec<ChatMessage>> {
        // Check cache
        if let Some(msgs) = self.messages.get(session_id.as_str()) {
            return Some(msgs.clone());
        }

        // Miss: load from DB
        let sid = session_id.as_str().to_string();
        let result = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT content_json FROM session_messages WHERE session_id = ?1 ORDER BY id ASC",
                )?;
                let rows = stmt.query_map(rusqlite::params![sid], |row| {
                    let json: String = row.get(0)?;
                    Ok(json)
                })?;
                let mut messages: Vec<ChatMessage> = Vec::new();
                for row in rows {
                    let json = row?;
                    if let Ok(msg) = serde_json::from_str::<ChatMessage>(&json) {
                        messages.push(msg);
                    }
                }
                Ok(messages)
            })
            .await;

        match result {
            Ok(msgs) if !msgs.is_empty() => {
                self.messages
                    .insert(session_id.as_str().to_string(), msgs.clone());
                Some(msgs)
            }
            Ok(_) => {
                // Check if session exists
                if self.sessions.contains_key(session_id.as_str()) {
                    Some(Vec::new())
                } else {
                    None
                }
            }
            Err(_) => None,
        }
    }

    async fn push_message(&self, session_id: &SessionId, message: ChatMessage) {
        // Update cache
        if let Some(mut msgs) = self.messages.get_mut(session_id.as_str()) {
            msgs.push(message.clone());
        }

        // Write-through to DB
        let sid = session_id.as_str().to_string();
        let role = match message.role {
            octo_types::MessageRole::User => "user",
            octo_types::MessageRole::Assistant => "assistant",
            octo_types::MessageRole::System => "system",
        };
        let content_json = serde_json::to_string(&message).unwrap_or_default();
        let role_str = role.to_string();

        let _ = self
            .conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO session_messages (session_id, role, content_json) VALUES (?1, ?2, ?3)",
                    rusqlite::params![sid, role_str, content_json],
                )?;
                Ok(())
            })
            .await;
    }

    async fn set_messages(&self, session_id: &SessionId, messages: Vec<ChatMessage>) {
        // Update cache
        self.messages
            .insert(session_id.as_str().to_string(), messages.clone());

        // Write-through: DELETE old + batch INSERT new
        let sid = session_id.as_str().to_string();
        let msgs = messages;

        let _ = self
            .conn
            .call(move |conn| {
                let tx = conn.transaction()?;
                tx.execute(
                    "DELETE FROM session_messages WHERE session_id = ?1",
                    rusqlite::params![sid],
                )?;
                for msg in &msgs {
                    let role = match msg.role {
                        octo_types::MessageRole::User => "user",
                        octo_types::MessageRole::Assistant => "assistant",
                        octo_types::MessageRole::System => "system",
                    };
                    let content_json = serde_json::to_string(msg).unwrap_or_default();
                    tx.execute(
                        "INSERT INTO session_messages (session_id, role, content_json) VALUES (?1, ?2, ?3)",
                        rusqlite::params![sid, role, content_json],
                    )?;
                }
                tx.commit()?;
                Ok(())
            })
            .await;
    }

    async fn list_sessions(&self, limit: usize, offset: usize) -> Vec<SessionSummary> {
        let result = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT s.session_id, s.created_at,
                            (SELECT COUNT(*) FROM session_messages m WHERE m.session_id = s.session_id) AS msg_count
                     FROM sessions s
                     ORDER BY s.created_at DESC
                     LIMIT ?1 OFFSET ?2",
                )?;
                let rows = stmt
                    .query_map(rusqlite::params![limit as i64, offset as i64], |row| {
                        Ok(SessionSummary {
                            session_id: row.get(0)?,
                            created_at: row.get(1)?,
                            message_count: row.get::<_, i64>(2)? as usize,
                        })
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await;

        result.unwrap_or_default()
    }
}
