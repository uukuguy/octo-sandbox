//! Dual Chat screen — split-panel view for Plan and Build agents
//!
//! Provides a side-by-side view showing conversations with both the Plan
//! agent (left panel) and Build agent (right panel). Users can toggle
//! focus between panels with Tab and scroll each independently.

use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::widgets::{Paragraph, Wrap};

use crate::commands::AppState;
use crate::tui::event::AppEvent;
use crate::tui::theme::TuiTheme;

use super::Screen;

/// Which panel is focused in dual view
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DualPanel {
    Plan,
    Build,
}

impl Default for DualPanel {
    fn default() -> Self {
        Self::Build
    }
}

/// State for the dual chat screen
#[derive(Debug)]
pub struct DualChatScreen {
    /// Currently focused panel
    pub focused: DualPanel,
    /// Plan agent messages (simplified as strings for now)
    pub plan_messages: Vec<String>,
    /// Build agent messages
    pub build_messages: Vec<String>,
    /// Plan agent status line
    pub plan_status: String,
    /// Build agent status line
    pub build_status: String,
    /// Scroll offset for plan panel
    pub plan_scroll: u16,
    /// Scroll offset for build panel
    pub build_scroll: u16,
}

impl Default for DualChatScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl DualChatScreen {
    pub fn new() -> Self {
        Self {
            focused: DualPanel::default(),
            plan_messages: Vec::new(),
            build_messages: Vec::new(),
            plan_status: "idle".to_string(),
            build_status: "idle".to_string(),
            plan_scroll: 0,
            build_scroll: 0,
        }
    }

    /// Toggle focus between panels
    pub fn toggle_focus(&mut self) {
        self.focused = match self.focused {
            DualPanel::Plan => DualPanel::Build,
            DualPanel::Build => DualPanel::Plan,
        };
    }

    /// Set focus to a specific panel
    pub fn set_focus(&mut self, panel: DualPanel) {
        self.focused = panel;
    }

    /// Add a message to the plan panel
    pub fn push_plan_message(&mut self, msg: String) {
        self.plan_messages.push(msg);
    }

    /// Add a message to the build panel
    pub fn push_build_message(&mut self, msg: String) {
        self.build_messages.push(msg);
    }

    /// Scroll the focused panel up
    pub fn scroll_up(&mut self) {
        match self.focused {
            DualPanel::Plan => self.plan_scroll = self.plan_scroll.saturating_sub(1),
            DualPanel::Build => self.build_scroll = self.build_scroll.saturating_sub(1),
        }
    }

    /// Scroll the focused panel down
    pub fn scroll_down(&mut self) {
        match self.focused {
            DualPanel::Plan => self.plan_scroll = self.plan_scroll.saturating_add(1),
            DualPanel::Build => self.build_scroll = self.build_scroll.saturating_add(1),
        }
    }

    fn handle_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Tab => self.toggle_focus(),
            KeyCode::Up => self.scroll_up(),
            KeyCode::Down => self.scroll_down(),
            KeyCode::Char('h') | KeyCode::Left => self.set_focus(DualPanel::Plan),
            KeyCode::Char('l') | KeyCode::Right => self.set_focus(DualPanel::Build),
            _ => {}
        }
    }

    /// Render a single agent panel (plan or build)
    fn render_panel(
        &self,
        frame: &mut Frame,
        area: Rect,
        title: &str,
        messages: &[String],
        status: &str,
        _scroll: u16,
        focused: bool,
        theme: &TuiTheme,
    ) {
        // Split panel into content area + status bar
        let panel_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(1)])
            .split(area);

        // Use theme helpers for consistent styling
        let active_title = format!(" {} ", title);
        let inactive_title = format!(" {} ", title);
        let block = if focused {
            theme.styled_block_active(&active_title)
        } else {
            theme.styled_block(&inactive_title)
        };

        // Build message lines
        let lines: Vec<Line> = if messages.is_empty() {
            vec![Line::from(Span::styled(
                "No messages yet.",
                theme.text_dim(),
            ))]
        } else {
            messages
                .iter()
                .map(|m| Line::from(Span::styled(m.as_str(), theme.text_normal())))
                .collect()
        };

        let paragraph = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, panel_chunks[0]);

        // Status bar
        let focus_marker = if focused { ">" } else { " " };
        let status_style = if focused {
            Style::default().fg(theme.accent)
        } else {
            theme.text_dim()
        };

        let status_line = Line::from(vec![
            Span::styled(format!("{} {} ", focus_marker, status), status_style),
            Span::styled(
                format!("| {} msgs", messages.len()),
                theme.text_dim(),
            ),
        ]);

        frame.render_widget(Paragraph::new(status_line), panel_chunks[1]);
    }
}

impl Screen for DualChatScreen {
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &TuiTheme, _state: &AppState) {
        // Split horizontally into two equal panels
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);

        // Clone values to avoid borrow issues
        let focused = self.focused;
        let plan_scroll = self.plan_scroll;
        let build_scroll = self.build_scroll;
        let plan_status = self.plan_status.clone();
        let build_status = self.build_status.clone();

        self.render_panel(
            frame,
            chunks[0],
            "Plan Agent",
            &self.plan_messages.clone(),
            &plan_status,
            plan_scroll,
            focused == DualPanel::Plan,
            theme,
        );

        self.render_panel(
            frame,
            chunks[1],
            "Build Agent",
            &self.build_messages.clone(),
            &build_status,
            build_scroll,
            focused == DualPanel::Build,
            theme,
        );
    }

    fn handle_event(&mut self, event: &AppEvent) {
        if let AppEvent::Key(key) = event {
            self.handle_key(key.code);
        }
    }

    fn title(&self) -> &str {
        "Dual Chat"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dual_panel_default() {
        let panel = DualPanel::default();
        assert_eq!(panel, DualPanel::Build);
    }

    #[test]
    fn test_toggle_focus() {
        let mut screen = DualChatScreen::new();
        assert_eq!(screen.focused, DualPanel::Build);

        screen.toggle_focus();
        assert_eq!(screen.focused, DualPanel::Plan);

        screen.toggle_focus();
        assert_eq!(screen.focused, DualPanel::Build);
    }

    #[test]
    fn test_set_focus() {
        let mut screen = DualChatScreen::new();

        screen.set_focus(DualPanel::Plan);
        assert_eq!(screen.focused, DualPanel::Plan);

        screen.set_focus(DualPanel::Build);
        assert_eq!(screen.focused, DualPanel::Build);
    }

    #[test]
    fn test_push_messages() {
        let mut screen = DualChatScreen::new();
        assert!(screen.plan_messages.is_empty());
        assert!(screen.build_messages.is_empty());

        screen.push_plan_message("plan step 1".to_string());
        screen.push_plan_message("plan step 2".to_string());
        assert_eq!(screen.plan_messages.len(), 2);
        assert_eq!(screen.plan_messages[0], "plan step 1");
        assert_eq!(screen.plan_messages[1], "plan step 2");

        screen.push_build_message("build output 1".to_string());
        assert_eq!(screen.build_messages.len(), 1);
        assert_eq!(screen.build_messages[0], "build output 1");
    }

    #[test]
    fn test_scroll_up_down() {
        let mut screen = DualChatScreen::new();
        // Default focus is Build
        assert_eq!(screen.build_scroll, 0);

        screen.scroll_down();
        assert_eq!(screen.build_scroll, 1);

        screen.scroll_down();
        assert_eq!(screen.build_scroll, 2);

        screen.scroll_up();
        assert_eq!(screen.build_scroll, 1);

        // Switch to Plan and scroll
        screen.set_focus(DualPanel::Plan);
        assert_eq!(screen.plan_scroll, 0);

        screen.scroll_down();
        screen.scroll_down();
        screen.scroll_down();
        assert_eq!(screen.plan_scroll, 3);

        screen.scroll_up();
        assert_eq!(screen.plan_scroll, 2);
    }

    #[test]
    fn test_scroll_saturating() {
        let mut screen = DualChatScreen::new();
        // Build panel at 0 — scroll up should stay at 0
        screen.scroll_up();
        assert_eq!(screen.build_scroll, 0);

        // Plan panel at 0 — scroll up should stay at 0
        screen.set_focus(DualPanel::Plan);
        screen.scroll_up();
        assert_eq!(screen.plan_scroll, 0);
    }

    #[test]
    fn test_handle_key_tab_toggles() {
        let mut screen = DualChatScreen::new();
        assert_eq!(screen.focused, DualPanel::Build);

        screen.handle_key(KeyCode::Tab);
        assert_eq!(screen.focused, DualPanel::Plan);
    }

    #[test]
    fn test_handle_key_arrows_scroll() {
        let mut screen = DualChatScreen::new();
        screen.handle_key(KeyCode::Down);
        assert_eq!(screen.build_scroll, 1);

        screen.handle_key(KeyCode::Up);
        assert_eq!(screen.build_scroll, 0);
    }

    #[test]
    fn test_handle_key_left_right_focus() {
        let mut screen = DualChatScreen::new();

        screen.handle_key(KeyCode::Left);
        assert_eq!(screen.focused, DualPanel::Plan);

        screen.handle_key(KeyCode::Right);
        assert_eq!(screen.focused, DualPanel::Build);
    }

    #[test]
    fn test_handle_key_h_l_focus() {
        let mut screen = DualChatScreen::new();

        screen.handle_key(KeyCode::Char('h'));
        assert_eq!(screen.focused, DualPanel::Plan);

        screen.handle_key(KeyCode::Char('l'));
        assert_eq!(screen.focused, DualPanel::Build);
    }

    #[test]
    fn test_title() {
        let screen = DualChatScreen::new();
        assert_eq!(screen.title(), "Dual Chat");
    }

    #[test]
    fn test_new_initial_state() {
        let screen = DualChatScreen::new();
        assert_eq!(screen.focused, DualPanel::Build);
        assert!(screen.plan_messages.is_empty());
        assert!(screen.build_messages.is_empty());
        assert_eq!(screen.plan_status, "idle");
        assert_eq!(screen.build_status, "idle");
        assert_eq!(screen.plan_scroll, 0);
        assert_eq!(screen.build_scroll, 0);
    }

    #[test]
    fn test_default_impl() {
        let screen = DualChatScreen::default();
        assert_eq!(screen.focused, DualPanel::Build);
        assert_eq!(screen.plan_status, "idle");
    }

    #[test]
    fn test_handle_event_key() {
        let mut screen = DualChatScreen::new();
        let key = crossterm::event::KeyEvent::new(
            KeyCode::Tab,
            crossterm::event::KeyModifiers::NONE,
        );
        screen.handle_event(&AppEvent::Key(key));
        assert_eq!(screen.focused, DualPanel::Plan);
    }

    #[test]
    fn test_handle_event_non_key_ignored() {
        let mut screen = DualChatScreen::new();
        screen.handle_event(&AppEvent::Tick);
        // State unchanged
        assert_eq!(screen.focused, DualPanel::Build);
        assert!(screen.plan_messages.is_empty());
    }

    #[test]
    fn test_render_panel_smoke() {
        use ratatui::{backend::TestBackend, Terminal};

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let screen = DualChatScreen::new();
        let theme = TuiTheme::default();

        terminal
            .draw(|f| {
                let area = f.area();
                screen.render_panel(
                    f,
                    area,
                    "Plan Agent",
                    &[],
                    "idle",
                    0,
                    true,
                    &theme,
                );
            })
            .unwrap();
    }

    #[test]
    fn test_render_panel_with_messages_smoke() {
        use ratatui::{backend::TestBackend, Terminal};

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut screen = DualChatScreen::new();

        screen.push_plan_message("Analyzing requirements...".to_string());
        screen.push_plan_message("Creating implementation plan.".to_string());
        screen.push_build_message("Building module A...".to_string());
        screen.plan_status = "thinking".to_string();
        screen.build_status = "running".to_string();

        let theme = TuiTheme::default();

        terminal
            .draw(|f| {
                let area = f.area();
                let chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Percentage(50),
                        Constraint::Percentage(50),
                    ])
                    .split(area);

                screen.render_panel(
                    f,
                    chunks[0],
                    "Plan Agent",
                    &screen.plan_messages.clone(),
                    &screen.plan_status.clone(),
                    screen.plan_scroll,
                    true,
                    &theme,
                );

                screen.render_panel(
                    f,
                    chunks[1],
                    "Build Agent",
                    &screen.build_messages.clone(),
                    &screen.build_status.clone(),
                    screen.build_scroll,
                    false,
                    &theme,
                );
            })
            .unwrap();
    }

    #[test]
    fn test_render_panel_unfocused_smoke() {
        use ratatui::{backend::TestBackend, Terminal};

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let screen = DualChatScreen::new();
        let theme = TuiTheme::default();

        terminal
            .draw(|f| {
                let area = f.area();
                screen.render_panel(
                    f,
                    area,
                    "Build Agent",
                    &["line 1".to_string(), "line 2".to_string()],
                    "running",
                    0,
                    false,
                    &theme,
                );
            })
            .unwrap();
    }
}
