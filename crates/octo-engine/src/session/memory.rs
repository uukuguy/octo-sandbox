use async_trait::async_trait;
use dashmap::DashMap;
use octo_types::{ChatMessage, SandboxId, SessionId, UserId};

use super::{SessionData, SessionStore, SessionSummary};

pub struct InMemorySessionStore {
    sessions: DashMap<String, SessionData>,
    messages: DashMap<String, Vec<ChatMessage>>,
}

impl InMemorySessionStore {
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
            messages: DashMap::new(),
        }
    }
}

impl Default for InMemorySessionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SessionStore for InMemorySessionStore {
    async fn create_session(&self) -> SessionData {
        self.create_session_with_user(&UserId::from_string("default"))
            .await
    }

    async fn create_session_with_user(&self, user_id: &UserId) -> SessionData {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let data = SessionData {
            session_id: SessionId::new(),
            user_id: user_id.clone(),
            sandbox_id: SandboxId::new(),
            created_at: now,
        };
        let sid = data.session_id.as_str().to_string();
        self.sessions.insert(sid.clone(), data.clone());
        self.messages.insert(sid, Vec::new());
        data
    }

    async fn get_session(&self, session_id: &SessionId) -> Option<SessionData> {
        self.sessions.get(session_id.as_str()).map(|v| v.clone())
    }

    async fn get_session_for_user(
        &self,
        session_id: &SessionId,
        user_id: &UserId,
    ) -> Option<SessionData> {
        self.sessions
            .get(session_id.as_str())
            .filter(|v| v.user_id.as_str() == user_id.as_str())
            .map(|v| v.clone())
    }

    async fn get_messages(&self, session_id: &SessionId) -> Option<Vec<ChatMessage>> {
        self.messages.get(session_id.as_str()).map(|v| v.clone())
    }

    async fn push_message(&self, session_id: &SessionId, message: ChatMessage) {
        if let Some(mut msgs) = self.messages.get_mut(session_id.as_str()) {
            msgs.push(message);
        }
    }

    async fn set_messages(&self, session_id: &SessionId, messages: Vec<ChatMessage>) {
        self.messages
            .insert(session_id.as_str().to_string(), messages);
    }

    async fn list_sessions(&self, limit: usize, offset: usize) -> Vec<SessionSummary> {
        let mut summaries: Vec<SessionSummary> = self
            .sessions
            .iter()
            .map(|entry| {
                let sid = entry.key().clone();
                let msg_count = self.messages.get(&sid).map(|m| m.len()).unwrap_or(0);
                SessionSummary {
                    session_id: sid,
                    created_at: 0, // InMemory does not track created_at
                    message_count: msg_count,
                }
            })
            .collect();
        summaries.sort_by(|a, b| b.session_id.cmp(&a.session_id));
        summaries.into_iter().skip(offset).take(limit).collect()
    }

    async fn list_sessions_for_user(
        &self,
        user_id: &UserId,
        limit: usize,
        offset: usize,
    ) -> Vec<SessionSummary> {
        let mut summaries: Vec<SessionSummary> = self
            .sessions
            .iter()
            .filter(|entry| entry.user_id.as_str() == user_id.as_str())
            .map(|entry| {
                let sid = entry.key().clone();
                let msg_count = self.messages.get(&sid).map(|m| m.len()).unwrap_or(0);
                SessionSummary {
                    session_id: sid,
                    created_at: 0,
                    message_count: msg_count,
                }
            })
            .collect();
        summaries.sort_by(|a, b| b.session_id.cmp(&a.session_id));
        summaries.into_iter().skip(offset).take(limit).collect()
    }

    async fn delete_session(&self, session_id: &SessionId) -> bool {
        let sid = session_id.as_str().to_string();
        self.messages.remove(&sid);
        self.sessions.remove(&sid).is_some()
    }

    async fn most_recent_session(&self) -> Option<SessionData> {
        self.sessions
            .iter()
            .max_by_key(|entry| entry.value().created_at)
            .map(|entry| entry.value().clone())
    }

    async fn most_recent_session_for_user(&self, user_id: &UserId) -> Option<SessionData> {
        self.sessions
            .iter()
            .filter(|entry| entry.value().user_id.as_str() == user_id.as_str())
            .max_by_key(|entry| entry.value().created_at)
            .map(|entry| entry.value().clone())
    }
}
