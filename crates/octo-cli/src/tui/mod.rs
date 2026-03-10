//! Full-screen TUI mode based on Ratatui 0.29

pub mod backend;
pub mod event;
pub mod screens;
pub mod theme;

use std::io;
use std::sync::Arc;

use anyhow::Result;
use crossterm::{
    event::{self as ct_event, Event as CEvent, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use ratatui::Terminal;

use crate::commands::AppState;
use self::event::AppEvent;
use self::theme::TuiTheme;

/// Active tab/screen in the TUI
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tab {
    Welcome,
    Dashboard,
    Chat,
    Agents,
    Sessions,
    Memory,
    Skills,
    Mcp,
    Tools,
    Security,
    Settings,
    Logs,
}

impl Tab {
    pub fn all() -> &'static [Tab] {
        &[
            Tab::Welcome,
            Tab::Dashboard,
            Tab::Chat,
            Tab::Agents,
            Tab::Sessions,
            Tab::Memory,
            Tab::Skills,
            Tab::Mcp,
            Tab::Tools,
            Tab::Security,
            Tab::Settings,
            Tab::Logs,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            Tab::Welcome => "Welcome",
            Tab::Dashboard => "Dashboard",
            Tab::Chat => "Chat",
            Tab::Agents => "Agents",
            Tab::Sessions => "Sessions",
            Tab::Memory => "Memory",
            Tab::Skills => "Skills",
            Tab::Mcp => "MCP",
            Tab::Tools => "Tools",
            Tab::Security => "Security",
            Tab::Settings => "Settings",
            Tab::Logs => "Logs",
        }
    }

    pub fn index(&self) -> usize {
        Tab::all().iter().position(|t| t == self).unwrap_or(0)
    }

    pub fn from_index(idx: usize) -> Self {
        Tab::all().get(idx).copied().unwrap_or(Tab::Welcome)
    }
}

/// Main TUI application state
pub struct App {
    /// Current active tab
    pub active_tab: Tab,
    /// Whether the app should quit
    pub should_quit: bool,
    /// Application state (shared with CLI commands)
    pub state: Arc<AppState>,
    /// Theme
    pub theme: TuiTheme,
    /// Screen instances
    pub screens: screens::ScreenManager,
    /// Status bar message
    pub status_message: Option<String>,
}

impl App {
    pub fn new(state: Arc<AppState>, theme: TuiTheme) -> Self {
        Self {
            active_tab: Tab::Welcome,
            should_quit: false,
            state,
            theme,
            screens: screens::ScreenManager::new(),
            status_message: None,
        }
    }

    /// Handle an application event
    pub fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Quit => self.should_quit = true,
            AppEvent::NextTab => {
                let idx = self.active_tab.index();
                let tabs = Tab::all();
                self.active_tab = Tab::from_index((idx + 1) % tabs.len());
            }
            AppEvent::PrevTab => {
                let idx = self.active_tab.index();
                let tabs = Tab::all();
                self.active_tab = Tab::from_index((idx + tabs.len() - 1) % tabs.len());
            }
            AppEvent::SelectTab(tab) => {
                self.active_tab = tab;
            }
            AppEvent::SetStatus(msg) => {
                self.status_message = Some(msg);
            }
            AppEvent::ClearStatus => {
                self.status_message = None;
            }
            _ => {
                // Forward to active screen
                self.screens.handle_event(&self.active_tab, &event);
            }
        }
    }

    /// Render the TUI
    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();

        // Layout: top tab bar + main content + bottom status bar
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // tab bar
                Constraint::Min(1),   // content area
                Constraint::Length(1), // status bar
            ])
            .split(area);

        self.render_tab_bar(frame, chunks[0]);

        // Clone what we need to avoid borrow conflicts
        let tab = self.active_tab;
        let theme = self.theme.clone();
        let state = self.state.clone();
        self.screens
            .render(&tab, frame, chunks[1], &theme, &state);

        self.render_status_bar(frame, chunks[2]);
    }

    fn render_tab_bar(&self, frame: &mut Frame, area: Rect) {
        let tabs: Vec<Line> = Tab::all()
            .iter()
            .map(|tab| {
                let style = if *tab == self.active_tab {
                    self.theme.tab_active()
                } else {
                    self.theme.tab_inactive()
                };
                Line::from(Span::styled(tab.label(), style))
            })
            .collect();

        let tab_bar = ratatui::widgets::Tabs::new(tabs)
            .select(self.active_tab.index())
            .divider(Span::raw(" | "))
            .highlight_style(self.theme.tab_active());

        frame.render_widget(tab_bar, area);
    }

    fn render_status_bar(&self, frame: &mut Frame, area: Rect) {
        let msg = self
            .status_message
            .as_deref()
            .unwrap_or("Press ? for help  |  Tab/Shift+Tab to navigate  |  q to quit");
        let status = Line::from(Span::styled(msg, self.theme.text_dim()));
        frame.render_widget(ratatui::widgets::Paragraph::new(status), area);
    }
}

/// Run the TUI application
pub async fn run_tui(state: AppState, theme_name: crate::ui::theme::ThemeName) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let tui_theme = TuiTheme::from_cli_theme(theme_name);
    let mut app = App::new(Arc::new(state), tui_theme);

    // Event loop
    let tick_rate = std::time::Duration::from_millis(100);

    let result = run_event_loop(&mut terminal, &mut app, tick_rate);

    // Restore terminal (always, even on error)
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    tick_rate: std::time::Duration,
) -> Result<()> {
    loop {
        terminal.draw(|f| app.render(f))?;

        if ct_event::poll(tick_rate)? {
            if let CEvent::Key(key) = ct_event::read()? {
                match (key.code, key.modifiers) {
                    (KeyCode::Char('q'), KeyModifiers::NONE) => {
                        app.handle_event(AppEvent::Quit);
                    }
                    (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                        app.handle_event(AppEvent::Quit);
                    }
                    (KeyCode::Tab, KeyModifiers::NONE) => {
                        app.handle_event(AppEvent::NextTab);
                    }
                    (KeyCode::BackTab, KeyModifiers::SHIFT) => {
                        app.handle_event(AppEvent::PrevTab);
                    }
                    (KeyCode::Char(c), KeyModifiers::NONE) if c.is_ascii_digit() => {
                        let idx = c.to_digit(10).unwrap_or(0) as usize;
                        if idx > 0 && idx <= Tab::all().len() {
                            app.handle_event(AppEvent::SelectTab(Tab::from_index(idx - 1)));
                        }
                    }
                    _ => {
                        app.handle_event(AppEvent::Key(key));
                    }
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- Tab tests --

    #[test]
    fn tab_all_returns_12_tabs() {
        assert_eq!(Tab::all().len(), 12);
    }

    #[test]
    fn tab_index_roundtrip() {
        for tab in Tab::all() {
            assert_eq!(Tab::from_index(tab.index()), *tab);
        }
    }

    #[test]
    fn tab_from_index_out_of_bounds_returns_welcome() {
        assert_eq!(Tab::from_index(999), Tab::Welcome);
    }

    #[test]
    fn tab_labels_are_nonempty() {
        for tab in Tab::all() {
            assert!(!tab.label().is_empty());
        }
    }

    #[test]
    fn tab_mcp_label_is_uppercase() {
        assert_eq!(Tab::Mcp.label(), "MCP");
    }

    #[test]
    fn tab_indices_are_sequential() {
        let tabs = Tab::all();
        for (i, tab) in tabs.iter().enumerate() {
            assert_eq!(tab.index(), i);
        }
    }

    // -- AppEvent / App state transition tests --
    // These tests verify the state machine without needing a terminal.

    fn make_test_app() -> App {
        // We cannot easily construct a real AppState in unit tests (requires DB).
        // So we test only the parts that don't need rendering.
        // For state-machine tests, we build a minimal App with a dummy Arc<AppState>.
        // This is safe because we never call render() in these tests.
        //
        // NOTE: if AppState::new becomes cheaper, we can use it here.
        // For now, skip tests that need a real AppState.
        panic!("Cannot construct AppState in unit tests without DB");
    }

    #[test]
    fn tab_next_wraps_around() {
        // Test the wrapping logic directly without needing App
        let tabs = Tab::all();
        let idx = tabs.len() - 1; // last tab (Logs)
        let next = Tab::from_index((idx + 1) % tabs.len());
        assert_eq!(next, Tab::Welcome);
    }

    #[test]
    fn tab_prev_wraps_around() {
        let tabs = Tab::all();
        let idx = 0; // first tab (Welcome)
        let prev = Tab::from_index((idx + tabs.len() - 1) % tabs.len());
        assert_eq!(prev, Tab::Logs);
    }

    #[test]
    fn tab_next_from_middle() {
        let tabs = Tab::all();
        let idx = Tab::Chat.index();
        let next = Tab::from_index((idx + 1) % tabs.len());
        assert_eq!(next, Tab::Agents);
    }

    #[test]
    fn tab_equality() {
        assert_eq!(Tab::Welcome, Tab::Welcome);
        assert_ne!(Tab::Welcome, Tab::Chat);
    }

    #[test]
    fn tab_clone() {
        let tab = Tab::Dashboard;
        let cloned = tab;
        assert_eq!(tab, cloned);
    }

    #[test]
    fn tab_debug_format() {
        let debug = format!("{:?}", Tab::Mcp);
        assert_eq!(debug, "Mcp");
    }

    // -- TuiTheme tests --

    #[test]
    fn theme_default_is_cyan() {
        let theme = TuiTheme::default();
        assert_eq!(theme.accent, Color::Rgb(6, 182, 212));
    }

    #[test]
    fn theme_from_each_name() {
        // Ensure no panics for all theme names
        let names = [
            crate::ui::theme::ThemeName::Cyan,
            crate::ui::theme::ThemeName::Sgcc,
            crate::ui::theme::ThemeName::Blue,
            crate::ui::theme::ThemeName::Indigo,
            crate::ui::theme::ThemeName::Violet,
            crate::ui::theme::ThemeName::Emerald,
            crate::ui::theme::ThemeName::Amber,
            crate::ui::theme::ThemeName::Coral,
            crate::ui::theme::ThemeName::Rose,
            crate::ui::theme::ThemeName::Teal,
            crate::ui::theme::ThemeName::Sunset,
            crate::ui::theme::ThemeName::Slate,
        ];
        for name in names {
            let _theme = TuiTheme::from_cli_theme(name);
        }
    }

    // -- ScreenManager tests --

    #[test]
    fn screen_manager_new_does_not_panic() {
        let _sm = screens::ScreenManager::new();
    }

    // -- AppEvent tests --

    #[test]
    fn app_event_debug_format() {
        let event = AppEvent::Quit;
        let debug = format!("{:?}", event);
        assert!(debug.contains("Quit"));
    }

    #[test]
    fn app_event_clone() {
        let event = AppEvent::SetStatus("hello".to_string());
        let cloned = event.clone();
        if let AppEvent::SetStatus(msg) = cloned {
            assert_eq!(msg, "hello");
        } else {
            panic!("Clone produced wrong variant");
        }
    }

    #[test]
    fn app_event_tick() {
        let event = AppEvent::Tick;
        let debug = format!("{:?}", event);
        assert!(debug.contains("Tick"));
    }

    #[test]
    fn app_event_select_tab() {
        let event = AppEvent::SelectTab(Tab::Memory);
        if let AppEvent::SelectTab(tab) = event {
            assert_eq!(tab, Tab::Memory);
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn digit_to_tab_mapping() {
        // Digits 1-9 map to tabs 0-8 (first 9 tabs)
        for digit in 1..=9u32 {
            let idx = digit as usize;
            if idx <= Tab::all().len() {
                let tab = Tab::from_index(idx - 1);
                assert_eq!(tab.index(), idx - 1);
            }
        }
    }
}
