//! Re-exports color constants and indentation helpers from the theme module.
//!
//! This provides API compatibility with the opendev-tui style_tokens module,
//! allowing ported formatters to use `super::style_tokens::*` unchanged.

use ratatui::style::Color;

// Markdown heading colors
pub const HEADING_1: Color = Color::Rgb(186, 182, 215);
pub const CODE_FG: Color = Color::Rgb(150, 190, 160);
pub const CODE_BG: Color = Color::Rgb(30, 30, 30);
pub const BULLET: Color = Color::Rgb(140, 148, 160);
pub const BOLD_FG: Color = Color::Rgb(222, 216, 200);

// Core palette
pub const PRIMARY: Color = Color::Rgb(208, 212, 220);
pub const ACCENT: Color = Color::Rgb(130, 160, 255);
pub const SUBTLE: Color = Color::Rgb(154, 160, 172);
pub const SUCCESS: Color = Color::Rgb(106, 209, 143);
pub const ERROR: Color = Color::Rgb(255, 92, 87);
pub const WARNING: Color = Color::Rgb(255, 179, 71);
pub const BLUE_BRIGHT: Color = Color::Rgb(74, 158, 255);
pub const BLUE_PATH: Color = Color::Rgb(88, 166, 255);
pub const GOLD: Color = Color::Rgb(255, 215, 0);
pub const BORDER: Color = Color::Rgb(88, 88, 88);
pub const BORDER_ACCENT: Color = Color::Rgb(147, 147, 255);

// Semantic colors
pub const GREY: Color = Color::Rgb(122, 126, 134);
pub const THINKING_BG: Color = Color::Rgb(105, 105, 105);
pub const ORANGE: Color = Color::Rgb(255, 140, 0);
pub const GREEN_LIGHT: Color = Color::Rgb(137, 209, 133);
pub const GREEN_BRIGHT: Color = Color::Rgb(0, 255, 0);
pub const BLUE_TASK: Color = Color::Rgb(37, 150, 190);
pub const BLUE_LIGHT: Color = Color::Rgb(156, 207, 253);
pub const ORANGE_CAUTION: Color = Color::Rgb(255, 165, 0);
pub const CYAN: Color = Color::Rgb(0, 191, 255);
pub const DIM_GREY: Color = Color::Rgb(107, 114, 128);

// Brand colors
pub const AMBER: Color = Color::Rgb(212, 160, 23);
pub const AMBER_DIM: Color = Color::Rgb(140, 105, 15);
pub const MAGENTA: Color = Color::Rgb(192, 120, 221);

// Diff background colors
pub const DIFF_ADD_BG: Color = Color::Rgb(0, 40, 0);
pub const DIFF_DEL_BG: Color = Color::Rgb(40, 0, 0);

/// Thinking icon (⟡)
pub const THINKING_ICON: &str = "\u{27e1}";

/// Continuation character (⎿) for tool results and nested calls.
pub const CONTINUATION_CHAR: char = '\u{23bf}';

/// Tool result prefix: ⎿ + 2 spaces
pub const RESULT_PREFIX: &str = "\u{23bf}  ";

/// Centralized indentation constants for conversation rendering.
pub struct Indent;

impl Indent {
    /// 2-space continuation for wrapped lines under a message
    pub const CONT: &str = "  ";
    /// Thinking continuation: vertical line + space
    pub const THINKING_CONT: &str = "\u{2502} ";
    /// Tool result continuation lines
    pub const RESULT_CONT: &str = "     ";

    const DEPTH: [&str; 5] = ["", "  ", "    ", "      ", "        "];

    pub fn for_depth(depth: usize) -> std::borrow::Cow<'static, str> {
        if depth < Self::DEPTH.len() {
            std::borrow::Cow::Borrowed(Self::DEPTH[depth])
        } else {
            std::borrow::Cow::Owned(Self::CONT.repeat(depth))
        }
    }
}
