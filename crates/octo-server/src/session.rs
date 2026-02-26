use dashmap::DashMap;
use octo_types::{ChatMessage, SandboxId, SessionId, UserId};

pub trait SessionStore: Send + Sync {
    fn create_session(&self) -> SessionData;
    fn get_messages(&self, session_id: &SessionId) -> Option<Vec<ChatMessage>>;
    fn push_message(&self, session_id: &SessionId, message: ChatMessage);
    fn set_messages(&self, session_id: &SessionId, messages: Vec<ChatMessage>);
}

#[derive(Debug, Clone)]
pub struct SessionData {
    pub session_id: SessionId,
    pub user_id: UserId,
    pub sandbox_id: SandboxId,
}

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

impl SessionStore for InMemorySessionStore {
    fn create_session(&self) -> SessionData {
        let data = SessionData {
            session_id: SessionId::new(),
            user_id: UserId::from_string("default"),
            sandbox_id: SandboxId::new(),
        };
        let sid = data.session_id.as_str().to_string();
        self.sessions.insert(sid.clone(), data.clone());
        self.messages.insert(sid, Vec::new());
        data
    }

    fn get_messages(&self, session_id: &SessionId) -> Option<Vec<ChatMessage>> {
        self.messages
            .get(session_id.as_str())
            .map(|v| v.clone())
    }

    fn push_message(&self, session_id: &SessionId, message: ChatMessage) {
        if let Some(mut msgs) = self.messages.get_mut(session_id.as_str()) {
            msgs.push(message);
        }
    }

    fn set_messages(&self, session_id: &SessionId, messages: Vec<ChatMessage>) {
        self.messages
            .insert(session_id.as_str().to_string(), messages);
    }
}
