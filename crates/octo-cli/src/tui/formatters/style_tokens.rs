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

// Semantic colors
pub const PRIMARY: Color = Color::Rgb(208, 212, 220);
pub const GREY: Color = Color::Rgb(122, 126, 134);
pub const BLUE_BRIGHT: Color = Color::Rgb(74, 158, 255);
pub const THINKING_BG: Color = Color::Rgb(105, 105, 105);

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
