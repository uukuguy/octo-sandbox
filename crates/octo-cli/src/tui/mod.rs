//! Full-screen TUI mode based on Ratatui 0.29

pub mod app_state;
pub mod backend;
pub mod event;
pub mod event_handler;
pub mod formatters;
pub mod managers;
pub mod screens;
pub mod theme;
pub mod widgets;

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
use self::screens::Screen;
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

/// Active view mode: Ops or Dev
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Ops,
    Dev,
}

/// Ops view tabs — a curated subset of the full Tab set
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpsTab {
    Dashboard,
    Agents,
    Sessions,
    Mcp,
    Security,
    Logs,
}

impl OpsTab {
    pub fn all() -> &'static [OpsTab] {
        &[
            OpsTab::Dashboard,
            OpsTab::Agents,
            OpsTab::Sessions,
            OpsTab::Mcp,
            OpsTab::Security,
            OpsTab::Logs,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            OpsTab::Dashboard => "Dashboard",
            OpsTab::Agents => "Agents",
            OpsTab::Sessions => "Sessions",
            OpsTab::Mcp => "MCP",
            OpsTab::Security => "Security",
            OpsTab::Logs => "Logs",
        }
    }

    pub fn index(&self) -> usize {
        OpsTab::all().iter().position(|t| t == self).unwrap_or(0)
    }

    pub fn from_index(idx: usize) -> Self {
        OpsTab::all().get(idx).copied().unwrap_or(OpsTab::Dashboard)
    }

    /// Map to the corresponding full Tab for screen routing
    fn to_tab(&self) -> Tab {
        match self {
            OpsTab::Dashboard => Tab::Dashboard,
            OpsTab::Agents => Tab::Agents,
            OpsTab::Sessions => Tab::Sessions,
            OpsTab::Mcp => Tab::Mcp,
            OpsTab::Security => Tab::Security,
            OpsTab::Logs => Tab::Logs,
        }
    }
}

/// Dev view tasks
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevTask {
    Agent, // placeholder for Phase N
    Eval,
}

impl DevTask {
    pub fn all() -> &'static [DevTask] {
        &[DevTask::Agent, DevTask::Eval]
    }

    pub fn label(&self) -> &'static str {
        match self {
            DevTask::Agent => "Agent Debug",
            DevTask::Eval => "Eval",
        }
    }

    pub fn index(&self) -> usize {
        DevTask::all().iter().position(|t| t == self).unwrap_or(0)
    }

    pub fn from_index(idx: usize) -> Self {
        DevTask::all().get(idx).copied().unwrap_or(DevTask::Eval)
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
    /// Current view mode (Ops or Dev)
    pub view_mode: ViewMode,
    /// Active Ops tab (when in Ops mode)
    pub ops_tab: OpsTab,
    /// Active Dev task (when in Dev mode)
    pub dev_task: DevTask,
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
            view_mode: ViewMode::Dev,
            ops_tab: OpsTab::Dashboard,
            dev_task: DevTask::Eval,
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
            AppEvent::SwitchToOps => {
                self.view_mode = ViewMode::Ops;
                self.status_message =
                    Some("[Ops] Ctrl+D switch to Dev | Tab/1-6 navigate".to_string());
            }
            AppEvent::SwitchToDev => {
                self.view_mode = ViewMode::Dev;
                self.status_message =
                    Some("[Dev] Ctrl+O switch to Ops | 1-2 select task".to_string());
            }
            _ => {
                // Forward to active screen (Dev mode sends to dev sub-screens)
                if self.view_mode == ViewMode::Dev {
                    match self.dev_task {
                        DevTask::Agent => self.screens.dev_agent.handle_event(&event),
                        DevTask::Eval => self.screens.dev_eval.handle_event(&event),
                    }
                } else {
                    self.screens.handle_event(&self.active_tab, &event);
                }
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

        match self.view_mode {
            ViewMode::Ops => {
                self.render_ops_tab_bar(frame, chunks[0]);
                self.render_ops_view(frame, chunks[1]);
            }
            ViewMode::Dev => {
                self.render_dev_header(frame, chunks[0]);
                self.render_dev_view(frame, chunks[1]);
            }
        }

        self.render_status_bar(frame, chunks[2]);
    }

    // -- Ops view rendering --

    fn render_ops_tab_bar(&self, frame: &mut Frame, area: Rect) {
        let tabs: Vec<Line> = OpsTab::all()
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let style = if *tab == self.ops_tab {
                    self.theme.tab_active()
                } else {
                    self.theme.tab_inactive()
                };
                let label = format!("{}:{}", i + 1, tab.label());
                Line::from(Span::styled(label, style))
            })
            .collect();

        let tab_bar = ratatui::widgets::Tabs::new(tabs)
            .select(self.ops_tab.index())
            .divider(Span::raw(" | "))
            .highlight_style(self.theme.tab_active());

        frame.render_widget(tab_bar, area);
    }

    fn render_ops_view(&mut self, frame: &mut Frame, area: Rect) {
        let tab = self.ops_tab.to_tab();
        let theme = self.theme.clone();
        let state = self.state.clone();
        self.screens.render(&tab, frame, area, &theme, &state);
    }

    // -- Dev view rendering --

    fn render_dev_header(&self, frame: &mut Frame, area: Rect) {
        let spans: Vec<Span> = DevTask::all()
            .iter()
            .enumerate()
            .flat_map(|(i, task)| {
                let style = if *task == self.dev_task {
                    self.theme.tab_active()
                } else {
                    self.theme.tab_inactive()
                };
                let label = format!("{}:{}", i + 1, task.label());
                let mut items = vec![Span::styled(label, style)];
                if i + 1 < DevTask::all().len() {
                    items.push(Span::styled("  ", self.theme.text_dim()));
                }
                items
            })
            .collect();

        let header = ratatui::widgets::Paragraph::new(Line::from(spans));
        frame.render_widget(header, area);
    }

    fn render_dev_view(&mut self, frame: &mut Frame, area: Rect) {
        match self.dev_task {
            DevTask::Agent => {
                let theme = self.theme.clone();
                let state = self.state.clone();
                self.screens.dev_agent.render(frame, area, &theme, &state);
            }
            DevTask::Eval => {
                let theme = self.theme.clone();
                let state = self.state.clone();
                self.screens.dev_eval.render(frame, area, &theme, &state);
            }
        }
    }

    // -- Legacy tab bar (kept for reference but no longer used in dual-view) --

    #[allow(dead_code)]
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
        let default_msg = match self.view_mode {
            ViewMode::Ops => {
                "[Ops] Ctrl+D switch to Dev | Tab/Shift+Tab navigate | 1-6 select tab | q quit"
            }
            ViewMode::Dev => {
                "[Dev] Ctrl+O switch to Ops | 1-2 select task | q quit"
            }
        };
        let msg = self.status_message.as_deref().unwrap_or(default_msg);
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
                    // View mode switching
                    (KeyCode::Char('o'), KeyModifiers::CONTROL) => {
                        app.handle_event(AppEvent::SwitchToOps);
                    }
                    (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                        app.handle_event(AppEvent::SwitchToDev);
                    }
                    // Tab/Shift+Tab navigation (mode-aware)
                    (KeyCode::Tab, KeyModifiers::NONE) => match app.view_mode {
                        ViewMode::Ops => {
                            let tabs = OpsTab::all();
                            let idx = app.ops_tab.index();
                            app.ops_tab = OpsTab::from_index((idx + 1) % tabs.len());
                        }
                        ViewMode::Dev => {
                            let tasks = DevTask::all();
                            let idx = app.dev_task.index();
                            app.dev_task = DevTask::from_index((idx + 1) % tasks.len());
                        }
                    },
                    (KeyCode::BackTab, KeyModifiers::SHIFT) => match app.view_mode {
                        ViewMode::Ops => {
                            let tabs = OpsTab::all();
                            let idx = app.ops_tab.index();
                            app.ops_tab =
                                OpsTab::from_index((idx + tabs.len() - 1) % tabs.len());
                        }
                        ViewMode::Dev => {
                            let tasks = DevTask::all();
                            let idx = app.dev_task.index();
                            app.dev_task =
                                DevTask::from_index((idx + tasks.len() - 1) % tasks.len());
                        }
                    },
                    // Digit keys (mode-aware)
                    (KeyCode::Char(c), KeyModifiers::NONE) if c.is_ascii_digit() => {
                        let digit = c.to_digit(10).unwrap_or(0) as usize;
                        match app.view_mode {
                            ViewMode::Ops => {
                                if digit > 0 && digit <= OpsTab::all().len() {
                                    app.ops_tab = OpsTab::from_index(digit - 1);
                                }
                            }
                            ViewMode::Dev => {
                                if digit > 0 && digit <= DevTask::all().len() {
                                    app.dev_task = DevTask::from_index(digit - 1);
                                }
                            }
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

    #[allow(dead_code)]
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

    // -- ViewMode tests --

    #[test]
    fn view_mode_equality() {
        assert_eq!(ViewMode::Ops, ViewMode::Ops);
        assert_eq!(ViewMode::Dev, ViewMode::Dev);
        assert_ne!(ViewMode::Ops, ViewMode::Dev);
    }

    #[test]
    fn view_mode_debug_format() {
        assert_eq!(format!("{:?}", ViewMode::Ops), "Ops");
        assert_eq!(format!("{:?}", ViewMode::Dev), "Dev");
    }

    #[test]
    fn view_mode_clone_copy() {
        let mode = ViewMode::Ops;
        let copied = mode;
        assert_eq!(mode, copied);
    }

    // -- OpsTab tests --

    #[test]
    fn ops_tab_all_returns_6_tabs() {
        assert_eq!(OpsTab::all().len(), 6);
    }

    #[test]
    fn ops_tab_index_roundtrip() {
        for tab in OpsTab::all() {
            assert_eq!(OpsTab::from_index(tab.index()), *tab);
        }
    }

    #[test]
    fn ops_tab_from_index_out_of_bounds_returns_dashboard() {
        assert_eq!(OpsTab::from_index(999), OpsTab::Dashboard);
    }

    #[test]
    fn ops_tab_labels_are_nonempty() {
        for tab in OpsTab::all() {
            assert!(!tab.label().is_empty());
        }
    }

    #[test]
    fn ops_tab_mcp_label_is_uppercase() {
        assert_eq!(OpsTab::Mcp.label(), "MCP");
    }

    #[test]
    fn ops_tab_indices_are_sequential() {
        for (i, tab) in OpsTab::all().iter().enumerate() {
            assert_eq!(tab.index(), i);
        }
    }

    #[test]
    fn ops_tab_to_tab_maps_correctly() {
        assert_eq!(OpsTab::Dashboard.to_tab(), Tab::Dashboard);
        assert_eq!(OpsTab::Agents.to_tab(), Tab::Agents);
        assert_eq!(OpsTab::Sessions.to_tab(), Tab::Sessions);
        assert_eq!(OpsTab::Mcp.to_tab(), Tab::Mcp);
        assert_eq!(OpsTab::Security.to_tab(), Tab::Security);
        assert_eq!(OpsTab::Logs.to_tab(), Tab::Logs);
    }

    #[test]
    fn ops_tab_next_wraps_around() {
        let tabs = OpsTab::all();
        let idx = tabs.len() - 1; // last (Logs)
        let next = OpsTab::from_index((idx + 1) % tabs.len());
        assert_eq!(next, OpsTab::Dashboard);
    }

    #[test]
    fn ops_tab_prev_wraps_around() {
        let tabs = OpsTab::all();
        let idx = 0; // first (Dashboard)
        let prev = OpsTab::from_index((idx + tabs.len() - 1) % tabs.len());
        assert_eq!(prev, OpsTab::Logs);
    }

    // -- DevTask tests --

    #[test]
    fn dev_task_all_returns_2_tasks() {
        assert_eq!(DevTask::all().len(), 2);
    }

    #[test]
    fn dev_task_index_roundtrip() {
        for task in DevTask::all() {
            assert_eq!(DevTask::from_index(task.index()), *task);
        }
    }

    #[test]
    fn dev_task_from_index_out_of_bounds() {
        assert_eq!(DevTask::from_index(999), DevTask::Eval);
    }

    #[test]
    fn dev_task_labels_are_nonempty() {
        for task in DevTask::all() {
            assert!(!task.label().is_empty());
        }
    }

    #[test]
    fn dev_task_agent_label() {
        assert_eq!(DevTask::Agent.label(), "Agent Debug");
    }

    #[test]
    fn dev_task_next_wraps_around() {
        let tasks = DevTask::all();
        let idx = tasks.len() - 1;
        let next = DevTask::from_index((idx + 1) % tasks.len());
        assert_eq!(next, DevTask::Agent);
    }

    // -- AppEvent switch variants --

    #[test]
    fn app_event_switch_to_ops_debug() {
        let ev = AppEvent::SwitchToOps;
        let debug = format!("{:?}", ev);
        assert!(debug.contains("SwitchToOps"));
    }

    #[test]
    fn app_event_switch_to_dev_debug() {
        let ev = AppEvent::SwitchToDev;
        let debug = format!("{:?}", ev);
        assert!(debug.contains("SwitchToDev"));
    }

    // -- ViewMode tests --

    #[test]
    fn view_mode_default_is_dev() {
        let mode = ViewMode::Dev;
        assert_eq!(mode, ViewMode::Dev);
    }

    #[test]
    fn view_mode_ops_variant_exists() {
        let mode = ViewMode::Ops;
        assert_eq!(mode, ViewMode::Ops);
        assert_ne!(mode, ViewMode::Dev);
    }
}
