//! Unified event system for the conversation-centric TUI.

use crossterm::event::KeyEvent;
use octo_engine::agent::AgentEvent;

/// Application-level events
#[derive(Debug, Clone)]
pub enum AppEvent {
    // ── Terminal events ──
    /// Key event forwarded to active handler
    Key(KeyEvent),
    /// Terminal resize
    Resize(u16, u16),
    /// Tick event (for animations/updates)
    Tick,

    // ── Agent events ──
    /// Agent lifecycle event bridged from broadcast::Receiver<AgentEvent>
    Agent(AgentEvent),
    /// User submitted input text
    UserSubmit(String),

    // ── Application control ──
    /// Quit the application
    Quit,
}
