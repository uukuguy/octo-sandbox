//! User input/prompt widget with multiline editing and cursor rendering.
//!
//! Displays a mode-colored separator line with mode indicator,
//! queue count, and multiline input area with visible cursor.
//! Light bottom border line for visual separation from status bar.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};
use unicode_width::UnicodeWidthChar;

use crate::tui::formatters::style_tokens;

/// Widget for the user input area.
pub struct InputWidget<'a> {
    buffer: &'a str,
    cursor: usize,
    mode: &'a str,
    pending_count: usize,
    has_focus: bool,
    is_streaming: bool,
    has_overlay: bool,
    has_approval: bool,
}

/// Result of rendering the input widget, including cursor position for IME.
pub struct InputRenderResult {
    /// Absolute cursor position (x, y) for terminal IME placement.
    pub cursor_position: Option<(u16, u16)>,
}

impl<'a> InputWidget<'a> {
    pub fn new(buffer: &'a str, cursor: usize, mode: &'a str, pending_count: usize) -> Self {
        Self {
            buffer,
            cursor,
            mode,
            pending_count,
            has_focus: true,
            is_streaming: false,
            has_overlay: false,
            has_approval: false,
        }
    }

    /// Set streaming/overlay/approval state for context-aware hotkey hints.
    pub fn hint_context(mut self, is_streaming: bool, has_overlay: bool, has_approval: bool) -> Self {
        self.is_streaming = is_streaming;
        self.has_overlay = has_overlay;
        self.has_approval = has_approval;
        self
    }

    pub fn has_focus(mut self, focused: bool) -> Self {
        self.has_focus = focused;
        self
    }

    /// Render the widget and return cursor position for IME.
    pub fn render_with_cursor(self, area: Rect, buf: &mut Buffer) -> InputRenderResult {
        let cursor_pos = self.compute_cursor_position(area);
        Widget::render(self, area, buf);
        InputRenderResult {
            cursor_position: cursor_pos,
        }
    }

    /// Compute absolute cursor position for IME placement.
    ///
    /// Uses display width (not byte length) for correct positioning with CJK text.
    fn compute_cursor_position(&self, area: Rect) -> Option<(u16, u16)> {
        if !self.has_focus || area.height < 2 {
            return None;
        }
        let text_y = area.y + 1; // below separator
        // "❯ " prefix: ❯ is 1 display column in most terminals + 1 space = 2
        let prefix_width = 2u16;

        if self.buffer.is_empty() {
            return Some((area.x + prefix_width, text_y));
        }

        let input_lines: Vec<&str> = self.buffer.split('\n').collect();
        let mut cursor_line = 0usize;
        let mut cursor_byte_col = 0usize;
        let mut pos = 0usize;
        for (i, line) in input_lines.iter().enumerate() {
            if self.cursor <= pos + line.len() {
                cursor_line = i;
                cursor_byte_col = self.cursor - pos;
                break;
            }
            pos += line.len() + 1;
            if i == input_lines.len() - 1 {
                cursor_line = i;
                cursor_byte_col = line.len();
            }
        }

        // Convert byte offset within line to display width
        let line_text = input_lines[cursor_line];
        let display_col: usize = line_text[..cursor_byte_col]
            .chars()
            .map(|c| UnicodeWidthChar::width(c).unwrap_or(0))
            .sum();

        let x = area.x + prefix_width + display_col as u16;
        let y = text_y + cursor_line as u16;
        Some((x, y))
    }

    /// Get the accent color and optional mode label based on current mode.
    fn mode_style(&self) -> (ratatui::style::Color, &'static str) {
        match self.mode {
            "Streaming" => (style_tokens::GREEN_LIGHT, "\u{25B8} Streaming"),
            "Thinking" => (style_tokens::MAGENTA, "\u{25E6} Thinking"),
            "PLAN" => (style_tokens::GREEN_LIGHT, "Plan"),
            _ => (style_tokens::ACCENT, ""),  // no label when idle
        }
    }

    /// Whether input text should be dimmed (during streaming/thinking).
    fn is_dimmed(&self) -> bool {
        self.mode == "Streaming" || self.mode == "Thinking"
    }
}

impl Widget for InputWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 2 {
            return;
        }

        let (accent, mode_label) = self.mode_style();
        let dimmed = self.is_dimmed();

        // Row 0: separator line with optional mode-colored indicator
        let sep_style = Style::default().fg(accent);
        let mut spans: Vec<Span> = Vec::new();
        let mut used = 0usize;

        if mode_label.is_empty() {
            // No mode label — plain separator line
        } else {
            let mode_text = format!("\u{2500}\u{2500} {} ", mode_label);
            used += mode_text.len();
            spans.push(Span::styled(
                mode_text,
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ));
        }

        if self.pending_count > 0 {
            let queue_text = format!(
                "\u{2500}\u{2500} {} message{} queued (ESC) ",
                self.pending_count,
                if self.pending_count == 1 { "" } else { "s" }
            );
            used += queue_text.len();
            spans.push(Span::styled(
                queue_text,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        // Context-aware hotkey hints (right-aligned, middot-separated)
        let hints = super::figures::hotkey_hints(self.is_streaming, self.has_overlay, self.has_approval);
        let hint_text: String = hints
            .iter()
            .map(|(key, desc)| format!("{} {}", key, desc))
            .collect::<Vec<_>>()
            .join(&format!(" {} ", super::figures::separator::MIDDOT));
        let hint_width = hint_text.len() + 2; // padding

        let remaining = (area.width as usize).saturating_sub(used);
        if remaining > hint_width + 4 {
            let dash_count = remaining - hint_width;
            spans.push(Span::styled("\u{2500}".repeat(dash_count), sep_style));
            spans.push(Span::styled(
                format!(" {} ", hint_text),
                Style::default().fg(style_tokens::DIM_GREY),
            ));
        } else {
            spans.push(Span::styled("\u{2500}".repeat(remaining), sep_style));
        }
        let sep_line = Line::from(spans);
        buf.set_line(area.left(), area.top(), &sep_line, area.width);

        // Text area below top separator (no bottom border — status bar provides its own top border)
        let text_height = area.height.saturating_sub(1); // -1 top separator
        if text_height == 0 {
            return;
        }
        let text_area = Rect {
            x: area.x,
            y: area.y + 1,
            width: area.width,
            height: text_height,
        };

        // Text style: dimmed when streaming/thinking
        let text_fg = if dimmed {
            style_tokens::GREY
        } else {
            Color::Reset // default terminal color
        };

        // Cursor style: solid block when focused, dim outline when unfocused
        let cursor_style = if self.has_focus {
            Style::default().fg(Color::Black).bg(Color::White)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        if self.buffer.is_empty() {
            // Empty: show prompt with block cursor (or dim placeholder when unfocused)
            let prefix = Span::styled(
                "\u{276f} ".to_string(),
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            );
            let cursor_span = if self.has_focus {
                Span::styled(" ", cursor_style) // block cursor
            } else {
                Span::styled("\u{2502}", cursor_style) // thin line cursor when unfocused
            };
            let content = vec![prefix, cursor_span];
            Paragraph::new(Line::from(content)).render(text_area, buf);
        } else {
            let input_lines: Vec<&str> = self.buffer.split('\n').collect();

            // Compute cursor line and column
            let mut cursor_line = 0;
            let mut cursor_col = 0;
            let mut pos = 0;
            for (i, line) in input_lines.iter().enumerate() {
                if self.cursor <= pos + line.len() {
                    cursor_line = i;
                    cursor_col = self.cursor - pos;
                    break;
                }
                pos += line.len() + 1;
                if i == input_lines.len() - 1 {
                    cursor_line = i;
                    cursor_col = line.len();
                }
            }

            let prefix_style = Style::default().fg(accent).add_modifier(Modifier::BOLD);

            for (i, line_text) in input_lines.iter().enumerate() {
                if i as u16 >= text_height {
                    break;
                }
                let row = text_area.y + i as u16;
                let pfx = if i == 0 { "\u{276f} " } else { "  " };

                if i == cursor_line {
                    let before = &line_text[..cursor_col];
                    let (cursor_char, after) = if cursor_col < line_text.len() {
                        let ch = line_text[cursor_col..].chars().next().unwrap();
                        let end = cursor_col + ch.len_utf8();
                        (&line_text[cursor_col..end], &line_text[end..])
                    } else if self.has_focus {
                        (" ", "")
                    } else {
                        ("\u{2502}", "") // thin line when unfocused at end of line
                    };
                    let spans = Line::from(vec![
                        Span::styled(pfx, prefix_style),
                        Span::styled(before.to_string(), Style::default().fg(text_fg)),
                        Span::styled(cursor_char.to_string(), cursor_style),
                        Span::styled(after.to_string(), Style::default().fg(text_fg)),
                    ]);
                    buf.set_line(text_area.x, row, &spans, text_area.width);
                } else {
                    let spans = Line::from(vec![
                        Span::styled(pfx, prefix_style),
                        Span::styled(line_text.to_string(), Style::default().fg(text_fg)),
                    ]);
                    buf.set_line(text_area.x, row, &spans, text_area.width);
                }
            }
        }

        // No bottom border — the status bar's top border provides the separator.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_widget_creation() {
        let _widget = InputWidget::new("hello", 3, "NORMAL", 0);
    }

    #[test]
    fn test_input_widget_empty() {
        let _widget = InputWidget::new("", 0, "NORMAL", 0);
    }

    #[test]
    fn test_queue_indicator_in_separator() {
        let area = Rect::new(0, 0, 60, 3);
        let mut buf = Buffer::empty(area);

        let widget = InputWidget::new("", 0, "NORMAL", 2);
        widget.render(area, &mut buf);

        let rendered: String = (0..area.width)
            .map(|x| {
                buf.cell((x, 0))
                    .map_or(' ', |c| c.symbol().chars().next().unwrap_or(' '))
            })
            .collect();
        assert!(
            rendered.contains("2 messages queued"),
            "Expected '2 messages queued' in separator line, got: {rendered:?}"
        );
    }

    #[test]
    fn test_queue_indicator_single_message() {
        let area = Rect::new(0, 0, 60, 3);
        let mut buf = Buffer::empty(area);

        let widget = InputWidget::new("", 0, "NORMAL", 1);
        widget.render(area, &mut buf);

        let rendered: String = (0..area.width)
            .map(|x| {
                buf.cell((x, 0))
                    .map_or(' ', |c| c.symbol().chars().next().unwrap_or(' '))
            })
            .collect();
        assert!(
            rendered.contains("1 message queued"),
            "Expected '1 message queued' in separator line, got: {rendered:?}"
        );
        assert!(
            !rendered.contains("1 messages"),
            "Should use singular 'message' for count=1"
        );
    }

    #[test]
    fn test_input_no_bottom_border() {
        let area = Rect::new(0, 0, 60, 3);
        let mut buf = Buffer::empty(area);

        let widget = InputWidget::new("hello", 5, "NORMAL", 0);
        widget.render(area, &mut buf);

        // Bottom row (row 2) should NOT be a border line (status bar provides its own)
        let bottom: String = (0..area.width)
            .map(|x| {
                buf.cell((x, 2))
                    .map_or(' ', |c| c.symbol().chars().next().unwrap_or(' '))
            })
            .collect();
        let dash_count = bottom.chars().filter(|c| *c == '\u{2500}').count();
        assert!(
            dash_count == 0,
            "Bottom row should NOT have a border line, got {dash_count} dashes"
        );
    }

    #[test]
    fn test_input_streaming_mode_label() {
        let area = Rect::new(0, 0, 60, 2);
        let mut buf = Buffer::empty(area);

        let widget = InputWidget::new("", 0, "Streaming", 0);
        widget.render(area, &mut buf);

        let top: String = (0..area.width)
            .map(|x| {
                buf.cell((x, 0))
                    .map_or(' ', |c| c.symbol().chars().next().unwrap_or(' '))
            })
            .collect();
        assert!(top.contains("Streaming"), "Should show streaming mode label");
    }

    #[test]
    fn test_input_thinking_mode_label() {
        let area = Rect::new(0, 0, 60, 2);
        let mut buf = Buffer::empty(area);

        let widget = InputWidget::new("", 0, "Thinking", 0);
        widget.render(area, &mut buf);

        let top: String = (0..area.width)
            .map(|x| {
                buf.cell((x, 0))
                    .map_or(' ', |c| c.symbol().chars().next().unwrap_or(' '))
            })
            .collect();
        assert!(top.contains("Thinking"), "Should show thinking mode label");
    }

    #[test]
    fn test_input_empty_shows_cursor_only() {
        let area = Rect::new(0, 0, 60, 2);
        let mut buf = Buffer::empty(area);

        let widget = InputWidget::new("", 0, "NORMAL", 0);
        widget.render(area, &mut buf);

        let text_row: String = (0..area.width)
            .map(|x| {
                buf.cell((x, 1))
                    .map_or(' ', |c| c.symbol().chars().next().unwrap_or(' '))
            })
            .collect();
        // Should NOT show the old placeholder "Type a message..."
        assert!(
            !text_row.contains("Type a message"),
            "Should not show old placeholder text"
        );
    }
}
