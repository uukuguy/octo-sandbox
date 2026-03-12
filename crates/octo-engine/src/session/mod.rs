pub mod memory;
pub mod sqlite;
pub mod thread_store;

use async_trait::async_trait;
use octo_types::{ChatMessage, SandboxId, SessionId, UserId};
use serde::{Deserialize, Serialize};

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

/// A conversation thread within a session.
/// Threads allow branching conversations (forking) from any turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    pub thread_id: String,
    pub session_id: SessionId,
    pub title: Option<String>,
    pub created_at: i64,
    pub parent_thread_id: Option<String>,
}

/// A single conversation turn (user message + assistant response).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    pub turn_id: String,
    pub thread_id: String,
    pub user_message: ChatMessage,
    pub assistant_messages: Vec<ChatMessage>,
    pub created_at: i64,
}

/// Storage trait for thread and turn operations.
/// Separate from SessionStore for backward compatibility.
#[async_trait]
pub trait ThreadStore: Send + Sync {
    // Thread operations
    async fn create_thread(
        &self,
        session_id: &SessionId,
        title: Option<&str>,
    ) -> anyhow::Result<Thread>;
    async fn list_threads(&self, session_id: &SessionId) -> anyhow::Result<Vec<Thread>>;
    async fn get_default_thread(&self, session_id: &SessionId) -> anyhow::Result<Thread>;
    async fn fork_thread(
        &self,
        thread_id: &str,
        from_turn_id: &str,
    ) -> anyhow::Result<Thread>;

    // Turn operations
    async fn push_turn(&self, thread_id: &str, turn: Turn) -> anyhow::Result<()>;
    async fn list_turns(
        &self,
        thread_id: &str,
        limit: usize,
        offset: usize,
    ) -> anyhow::Result<Vec<Turn>>;
    async fn undo_last_turn(&self, thread_id: &str) -> anyhow::Result<Option<Turn>>;
    async fn get_thread_messages(&self, thread_id: &str) -> anyhow::Result<Vec<ChatMessage>>;
}

pub use memory::InMemorySessionStore;
pub use sqlite::SqliteSessionStore;
pub use thread_store::SqliteThreadStore;
