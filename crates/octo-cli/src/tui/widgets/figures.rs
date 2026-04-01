//! Platform-adaptive Unicode symbol set for TUI rendering.
//!
//! Provides a centralized collection of symbols used throughout the TUI:
//! status indicators, progress markers, separators, effort levels,
//! and spinner verbs for animated activity display.
//!
//! Inspired by CC-OSS `figures.ts`. All symbols are Unicode-safe and
//! render correctly in modern terminal emulators.

/// Circle variants for status and effort indicators.
pub mod circle {
    /// Empty circle: idle/inactive
    pub const EMPTY: char = '\u{25CB}'; // ○
    /// Half circle: medium effort/progress
    pub const HALF: char = '\u{25D0}'; // ◐
    /// Filled circle: high effort/active
    pub const FILLED: char = '\u{25CF}'; // ●
    /// Double circle: maximum effort
    pub const DOUBLE: char = '\u{25C9}'; // ◉
}

/// Status indicator symbols.
pub mod status {
    /// Success checkmark
    pub const SUCCESS: char = '\u{2713}'; // ✓
    /// Failure cross
    pub const FAILURE: char = '\u{2717}'; // ✗
    /// Warning triangle
    pub const WARNING: char = '\u{26A0}'; // ⚠
    /// Info circle
    pub const INFO: char = '\u{2139}'; // ℹ
    /// Pending dot
    pub const PENDING: char = '\u{2022}'; // •
    /// Completed record
    pub const COMPLETED: char = '\u{23FA}'; // ⏺
}

/// Arrow symbols for direction indicators.
pub mod arrow {
    pub const RIGHT: char = '\u{25B8}'; // ▸
    pub const LEFT: char = '\u{25C2}'; // ◂
    pub const UP: char = '\u{25B4}'; // ▴
    pub const DOWN: char = '\u{25BE}'; // ▾
    pub const RIGHT_DOUBLE: &str = "\u{00BB}"; // »
}

/// Box-drawing and separator symbols.
pub mod separator {
    /// Vertical bar separator
    pub const VERTICAL: char = '\u{2502}'; // │
    /// Horizontal bar
    pub const HORIZONTAL: char = '\u{2500}'; // ─
    /// Middle dot (for inline separators like "Enter to submit · Esc to cancel")
    pub const MIDDOT: char = '\u{00B7}'; // ·
    /// Elbow bracket for assistant message continuation
    pub const ELBOW: char = '\u{23BF}'; // ⎿
    /// Top-left corner
    pub const TOP_LEFT: &str = "\u{256D}\u{2500}"; // ╭─
    /// Bottom-left corner
    pub const BOTTOM_LEFT: &str = "\u{2570}\u{2500}"; // ╰─
}

/// Tree connector symbols for nested display.
pub mod tree {
    pub const BRANCH: &str = "\u{251C}\u{2500}"; // ├─
    pub const LAST: &str = "\u{2514}\u{2500}"; // └─
    pub const VERTICAL: &str = "\u{2502}"; // │
}

/// Diamond symbols for decorative markers.
pub mod diamond {
    pub const SMALL: char = '\u{25C6}'; // ◆
    pub const EMPTY: char = '\u{25C7}'; // ◇
}

/// Progress bar segments.
pub mod progress {
    pub const FILLED: char = '\u{25AE}'; // ▮
    pub const EMPTY: char = '\u{25AF}'; // ▯
}

/// Effort level display (for status bar reasoning effort indicator).
///
/// Returns `(symbol, label)` for 4 effort levels.
pub fn effort_indicator(level: u8) -> (char, &'static str) {
    match level {
        0 => (circle::EMPTY, "low"),
        1 => (circle::HALF, "med"),
        2 => (circle::FILLED, "high"),
        _ => (circle::DOUBLE, "max"),
    }
}

/// Random spinner verb for activity display.
///
/// Returns a randomized action verb to replace static "Thinking"/"Streaming"
/// labels, providing visual variety during long operations.
pub fn spinner_verb(tick: u64) -> &'static str {
    const VERBS: &[&str] = &[
        "Thinking",
        "Reasoning",
        "Analyzing",
        "Processing",
        "Computing",
        "Evaluating",
        "Considering",
        "Examining",
        "Reflecting",
        "Pondering",
        "Exploring",
        "Investigating",
        "Synthesizing",
        "Formulating",
        "Composing",
        "Assembling",
        "Constructing",
        "Mapping",
        "Tracing",
        "Scanning",
        "Parsing",
        "Resolving",
        "Unraveling",
        "Connecting",
        "Weaving",
        "Distilling",
        "Refining",
        "Calibrating",
        "Harmonizing",
        "Orchestrating",
        "Navigating",
        "Decoding",
        "Interpreting",
        "Inferring",
        "Deducing",
        "Hypothesizing",
        "Brainstorming",
        "Iterating",
        "Converging",
        "Crystallizing",
    ];
    // Simple deterministic selection: change verb every ~8 seconds (80 ticks at 100ms)
    let idx = ((tick / 80) as usize) % VERBS.len();
    VERBS[idx]
}

/// Stalled state detection for long-running operations.
///
/// Returns color hints based on elapsed duration:
/// - Normal (< 10s): None
/// - Warning (10-30s): Some(warning color hint)
/// - Stalled (> 30s): Some(error color hint)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StalledState {
    /// Normal operation, no timeout concern.
    Normal,
    /// Approaching timeout, show warning color.
    Warning,
    /// Operation appears stalled, show error color.
    Stalled,
}

impl StalledState {
    /// Determine stalled state from elapsed seconds.
    pub fn from_elapsed_secs(secs: u64) -> Self {
        if secs >= 30 {
            Self::Stalled
        } else if secs >= 10 {
            Self::Warning
        } else {
            Self::Normal
        }
    }

    /// Get the HSL hue for this stalled state (for breathing animation).
    /// Returns `(hue, saturation, lightness_base)`.
    pub fn breathing_params(&self) -> (f64, f64, f64) {
        match self {
            Self::Normal => (35.0, 1.0, 0.55),    // Amber breathing
            Self::Warning => (45.0, 1.0, 0.50),   // Yellow-ish warning
            Self::Stalled => (0.0, 0.9, 0.45),    // Red alert
        }
    }
}

/// Format elapsed duration with sub-second precision for short durations.
///
/// - < 1s: "0.Xs"
/// - < 10s: "X.Xs"
/// - < 60s: "Xs"
/// - >= 60s: "XmYs" or "Xm"
/// - >= 3600s: "Xh" only
pub fn format_elapsed_precise(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    let millis = d.as_millis();

    if millis < 1000 {
        format!("0.{}s", millis / 100)
    } else if secs < 10 {
        format!("{}.{}s", secs, (millis % 1000) / 100)
    } else if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        let m = secs / 60;
        let s = secs % 60;
        if s == 0 {
            format!("{}m", m)
        } else {
            format!("{}m{}s", m, s)
        }
    } else {
        let h = secs / 3600;
        format!("{}h", h)
    }
}

/// Shimmer effect: compute RGB color from phase for thinking animation.
///
/// Uses a slow sine wave through the hue spectrum to create a gentle
/// color shift effect. Phase should increment each tick.
pub fn shimmer_color(phase: f64) -> (u8, u8, u8) {
    // Gentle oscillation between purple and blue hues
    let hue = 260.0 + 40.0 * phase.sin(); // 220-300 range (blue-purple)
    let saturation = 0.6;
    let lightness = 0.55 + 0.1 * (phase * 1.5).cos();
    hsl_to_rgb(hue, saturation, lightness)
}

/// HSL to RGB conversion (h in degrees, s and l in 0.0-1.0).
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    (
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}

/// Context-aware hotkey hints for the footer.
///
/// Returns appropriate hints based on the current TUI mode.
pub fn hotkey_hints(is_streaming: bool, has_overlay: bool, has_approval: bool) -> Vec<(&'static str, &'static str)> {
    if has_approval {
        return vec![
            ("y", "allow"),
            ("n", "deny"),
            ("a", "allow session"),
        ];
    }

    if has_overlay {
        return vec![
            ("Esc", "close"),
        ];
    }

    if is_streaming {
        return vec![
            ("Esc", "cancel"),
            ("Ctrl+C", "interrupt"),
        ];
    }

    // Default idle hints
    vec![
        ("Enter", "submit"),
        ("Esc", "cancel"),
        ("/help", "commands"),
        ("Ctrl+D", "debug"),
    ]
}

/// Reduced motion configuration.
///
/// When enabled, disables shimmer, breathing, and stalled color transitions.
/// Animations fall back to static indicators.
#[derive(Debug, Clone, Copy)]
pub struct ReducedMotion {
    pub enabled: bool,
}

impl ReducedMotion {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    /// Check if animations should be suppressed.
    pub fn suppress_animation(&self) -> bool {
        self.enabled
    }
}

impl Default for ReducedMotion {
    fn default() -> Self {
        Self { enabled: false }
    }
}

/// Interpolate between two RGB colors.
///
/// `t` ranges from 0.0 (fully `c1`) to 1.0 (fully `c2`).
pub fn interpolate_rgb(c1: (u8, u8, u8), c2: (u8, u8, u8), t: f64) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    (
        (c1.0 as f64 + (c2.0 as f64 - c1.0 as f64) * t) as u8,
        (c1.1 as f64 + (c2.1 as f64 - c1.1 as f64) * t) as u8,
        (c1.2 as f64 + (c2.2 as f64 - c1.2 as f64) * t) as u8,
    )
}

/// Render a string with character-level shimmer (glimmer) effect.
///
/// A bright spot slides across the text, creating a wave-like glow.
/// Returns `(char_index, intensity)` pairs where intensity is 0.0-1.0.
///
/// `tick` should increment each frame (~100ms). The wave slides at
/// ~1 character per tick.
pub fn shimmer_intensities(text_len: usize, tick: u64) -> Vec<f64> {
    let cycle_len = text_len + 20; // extra gap between waves
    let pos = (tick as usize) % cycle_len;

    (0..text_len)
        .map(|i| {
            let dist = (i as isize - pos as isize).unsigned_abs();
            if dist < 5 {
                1.0 - (dist as f64 / 5.0)
            } else {
                0.0
            }
        })
        .collect()
}

/// Permission mode for cycling via Shift+Tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionMode {
    /// Read-only: no write/execute tools allowed
    ReadOnly,
    /// Supervised: ask for approval on risky actions
    Supervised,
    /// Full: all actions auto-approved
    Full,
}

impl PermissionMode {
    /// Cycle to the next permission mode.
    pub fn next(self) -> Self {
        match self {
            Self::ReadOnly => Self::Supervised,
            Self::Supervised => Self::Full,
            Self::Full => Self::ReadOnly,
        }
    }

    /// Display label for the status bar.
    pub fn label(&self) -> &'static str {
        match self {
            Self::ReadOnly => "ReadOnly",
            Self::Supervised => "Supervised",
            Self::Full => "Full",
        }
    }

    /// Color hint for the permission mode.
    pub fn color_rgb(&self) -> (u8, u8, u8) {
        match self {
            Self::ReadOnly => (137, 209, 133),   // green
            Self::Supervised => (255, 215, 0),   // gold
            Self::Full => (255, 140, 0),         // orange
        }
    }
}

impl Default for PermissionMode {
    fn default() -> Self {
        Self::Supervised
    }
}

/// History search state for Ctrl+R reverse incremental search.
#[derive(Debug, Clone)]
pub struct HistorySearchState {
    /// Whether search mode is active.
    pub active: bool,
    /// Current search query.
    pub query: String,
    /// Index of the matched entry (into the history list, from end).
    pub match_index: usize,
    /// The matched history entry text (if any).
    pub matched_text: Option<String>,
}

impl HistorySearchState {
    pub fn new() -> Self {
        Self {
            active: false,
            query: String::new(),
            match_index: 0,
            matched_text: None,
        }
    }

    /// Enter search mode.
    pub fn activate(&mut self) {
        self.active = true;
        self.query.clear();
        self.match_index = 0;
        self.matched_text = None;
    }

    /// Exit search mode.
    pub fn deactivate(&mut self) {
        self.active = false;
        self.query.clear();
        self.match_index = 0;
        self.matched_text = None;
    }

    /// Search history entries for the current query.
    ///
    /// `history` should be in chronological order (oldest first).
    /// Returns true if a match was found.
    pub fn search(&mut self, history: &[String]) -> bool {
        if self.query.is_empty() {
            self.matched_text = None;
            return false;
        }

        let query_lower = self.query.to_lowercase();
        let mut found_count = 0;

        // Search from newest to oldest
        for entry in history.iter().rev() {
            if entry.to_lowercase().contains(&query_lower) {
                if found_count == self.match_index {
                    self.matched_text = Some(entry.clone());
                    return true;
                }
                found_count += 1;
            }
        }

        // Wrap around
        if self.match_index > 0 && found_count > 0 {
            self.match_index = 0;
            return self.search(history);
        }

        self.matched_text = None;
        false
    }

    /// Move to the next match (Ctrl+R pressed again while in search mode).
    pub fn next_match(&mut self) {
        self.match_index += 1;
    }

    /// Format the search prompt line.
    pub fn prompt_line(&self) -> String {
        if let Some(ref matched) = self.matched_text {
            format!("(reverse-i-search)`{}': {}", self.query, matched)
        } else if self.query.is_empty() {
            "(reverse-i-search)`': ".to_string()
        } else {
            format!("(reverse-i-search)`{}': [no match]", self.query)
        }
    }
}

impl Default for HistorySearchState {
    fn default() -> Self {
        Self::new()
    }
}

/// Vim editing mode for TUI input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimMode {
    /// Normal mode: navigation and commands.
    Normal,
    /// Insert mode: direct text input.
    Insert,
    /// Visual mode: text selection.
    Visual,
}

impl VimMode {
    /// Status bar label for the current vim mode.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Insert => "INSERT",
            Self::Visual => "VISUAL",
        }
    }

    /// Color for the mode indicator.
    pub fn color_rgb(&self) -> (u8, u8, u8) {
        match self {
            Self::Normal => (130, 160, 255),    // blue
            Self::Insert => (137, 209, 133),    // green
            Self::Visual => (192, 120, 221),    // magenta
        }
    }
}

impl Default for VimMode {
    fn default() -> Self {
        Self::Insert // Start in Insert mode (normal TUI behavior)
    }
}

/// Vim state for the TUI input system.
#[derive(Debug, Clone)]
pub struct VimState {
    /// Current editing mode.
    pub mode: VimMode,
    /// Whether vim mode is enabled (vs normal emacs-style editing).
    pub enabled: bool,
    /// Visual mode selection start position.
    pub visual_start: Option<usize>,
}

impl VimState {
    pub fn new(enabled: bool) -> Self {
        Self {
            mode: if enabled { VimMode::Normal } else { VimMode::Insert },
            enabled,
            visual_start: None,
        }
    }

    /// Toggle vim mode on/off.
    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
        if self.enabled {
            self.mode = VimMode::Normal;
        } else {
            self.mode = VimMode::Insert;
        }
        self.visual_start = None;
    }

    /// Enter insert mode (from normal/visual).
    pub fn enter_insert(&mut self) {
        self.mode = VimMode::Insert;
        self.visual_start = None;
    }

    /// Enter normal mode (from insert/visual).
    pub fn enter_normal(&mut self) {
        self.mode = VimMode::Normal;
        self.visual_start = None;
    }

    /// Enter visual mode (from normal).
    pub fn enter_visual(&mut self, cursor_pos: usize) {
        self.mode = VimMode::Visual;
        self.visual_start = Some(cursor_pos);
    }
}

impl Default for VimState {
    fn default() -> Self {
        Self::new(false)
    }
}

/// Model selector state for Meta+P popup.
#[derive(Debug, Clone)]
pub struct ModelSelectorState {
    /// Whether the selector popup is visible.
    pub visible: bool,
    /// Available model names.
    pub models: Vec<String>,
    /// Currently selected index.
    pub selected: usize,
    /// Index of the active (current) model.
    pub active_index: usize,
}

impl ModelSelectorState {
    pub fn new() -> Self {
        Self {
            visible: false,
            models: vec![
                "claude-sonnet-4-6".to_string(),
                "claude-opus-4-6".to_string(),
                "claude-haiku-4-5".to_string(),
            ],
            selected: 0,
            active_index: 0,
        }
    }

    /// Toggle visibility.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible {
            self.selected = self.active_index;
        }
    }

    /// Select previous item.
    pub fn prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Select next item.
    pub fn next(&mut self) {
        if self.selected + 1 < self.models.len() {
            self.selected += 1;
        }
    }

    /// Confirm selection and return the chosen model name.
    pub fn confirm(&mut self) -> Option<String> {
        if self.selected < self.models.len() {
            self.active_index = self.selected;
            self.visible = false;
            Some(self.models[self.selected].clone())
        } else {
            None
        }
    }

    /// Set available models (from runtime configuration).
    pub fn set_models(&mut self, models: Vec<String>, active_model: &str) {
        self.models = models;
        self.active_index = self.models.iter().position(|m| m == active_model).unwrap_or(0);
        self.selected = self.active_index;
    }
}

impl Default for ModelSelectorState {
    fn default() -> Self {
        Self::new()
    }
}

/// Sub-session entry for multi-session spinner tree (E-17).
#[derive(Debug, Clone)]
pub struct SubSessionEntry {
    /// Session display name.
    pub name: String,
    /// Current status description.
    pub status: String,
    /// Whether this session is currently active/running.
    pub active: bool,
    /// Elapsed seconds since session started.
    pub elapsed_secs: u64,
}

impl SubSessionEntry {
    pub fn new(name: String, status: String, active: bool, elapsed_secs: u64) -> Self {
        Self { name, status, active, elapsed_secs }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_effort_indicator_levels() {
        assert_eq!(effort_indicator(0).0, circle::EMPTY);
        assert_eq!(effort_indicator(1).0, circle::HALF);
        assert_eq!(effort_indicator(2).0, circle::FILLED);
        assert_eq!(effort_indicator(3).0, circle::DOUBLE);
        assert_eq!(effort_indicator(255).0, circle::DOUBLE);
    }

    #[test]
    fn test_spinner_verbs_rotate() {
        let v0 = spinner_verb(0);
        let v1 = spinner_verb(80); // next verb
        assert_ne!(v0, v1);
        // Wraps around after all verbs
        let v_wrap = spinner_verb(80 * 40);
        assert_eq!(v0, v_wrap);
    }

    #[test]
    fn test_stalled_state() {
        assert_eq!(StalledState::from_elapsed_secs(5), StalledState::Normal);
        assert_eq!(StalledState::from_elapsed_secs(15), StalledState::Warning);
        assert_eq!(StalledState::from_elapsed_secs(45), StalledState::Stalled);
    }

    #[test]
    fn test_format_elapsed_precise() {
        assert_eq!(format_elapsed_precise(std::time::Duration::from_millis(500)), "0.5s");
        assert_eq!(format_elapsed_precise(std::time::Duration::from_millis(3200)), "3.2s");
        assert_eq!(format_elapsed_precise(std::time::Duration::from_secs(15)), "15s");
        assert_eq!(format_elapsed_precise(std::time::Duration::from_secs(125)), "2m5s");
        assert_eq!(format_elapsed_precise(std::time::Duration::from_secs(3600)), "1h");
    }

    #[test]
    fn test_shimmer_color_range() {
        let (r, g, b) = shimmer_color(0.0);
        assert!(r > 0 || g > 0 || b > 0, "Should produce non-black color");

        let (r2, g2, b2) = shimmer_color(std::f64::consts::PI);
        // Colors should shift across phases
        assert!(r != r2 || g != g2 || b != b2, "Different phases should produce different colors");
    }

    #[test]
    fn test_hotkey_hints_modes() {
        let idle = hotkey_hints(false, false, false);
        assert!(idle.iter().any(|(k, _)| *k == "Enter"));

        let streaming = hotkey_hints(true, false, false);
        assert!(streaming.iter().any(|(k, _)| *k == "Esc"));

        let approval = hotkey_hints(false, false, true);
        assert!(approval.iter().any(|(k, _)| *k == "y"));
    }

    #[test]
    fn test_reduced_motion() {
        let rm = ReducedMotion::default();
        assert!(!rm.suppress_animation());

        let rm_on = ReducedMotion::new(true);
        assert!(rm_on.suppress_animation());
    }

    #[test]
    fn test_stalled_breathing_params() {
        let (h, _, _) = StalledState::Normal.breathing_params();
        assert!((h - 35.0).abs() < 0.001);

        let (h, _, _) = StalledState::Stalled.breathing_params();
        assert!(h < 1.0, "Stalled should use red hue");
    }

    #[test]
    fn test_interpolate_rgb() {
        let black = (0u8, 0u8, 0u8);
        let white = (255u8, 255u8, 255u8);

        let mid = interpolate_rgb(black, white, 0.5);
        assert_eq!(mid, (127, 127, 127));

        assert_eq!(interpolate_rgb(black, white, 0.0), black);
        assert_eq!(interpolate_rgb(black, white, 1.0), white);
    }

    #[test]
    fn test_shimmer_intensities() {
        let intensities = shimmer_intensities(10, 3);
        assert_eq!(intensities.len(), 10);
        // Position 3 should have max intensity
        assert!((intensities[3] - 1.0).abs() < 0.001);
        // Far positions should be 0
        assert!(intensities[9] < 0.001);
    }

    #[test]
    fn test_permission_mode_cycling() {
        let mode = PermissionMode::ReadOnly;
        assert_eq!(mode.next(), PermissionMode::Supervised);
        assert_eq!(mode.next().next(), PermissionMode::Full);
        assert_eq!(mode.next().next().next(), PermissionMode::ReadOnly);
    }

    #[test]
    fn test_permission_mode_label() {
        assert_eq!(PermissionMode::ReadOnly.label(), "ReadOnly");
        assert_eq!(PermissionMode::Supervised.label(), "Supervised");
        assert_eq!(PermissionMode::Full.label(), "Full");
    }

    #[test]
    fn test_history_search() {
        let mut state = HistorySearchState::new();
        state.activate();
        assert!(state.active);

        state.query = "git".to_string();
        let history = vec![
            "ls -la".to_string(),
            "git status".to_string(),
            "cargo build".to_string(),
            "git diff".to_string(),
        ];

        assert!(state.search(&history));
        assert_eq!(state.matched_text.as_deref(), Some("git diff"));

        // Next match
        state.next_match();
        assert!(state.search(&history));
        assert_eq!(state.matched_text.as_deref(), Some("git status"));
    }

    #[test]
    fn test_history_search_no_match() {
        let mut state = HistorySearchState::new();
        state.activate();
        state.query = "nonexistent".to_string();
        let history = vec!["hello".to_string()];
        assert!(!state.search(&history));
        assert!(state.matched_text.is_none());
    }

    #[test]
    fn test_history_search_prompt() {
        let mut state = HistorySearchState::new();
        state.activate();
        assert!(state.prompt_line().contains("(reverse-i-search)"));

        state.query = "git".to_string();
        state.matched_text = Some("git status".to_string());
        assert!(state.prompt_line().contains("git status"));
    }

    #[test]
    fn test_vim_mode_transitions() {
        let mut vim = VimState::new(true);
        assert_eq!(vim.mode, VimMode::Normal);

        vim.enter_insert();
        assert_eq!(vim.mode, VimMode::Insert);

        vim.enter_normal();
        assert_eq!(vim.mode, VimMode::Normal);

        vim.enter_visual(5);
        assert_eq!(vim.mode, VimMode::Visual);
        assert_eq!(vim.visual_start, Some(5));
    }

    #[test]
    fn test_vim_mode_toggle() {
        let mut vim = VimState::new(false);
        assert!(!vim.enabled);
        assert_eq!(vim.mode, VimMode::Insert);

        vim.toggle();
        assert!(vim.enabled);
        assert_eq!(vim.mode, VimMode::Normal);

        vim.toggle();
        assert!(!vim.enabled);
        assert_eq!(vim.mode, VimMode::Insert);
    }

    #[test]
    fn test_vim_mode_labels() {
        assert_eq!(VimMode::Normal.label(), "NORMAL");
        assert_eq!(VimMode::Insert.label(), "INSERT");
        assert_eq!(VimMode::Visual.label(), "VISUAL");
    }

    #[test]
    fn test_model_selector() {
        let mut sel = ModelSelectorState::new();
        assert!(!sel.visible);
        assert_eq!(sel.models.len(), 3);

        sel.toggle();
        assert!(sel.visible);

        sel.next();
        assert_eq!(sel.selected, 1);
        sel.next();
        assert_eq!(sel.selected, 2);
        sel.next(); // capped
        assert_eq!(sel.selected, 2);

        sel.prev();
        assert_eq!(sel.selected, 1);

        let chosen = sel.confirm();
        assert_eq!(chosen.as_deref(), Some("claude-opus-4-6"));
        assert!(!sel.visible);
        assert_eq!(sel.active_index, 1);
    }

    #[test]
    fn test_model_selector_set_models() {
        let mut sel = ModelSelectorState::new();
        sel.set_models(
            vec!["gpt-4o".to_string(), "claude-sonnet".to_string()],
            "claude-sonnet",
        );
        assert_eq!(sel.active_index, 1);
        assert_eq!(sel.selected, 1);
    }

    #[test]
    fn test_sub_session_entry() {
        let entry = SubSessionEntry::new(
            "coder-1".to_string(),
            "Implementing feature".to_string(),
            true,
            45,
        );
        assert!(entry.active);
        assert_eq!(entry.elapsed_secs, 45);
    }
}
