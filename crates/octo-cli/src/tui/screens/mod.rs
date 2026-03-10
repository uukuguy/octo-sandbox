//! Screen management and routing

pub mod agents;
pub mod chat;
pub mod dashboard;
pub mod dual_chat;
pub mod logs;
pub mod mcp;
pub mod memory;
pub mod security;
pub mod sessions;
pub mod settings;
pub mod skills;
pub mod tools;
pub mod welcome;

use ratatui::prelude::*;

use crate::commands::AppState;
use crate::tui::event::AppEvent;
use crate::tui::theme::TuiTheme;
use crate::tui::Tab;

/// Trait for TUI screens
pub trait Screen {
    /// Render the screen content
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &TuiTheme, state: &AppState);

    /// Handle an event
    fn handle_event(&mut self, _event: &AppEvent) {}

    /// Screen title for the tab bar
    fn title(&self) -> &str;
}

/// Manages screen instances
pub struct ScreenManager {
    pub welcome: welcome::WelcomeScreen,
    pub dashboard: dashboard::DashboardScreen,
    pub chat: chat::ChatScreen,
    pub agents: agents::AgentsScreen,
    pub sessions: sessions::SessionsScreen,
    pub memory: memory::MemoryScreen,
    pub skills: skills::SkillsScreen,
    pub mcp: mcp::McpScreen,
    pub tools: tools::ToolsScreen,
    pub security: security::SecurityScreen,
    pub settings: settings::SettingsScreen,
    pub logs: logs::LogsScreen,
}

impl ScreenManager {
    pub fn new() -> Self {
        Self {
            welcome: welcome::WelcomeScreen::new(),
            dashboard: dashboard::DashboardScreen::new(),
            chat: chat::ChatScreen::new(),
            agents: agents::AgentsScreen::new(),
            sessions: sessions::SessionsScreen::new(),
            memory: memory::MemoryScreen::new(),
            skills: skills::SkillsScreen::new(),
            mcp: mcp::McpScreen::new(),
            tools: tools::ToolsScreen::new(),
            security: security::SecurityScreen::new(),
            settings: settings::SettingsScreen::new(),
            logs: logs::LogsScreen::new(),
        }
    }

    pub fn render(
        &mut self,
        tab: &Tab,
        frame: &mut Frame,
        area: Rect,
        theme: &TuiTheme,
        state: &AppState,
    ) {
        match tab {
            Tab::Welcome => self.welcome.render(frame, area, theme, state),
            Tab::Dashboard => self.dashboard.render(frame, area, theme, state),
            Tab::Chat => self.chat.render(frame, area, theme, state),
            Tab::Agents => self.agents.render(frame, area, theme, state),
            Tab::Sessions => self.sessions.render(frame, area, theme, state),
            Tab::Memory => self.memory.render(frame, area, theme, state),
            Tab::Skills => self.skills.render(frame, area, theme, state),
            Tab::Mcp => self.mcp.render(frame, area, theme, state),
            Tab::Tools => self.tools.render(frame, area, theme, state),
            Tab::Security => self.security.render(frame, area, theme, state),
            Tab::Settings => self.settings.render(frame, area, theme, state),
            Tab::Logs => self.logs.render(frame, area, theme, state),
        }
    }

    pub fn handle_event(&mut self, tab: &Tab, event: &AppEvent) {
        match tab {
            Tab::Welcome => self.welcome.handle_event(event),
            Tab::Dashboard => self.dashboard.handle_event(event),
            Tab::Chat => self.chat.handle_event(event),
            Tab::Agents => self.agents.handle_event(event),
            Tab::Sessions => self.sessions.handle_event(event),
            Tab::Memory => self.memory.handle_event(event),
            Tab::Skills => self.skills.handle_event(event),
            Tab::Mcp => self.mcp.handle_event(event),
            Tab::Tools => self.tools.handle_event(event),
            Tab::Security => self.security.handle_event(event),
            Tab::Settings => self.settings.handle_event(event),
            Tab::Logs => self.logs.handle_event(event),
        }
    }
}
