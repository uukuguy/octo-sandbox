pub mod memory;
pub mod sqlite;

use async_trait::async_trait;
use octo_types::{ChatMessage, SandboxId, SessionId, UserId};
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct SessionData {
    pub session_id: SessionId,
    pub user_id: UserId,
    pub sandbox_id: SandboxId,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionSummary {
    pub session_id: String,
    pub created_at: i64,
    pub message_count: usize,
}

#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn create_session(&self) -> SessionData;
    async fn create_session_with_user(&self, user_id: &UserId) -> SessionData;
    async fn get_session(&self, session_id: &SessionId) -> Option<SessionData>;
    async fn get_session_for_user(
        &self,
        session_id: &SessionId,
        user_id: &UserId,
    ) -> Option<SessionData>;
    async fn get_messages(&self, session_id: &SessionId) -> Option<Vec<ChatMessage>>;
    async fn push_message(&self, session_id: &SessionId, message: ChatMessage);
    async fn set_messages(&self, session_id: &SessionId, messages: Vec<ChatMessage>);
    async fn list_sessions(&self, limit: usize, offset: usize) -> Vec<SessionSummary>;
    async fn list_sessions_for_user(
        &self,
        user_id: &UserId,
        limit: usize,
        offset: usize,
    ) -> Vec<SessionSummary>;

    /// Delete a session and all its messages
    async fn delete_session(&self, session_id: &SessionId) -> bool;

    /// Get the most recent session (for --continue functionality)
    async fn most_recent_session(&self) -> Option<SessionData>;

    /// Get the most recent session for a specific user
    async fn most_recent_session_for_user(&self, user_id: &UserId) -> Option<SessionData>;
}

pub use memory::InMemorySessionStore;
pub use sqlite::SqliteSessionStore;
