//! Lifecycle state machine: start / stop / pause / resume

use super::{AgentId, AgentRegistry, AgentRuntimeHandle, AgentStatus};
use crate::agent::CancellationToken;

#[derive(Debug)]
pub enum AgentError {
    NotFound(AgentId),
    InvalidTransition { from: AgentStatus, action: &'static str },
}

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "agent not found: {id}"),
            Self::InvalidTransition { from, action } => {
                write!(f, "cannot {action} agent in state {from}")
            }
        }
    }
}

impl std::error::Error for AgentError {}

impl AgentRegistry {
    /// Mark agent as Running. Actual AgentLoop spawned by AgentRunner.
    /// State check and write happen in a single get_mut to avoid TOCTOU races.
    pub fn mark_running(
        &self,
        id: &AgentId,
        cancel_token: CancellationToken,
    ) -> Result<(), AgentError> {
        let mut slot = self
            .by_id
            .get_mut(id)
            .ok_or_else(|| AgentError::NotFound(id.clone()))?;
        match slot.value().0.state {
            AgentStatus::Created | AgentStatus::Paused => {
                slot.value_mut().1 = Some(AgentRuntimeHandle { cancel_token });
                slot.value_mut().0.state = AgentStatus::Running;
                Ok(())
            }
            ref other => Err(AgentError::InvalidTransition {
                from: other.clone(),
                action: "start",
            }),
        }
    }

    /// Mark agent as Stopped. State check, handle cancellation, and write happen
    /// in a single get_mut to avoid TOCTOU races.
    pub fn mark_stopped(&self, id: &AgentId) -> Result<(), AgentError> {
        let mut slot = self
            .by_id
            .get_mut(id)
            .ok_or_else(|| AgentError::NotFound(id.clone()))?;
        if slot.value().0.state == AgentStatus::Stopped {
            return Err(AgentError::InvalidTransition {
                from: AgentStatus::Stopped,
                action: "stop",
            });
        }
        if let Some(h) = slot.value_mut().1.take() {
            h.cancel_token.cancel();
        }
        slot.value_mut().0.state = AgentStatus::Stopped;
        Ok(())
    }

    /// Mark agent as Paused. State check, handle cancellation, and write happen
    /// in a single get_mut to avoid TOCTOU races.
    pub fn mark_paused(&self, id: &AgentId) -> Result<(), AgentError> {
        let mut slot = self
            .by_id
            .get_mut(id)
            .ok_or_else(|| AgentError::NotFound(id.clone()))?;
        if slot.value().0.state != AgentStatus::Running {
            return Err(AgentError::InvalidTransition {
                from: slot.value().0.state.clone(),
                action: "pause",
            });
        }
        if let Some(h) = slot.value_mut().1.take() {
            h.cancel_token.cancel();
        }
        slot.value_mut().0.state = AgentStatus::Paused;
        Ok(())
    }

    /// Mark agent as Resumed (back to Running). State check and write happen
    /// in a single get_mut to avoid TOCTOU races.
    pub fn mark_resumed(
        &self,
        id: &AgentId,
        cancel_token: CancellationToken,
    ) -> Result<(), AgentError> {
        let mut slot = self
            .by_id
            .get_mut(id)
            .ok_or_else(|| AgentError::NotFound(id.clone()))?;
        if slot.value().0.state != AgentStatus::Paused {
            return Err(AgentError::InvalidTransition {
                from: slot.value().0.state.clone(),
                action: "resume",
            });
        }
        slot.value_mut().1 = Some(AgentRuntimeHandle { cancel_token });
        slot.value_mut().0.state = AgentStatus::Running;
        Ok(())
    }
}
