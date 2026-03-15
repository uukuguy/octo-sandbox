//! Unified event system for the TUI

use crossterm::event::KeyEvent;

use super::Tab;

/// Application-level events
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// Quit the application
    Quit,
    /// Switch to next tab
    NextTab,
    /// Switch to previous tab
    PrevTab,
    /// Select a specific tab
    SelectTab(Tab),
    /// Key event forwarded to active screen
    Key(KeyEvent),
    /// Set status bar message
    SetStatus(String),
    /// Clear status bar
    ClearStatus,
    /// Tick event (for animations/updates)
    Tick,
    /// Switch to Ops view mode
    SwitchToOps,
    /// Switch to Dev view mode
    SwitchToDev,
}
