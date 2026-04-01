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
}
