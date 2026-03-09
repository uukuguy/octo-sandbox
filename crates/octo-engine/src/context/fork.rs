use octo_types::ChatMessage;

/// A forked context for isolated skill execution.
/// When a skill has `context_fork = true`, it runs with a copy of the
/// current conversation history, and any messages generated during execution
/// do NOT flow back into the main conversation.
#[derive(Debug, Clone)]
pub struct ContextFork {
    /// The forked message history (snapshot at fork time)
    messages: Vec<ChatMessage>,
    /// System prompt to use in the forked context
    system_prompt: Option<String>,
    /// Maximum messages to include from parent (None = all)
    max_parent_messages: Option<usize>,
}

impl ContextFork {
    /// Create a new context fork from a parent conversation history.
    pub fn from_parent(
        parent_messages: &[ChatMessage],
        system_prompt: Option<String>,
        max_parent_messages: Option<usize>,
    ) -> Self {
        let messages = if let Some(max) = max_parent_messages {
            let start = parent_messages.len().saturating_sub(max);
            parent_messages[start..].to_vec()
        } else {
            parent_messages.to_vec()
        };

        Self {
            messages,
            system_prompt,
            max_parent_messages,
        }
    }

    /// Create an empty fork (no parent context).
    pub fn empty() -> Self {
        Self {
            messages: Vec::new(),
            system_prompt: None,
            max_parent_messages: None,
        }
    }

    /// Get the forked messages.
    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    /// Get a mutable reference to the forked messages.
    pub fn messages_mut(&mut self) -> &mut Vec<ChatMessage> {
        &mut self.messages
    }

    /// Get system prompt.
    pub fn system_prompt(&self) -> Option<&str> {
        self.system_prompt.as_deref()
    }

    /// Get the max parent messages setting used when this fork was created.
    pub fn max_parent_messages(&self) -> Option<usize> {
        self.max_parent_messages
    }

    /// Add a message to the forked context.
    pub fn push_message(&mut self, msg: ChatMessage) {
        self.messages.push(msg);
    }

    /// Get the number of messages.
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Extract only the messages added after the fork point.
    /// Returns messages that were added via push_message (after initial snapshot).
    pub fn new_messages(&self, original_count: usize) -> &[ChatMessage] {
        if self.messages.len() > original_count {
            &self.messages[original_count..]
        } else {
            &[]
        }
    }
}
