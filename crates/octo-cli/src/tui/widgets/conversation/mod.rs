//! Conversation display widget — renders ChatMessage history.
//!
//! Adapted from opendev-tui conversation widget. Renders octo-types
//! `ChatMessage { role, content: Vec<ContentBlock> }` with markdown,
//! tool call summaries, and scroll support.

mod tool_format;
mod spinner;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget, Widget, Wrap},
};

use octo_types::message::{ChatMessage, ContentBlock, MessageRole};

use crate::tui::formatters::markdown::MarkdownRenderer;
use crate::tui::formatters::style_tokens;

#[allow(unused_imports)]
use crate::tui::formatters::tool_registry;

pub use spinner::ActiveTool;

/// Widget that renders the conversation log.
pub struct ConversationWidget<'a> {
    messages: &'a [ChatMessage],
    scroll_offset: u16,
    /// Total content height for scrollbar calculation.
    content_height: u16,
    /// Active tool executions (rendered as spinners below messages).
    active_tools: &'a [ActiveTool],
    spinner_char: char,
}

impl<'a> ConversationWidget<'a> {
    pub fn new(messages: &'a [ChatMessage], scroll_offset: u16) -> Self {
        Self {
            messages,
            scroll_offset,
            content_height: 0,
            active_tools: &[],
            spinner_char: '\u{280b}',
        }
    }

    pub fn active_tools(mut self, tools: &'a [ActiveTool], spinner_char: char) -> Self {
        self.active_tools = tools;
        self.spinner_char = spinner_char;
        self
    }

    /// Build all lines from messages.
    fn build_lines(&self) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::new();
        for msg in self.messages {
            // Role prefix
            let (prefix, prefix_style) = role_prefix(&msg.role);
            lines.push(Line::from(Span::styled(prefix, prefix_style)));

            // Render each content block
            for block in &msg.content {
                match block {
                    ContentBlock::Text { text } => {
                        let md_lines = MarkdownRenderer::render(text);
                        lines.extend(md_lines);
                    }
                    ContentBlock::ToolUse { name, input, .. } => {
                        lines.push(tool_format::format_tool_call(name, input));
                    }
                    ContentBlock::ToolResult { content, is_error, .. } => {
                        lines.extend(tool_format::format_tool_result(content, *is_error));
                    }
                    _ => {} // Image, Document — future
                }
            }

            // Blank line between messages
            lines.push(Line::from(""));
        }

        // Append active tool spinners
        for tool in self.active_tools {
            lines.push(spinner::render_active_tool(tool, self.spinner_char));
        }

        lines
    }
}

impl Widget for ConversationWidget<'_> {
    fn render(mut self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        let lines = self.build_lines();
        self.content_height = lines.len() as u16;

        let paragraph = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll_offset, 0));

        paragraph.render(area, buf);

        // Scrollbar
        if self.content_height > area.height {
            let mut scrollbar_state = ScrollbarState::new(self.content_height as usize)
                .position(self.scroll_offset as usize)
                .viewport_content_length(area.height as usize);
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
            scrollbar.render(area, buf, &mut scrollbar_state);
        }
    }
}

/// Get role display prefix and style.
fn role_prefix(role: &MessageRole) -> (String, Style) {
    match role {
        MessageRole::User => (
            "\u{276f} You".to_string(),
            Style::default()
                .fg(style_tokens::BLUE_BRIGHT)
                .add_modifier(Modifier::BOLD),
        ),
        MessageRole::Assistant => (
            "\u{25C6} Assistant".to_string(),
            Style::default()
                .fg(style_tokens::ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        MessageRole::System => (
            "\u{2699} System".to_string(),
            Style::default()
                .fg(style_tokens::GREY)
                .add_modifier(Modifier::ITALIC),
        ),
    }
}

/// Calculate total content height for a message list.
pub fn estimate_content_height(messages: &[ChatMessage]) -> u16 {
    let widget = ConversationWidget::new(messages, 0);
    widget.build_lines().len() as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_prefix_user() {
        let (prefix, _) = role_prefix(&MessageRole::User);
        assert!(prefix.contains("You"));
    }

    #[test]
    fn test_role_prefix_assistant() {
        let (prefix, _) = role_prefix(&MessageRole::Assistant);
        assert!(prefix.contains("Assistant"));
    }

    #[test]
    fn test_build_lines_empty() {
        let widget = ConversationWidget::new(&[], 0);
        assert!(widget.build_lines().is_empty());
    }

    #[test]
    fn test_build_lines_text_message() {
        let messages = vec![ChatMessage::user("Hello world")];
        let widget = ConversationWidget::new(&messages, 0);
        let lines = widget.build_lines();
        // Should have: role prefix + text line + blank separator
        assert!(lines.len() >= 3);
    }

    #[test]
    fn test_build_lines_tool_use() {
        let messages = vec![ChatMessage {
            role: MessageRole::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: "t1".into(),
                name: "bash".into(),
                input: serde_json::json!({"command": "ls"}),
            }],
        }];
        let widget = ConversationWidget::new(&messages, 0);
        let lines = widget.build_lines();
        assert!(lines.len() >= 3);
    }

    #[test]
    fn test_estimate_content_height() {
        let messages = vec![
            ChatMessage::user("Hello"),
            ChatMessage::assistant("World"),
        ];
        let height = estimate_content_height(&messages);
        assert!(height > 0);
    }
}
