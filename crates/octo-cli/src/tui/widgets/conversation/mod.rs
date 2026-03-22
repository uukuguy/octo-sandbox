//! Conversation display widget — renders ChatMessage history.
//!
//! Adapted from opendev-tui conversation widget with full rendering quality:
//! role-colored prefixes, markdown rendering, tool call formatting with
//! verb(arg) pattern, diff highlighting, system-reminder filtering,
//! collapsible tool results, thinking traces, and scroll support.
//!
//! Uses `ChatMessage` from octo-types directly (zero adaptation layer).

mod spinner;
mod tool_format;

use std::borrow::Cow;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget, Widget, Wrap},
};

use octo_types::message::{ChatMessage, ContentBlock, MessageRole};

use crate::tui::formatters::base::strip_system_reminders;
use crate::tui::formatters::diff::{
    is_diff_tool, parse_unified_diff, render_diff_entries,
};
use crate::tui::formatters::formatter_registry::ToolFormatterRegistry;
use crate::tui::formatters::markdown::MarkdownRenderer;
use crate::tui::formatters::style_tokens::{self, Indent};
use crate::tui::widgets::spinner::{COMPLETED_CHAR, SPINNER_FRAMES};

pub use spinner::ActiveTool;

/// Collapse state for tool results.
pub struct ToolCollapseState<'a> {
    /// Whether tools are collapsed by default.
    pub default_collapsed: bool,
    /// Per-tool override map: tool_use_id -> expanded.
    pub overrides: &'a std::collections::HashMap<String, bool>,
}

impl<'a> ToolCollapseState<'a> {
    fn is_collapsed(&self, tool_use_id: &str) -> bool {
        match self.overrides.get(tool_use_id) {
            Some(expanded) => !expanded,
            None => self.default_collapsed,
        }
    }
}

/// Widget that renders the conversation log.
pub struct ConversationWidget<'a> {
    messages: &'a [ChatMessage],
    scroll_offset: u16,
    /// Active tool executions (rendered as spinners below messages).
    active_tools: &'a [ActiveTool],
    spinner_char: char,
    /// Optional formatter registry for tool-specific output rendering.
    formatter_registry: Option<&'a ToolFormatterRegistry>,
    /// Optional tool collapse state.
    collapse_state: Option<ToolCollapseState<'a>>,
}

impl<'a> ConversationWidget<'a> {
    pub fn new(messages: &'a [ChatMessage], scroll_offset: u16) -> Self {
        Self {
            messages,
            scroll_offset,
            active_tools: &[],
            spinner_char: SPINNER_FRAMES[0],
            formatter_registry: None,
            collapse_state: None,
        }
    }

    pub fn formatter_registry(mut self, registry: &'a ToolFormatterRegistry) -> Self {
        self.formatter_registry = Some(registry);
        self
    }

    pub fn collapse_state(mut self, state: ToolCollapseState<'a>) -> Self {
        self.collapse_state = Some(state);
        self
    }

    pub fn active_tools(mut self, tools: &'a [ActiveTool], spinner_char: char) -> Self {
        self.active_tools = tools;
        self.spinner_char = spinner_char;
        self
    }

    /// Build styled lines from messages.
    fn build_lines(&self) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::new();

        for (msg_idx, msg) in self.messages.iter().enumerate() {
            match msg.role {
                MessageRole::User => {
                    self.build_user_lines(msg, &mut lines);
                }
                MessageRole::Assistant => {
                    self.build_assistant_lines(msg, &mut lines);
                }
                MessageRole::System => {
                    // System messages are the agent's system prompt — not useful to display.
                    // Skip them entirely in the conversation view.
                }
            }

            // Blank line between messages
            if msg_idx + 1 < self.messages.len() {
                lines.push(Line::from(""));
            }
        }

        lines
    }

    /// Build spinner lines separately from message content.
    /// Rendered outside the scrollable area so 60ms tick animation
    /// doesn't shift scroll math or cause jitter.
    fn build_spinner_lines(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        let active: Vec<_> = self.active_tools.iter().collect();
        if active.is_empty() {
            return lines;
        }

        for tool in &active {
            lines.push(spinner::render_active_tool(tool, self.spinner_char));
        }

        lines
    }

    /// Render user message lines with `> ` prefix.
    /// Also handles ToolResult blocks (Anthropic API stores them in User role).
    fn build_user_lines(&self, msg: &ChatMessage, lines: &mut Vec<Line<'static>>) {
        let user_style = Style::default()
            .fg(style_tokens::BLUE_BRIGHT)
            .add_modifier(Modifier::BOLD);

        for block in &msg.content {
            match block {
                ContentBlock::Text { text } => {
                    let cleaned = strip_system_reminders(text);
                    if cleaned.is_empty() {
                        continue;
                    }
                    for (i, line) in cleaned.lines().enumerate() {
                        if i == 0 {
                            lines.push(Line::from(vec![
                                Span::styled("> ", user_style),
                                Span::styled(
                                    line.to_string(),
                                    Style::default().fg(style_tokens::PRIMARY),
                                ),
                            ]));
                        } else {
                            lines.push(Line::from(vec![
                                Span::raw("  "),
                                Span::styled(
                                    line.to_string(),
                                    Style::default().fg(style_tokens::PRIMARY),
                                ),
                            ]));
                        }
                    }
                }
                ContentBlock::ToolResult {
                    content, is_error, tool_use_id, ..
                } => {
                    // Check if this tool result should be collapsed
                    if let Some(ref cs) = self.collapse_state {
                        if cs.is_collapsed(tool_use_id) {
                            let tool_name = self.find_tool_name(tool_use_id);
                            let name = tool_name.as_deref().unwrap_or("tool");
                            if let Some(registry) = self.formatter_registry {
                                lines.push(registry.format_collapsed(name, content));
                            } else {
                                let line_count = content.lines().count();
                                lines.push(Line::from(vec![
                                    Span::styled(
                                        "  \u{25B6} ",
                                        Style::default().fg(style_tokens::ACCENT),
                                    ),
                                    Span::styled(
                                        format!("\u{2699} {name} \u{2713} \u{2014} {line_count} lines "),
                                        Style::default().fg(style_tokens::GREY),
                                    ),
                                    Span::styled(
                                        "(Ctrl+O cycle | Ctrl+Shift+O all)",
                                        Style::default()
                                            .fg(style_tokens::SUBTLE)
                                            .add_modifier(Modifier::DIM),
                                    ),
                                ]));
                            }
                            continue;
                        }
                    }

                    // Expanded: full rendering
                    let tool_name = self.find_tool_name(tool_use_id);
                    self.build_tool_result_lines(
                        content,
                        *is_error,
                        tool_name.as_deref(),
                        lines,
                    );
                }
                _ => {} // ToolUse in User messages — skip
            }
        }
    }

    /// Render assistant message with markdown and tool calls.
    fn build_assistant_lines(&self, msg: &ChatMessage, lines: &mut Vec<Line<'static>>) {
        let blocks = &msg.content;
        for (block_idx, block) in blocks.iter().enumerate() {
            match block {
                ContentBlock::Text { text } => {
                    let cleaned = strip_system_reminders(text);
                    if cleaned.is_empty() {
                        continue;
                    }

                    // Determine if this is an intermediate monologue (followed by ToolUse)
                    // or a final response (last text block with no ToolUse after it)
                    let has_tool_after = blocks[block_idx + 1..]
                        .iter()
                        .any(|b| matches!(b, ContentBlock::ToolUse { .. }));

                    if has_tool_after {
                        // Intermediate monologue: dim italic style, no markdown
                        let prefix_char = "\u{25CB}"; // ○ hollow circle
                        for (i, line) in cleaned.lines().enumerate() {
                            let prefix = if i == 0 {
                                format!("{prefix_char} ")
                            } else {
                                Indent::CONT.to_string()
                            };
                            lines.push(Line::from(vec![
                                Span::styled(
                                    prefix,
                                    Style::default().fg(style_tokens::SUBTLE),
                                ),
                                Span::styled(
                                    line.to_string(),
                                    Style::default()
                                        .fg(style_tokens::GREY)
                                        .add_modifier(Modifier::ITALIC),
                                ),
                            ]));
                        }
                        continue;
                    }

                    // Final response: full markdown rendering with ⏺ marker
                    let md_lines = MarkdownRenderer::render(&cleaned);
                    let mut leading_consumed = false;
                    for md_line in md_lines.into_iter() {
                        let line_text: String = md_line
                            .spans
                            .iter()
                            .map(|s| s.content.to_string())
                            .collect();
                        let has_content = !line_text.trim().is_empty();

                        if !leading_consumed && has_content {
                            // First non-empty line gets ⏺ leading marker (green)
                            let mut spans = vec![Span::styled(
                                format!("{COMPLETED_CHAR} "),
                                Style::default().fg(style_tokens::GREEN_BRIGHT),
                            )];
                            spans.extend(md_line.spans);
                            lines.push(Line::from(spans));
                            leading_consumed = true;
                        } else {
                            let mut spans = vec![Span::raw(Indent::CONT)];
                            spans.extend(md_line.spans);
                            lines.push(Line::from(spans));
                        }
                    }
                }
                ContentBlock::ToolUse { name, input, .. } => {
                    let tool_line = tool_format::format_tool_use(name, input);
                    lines.push(tool_line);
                }
                ContentBlock::ToolResult {
                    content, is_error, tool_use_id, ..
                } => {
                    // Check if this tool result should be collapsed
                    if let Some(ref cs) = self.collapse_state {
                        if cs.is_collapsed(tool_use_id) {
                            let tool_name = self.find_tool_name(tool_use_id);
                            let name = tool_name.as_deref().unwrap_or("tool");
                            let line_count = content.lines().count();
                            if let Some(registry) = self.formatter_registry {
                                lines.push(registry.format_collapsed(name, content));
                            } else {
                                lines.push(Line::from(vec![
                                    Span::styled(
                                        "  \u{25B6} ",
                                        Style::default().fg(style_tokens::ACCENT),
                                    ),
                                    Span::styled(
                                        format!("\u{2699} {name} \u{2713} \u{2014} {line_count} lines "),
                                        Style::default().fg(style_tokens::GREY),
                                    ),
                                    Span::styled(
                                        "(Ctrl+O cycle | Ctrl+Shift+O all)",
                                        Style::default()
                                            .fg(style_tokens::SUBTLE)
                                            .add_modifier(Modifier::DIM),
                                    ),
                                ]));
                            }
                            continue;
                        }
                    }

                    // Expanded: full rendering
                    let tool_name = self.find_tool_name(tool_use_id);
                    self.build_tool_result_lines(
                        content,
                        *is_error,
                        tool_name.as_deref(),
                        lines,
                    );
                }
                _ => {} // Image, Document — future
            }
        }
    }

    /// Render system message with ⚙ prefix, muted style.
    #[allow(dead_code)]
    fn build_system_lines(&self, msg: &ChatMessage, lines: &mut Vec<Line<'static>>) {
        let system_style = Style::default()
            .fg(style_tokens::GREY)
            .add_modifier(Modifier::ITALIC);

        for block in &msg.content {
            if let ContentBlock::Text { text } = block {
                let cleaned = strip_system_reminders(text);
                if cleaned.is_empty() {
                    continue;
                }
                for (i, line) in cleaned.lines().enumerate() {
                    if i == 0 {
                        lines.push(Line::from(vec![
                            Span::styled("! ", system_style),
                            Span::styled(line.to_string(), system_style),
                        ]));
                    } else {
                        lines.push(Line::from(vec![
                            Span::raw("  "),
                            Span::styled(line.to_string(), system_style),
                        ]));
                    }
                }
            }
        }
    }

    /// Build tool result lines with diff detection, formatter registry, and collapsible display.
    /// Expanded results are wrapped with subtle separator lines for visual clarity.
    fn build_tool_result_lines(
        &self,
        content: &str,
        is_error: bool,
        tool_name: Option<&str>,
        lines: &mut Vec<Line<'static>>,
    ) {
        if content.is_empty() {
            return;
        }

        let continuation = style_tokens::CONTINUATION_CHAR;

        // Top separator: ╭─ tool output ─────
        let name_label = tool_name.unwrap_or("output");
        let sep_text = format!("  \u{256D}\u{2500} {name_label} \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}");
        lines.push(Line::from(Span::styled(
            sep_text,
            Style::default().fg(style_tokens::BORDER),
        )));

        // Check if this is a diff tool result
        let use_diff = tool_name.is_some_and(|n| is_diff_tool(n));

        if use_diff {
            let result_lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
            let (summary, entries) = parse_unified_diff(&result_lines);
            if !summary.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("  \u{2502} "),
                        Style::default().fg(style_tokens::BORDER),
                    ),
                    Span::styled(summary, Style::default().fg(style_tokens::SUBTLE)),
                ]));
            }
            render_diff_entries(&entries, lines);
        } else if let Some(registry) = self.formatter_registry {
            // Use formatter registry for tool-specific rendering
            let formatted = registry.format(name_label, content);
            lines.push(formatted.header);
            lines.extend(formatted.body);
            if let Some(footer) = formatted.footer {
                lines.push(footer);
            }
        } else {
            // Fallback: generic rendering without registry
            let color = if is_error {
                style_tokens::ERROR
            } else {
                style_tokens::SUBTLE
            };

            let result_lines: Vec<&str> = content.lines().collect();
            let max_display = 20;
            let truncated = result_lines.len() > max_display;
            let display_lines = if truncated {
                &result_lines[..max_display]
            } else {
                &result_lines[..]
            };

            for (i, line) in display_lines.iter().enumerate() {
                let prefix: Cow<'static, str> = if i == 0 {
                    format!("  {continuation}  ").into()
                } else {
                    Cow::Borrowed(Indent::RESULT_CONT)
                };
                lines.push(Line::from(vec![
                    Span::styled(prefix, Style::default().fg(style_tokens::SUBTLE)),
                    Span::styled((*line).to_string(), Style::default().fg(color)),
                ]));
            }

            if truncated {
                let remaining = result_lines.len() - max_display;
                lines.push(Line::from(Span::styled(
                    format!("  {continuation}  ({remaining} more lines)"),
                    Style::default().fg(style_tokens::GREY),
                )));
            }
        }

        // Bottom separator: ╰──────────────
        lines.push(Line::from(Span::styled(
            "  \u{2570}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
            Style::default().fg(style_tokens::BORDER),
        )));
    }

    /// Find the tool name for a given tool_use_id by searching back through messages.
    fn find_tool_name(&self, tool_use_id: &str) -> Option<String> {
        for msg in self.messages {
            for block in &msg.content {
                if let ContentBlock::ToolUse { id, name, .. } = block {
                    if id == tool_use_id {
                        return Some(name.clone());
                    }
                }
            }
        }
        None
    }
}

impl Widget for ConversationWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 2 || area.width == 0 {
            return;
        }

        // Build spinner lines separately so 60ms tick animation doesn't
        // shift scroll math or cause the gap to jitter.
        let spinner_lines = self.build_spinner_lines();
        let spinner_height = spinner_lines.len() as u16;

        // Reserve bottom rows: 1 gap row + spinner rows (if any).
        let reserved = 1 + spinner_height;
        let content_height = area.height.saturating_sub(reserved);
        if content_height == 0 {
            return;
        }

        let content_area = Rect {
            height: content_height,
            width: area.width.saturating_sub(1), // leave room for scrollbar
            ..area
        };

        let lines = self.build_lines();

        let paragraph = Paragraph::new(lines.clone()).wrap(Wrap { trim: false });

        // Use ratatui's own line_count() for accurate wrapped line total
        let total_lines = paragraph.line_count(content_area.width);
        let viewport_height = content_area.height as usize;
        let max_scroll = total_lines.saturating_sub(viewport_height);

        // scroll_offset = lines from bottom; convert to lines from top for ratatui
        let clamped = (self.scroll_offset as usize).min(max_scroll);
        let actual_scroll = max_scroll.saturating_sub(clamped);

        paragraph
            .scroll((actual_scroll as u16, 0))
            .render(content_area, buf);

        // Extend diff background colors to fill entire row width.
        for y in content_area.y..content_area.y.saturating_add(content_area.height) {
            let mut diff_bg = None;
            for x in content_area.x..content_area.x.saturating_add(content_area.width) {
                if let Some(cell) = buf.cell(ratatui::layout::Position::new(x, y)) {
                    if cell.bg == style_tokens::DIFF_ADD_BG
                        || cell.bg == style_tokens::DIFF_DEL_BG
                    {
                        diff_bg = Some(cell.bg);
                        break;
                    }
                }
            }
            if let Some(bg) = diff_bg {
                for x in content_area.x..content_area.x.saturating_add(content_area.width) {
                    if let Some(cell) = buf.cell_mut(ratatui::layout::Position::new(x, y)) {
                        cell.set_bg(bg);
                    }
                }
            }
        }

        // Render spinner lines below the scroll area.
        if spinner_height > 0 {
            let last_content_row = (content_area.y
                ..content_area.y.saturating_add(content_area.height))
                .rev()
                .find(|&y| {
                    (content_area.x..content_area.x.saturating_add(content_area.width)).any(|x| {
                        buf.cell(ratatui::layout::Position::new(x, y))
                            .is_some_and(|c| c.symbol() != " ")
                    })
                });

            let spinner_y = match last_content_row {
                Some(y) => y + 2, // 1 blank line gap
                None => content_area.y,
            };

            for (i, line) in spinner_lines.iter().enumerate() {
                let y = spinner_y + i as u16;
                if y < area.bottom() {
                    buf.set_line(area.x, y, line, area.width);
                }
            }
        }

        // Visual scrollbar when content overflows
        if max_scroll > 0 {
            let mut scrollbar_state = ScrollbarState::new(max_scroll)
                .position(actual_scroll)
                .viewport_content_length(viewport_height);
            StatefulWidget::render(
                Scrollbar::new(ScrollbarOrientation::VerticalRight),
                area,
                buf,
                &mut scrollbar_state,
            );
        }
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
    fn test_build_lines_empty() {
        let widget = ConversationWidget::new(&[], 0);
        assert!(widget.build_lines().is_empty());
    }

    #[test]
    fn test_build_lines_user_message() {
        let messages = vec![ChatMessage::user("Hello world")];
        let widget = ConversationWidget::new(&messages, 0);
        let lines = widget.build_lines();
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        assert!(text.contains("> "), "Should have user prefix");
        assert!(text.contains("Hello world"));
    }

    #[test]
    fn test_build_lines_assistant_markdown() {
        let messages = vec![ChatMessage::assistant("**bold** text")];
        let widget = ConversationWidget::new(&messages, 0);
        let lines = widget.build_lines();
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        // Should have ⏺ marker
        assert!(text.contains('\u{23fa}'), "Should have ⏺ marker");
        assert!(text.contains("bold"));
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
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        assert!(text.contains('\u{25B8}'), "Should have ▸ tool call marker");
    }

    #[test]
    fn test_build_lines_tool_result() {
        let messages = vec![ChatMessage {
            role: MessageRole::Assistant,
            content: vec![
                ContentBlock::ToolUse {
                    id: "t1".into(),
                    name: "bash".into(),
                    input: serde_json::json!({"command": "ls"}),
                },
                ContentBlock::ToolResult {
                    tool_use_id: "t1".into(),
                    content: "file1.rs\nfile2.rs".into(),
                    is_error: false,
                },
            ],
        }];
        let widget = ConversationWidget::new(&messages, 0);
        let lines = widget.build_lines();
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        assert!(text.contains("file1.rs"));
        assert!(text.contains("file2.rs"));
        // Should have continuation char
        assert!(text.contains('\u{23bf}'), "Should have ⎿ continuation");
    }

    #[test]
    fn test_system_reminder_filtered() {
        let messages = vec![ChatMessage::assistant(
            "Hello<system-reminder>secret</system-reminder> world",
        )];
        let widget = ConversationWidget::new(&messages, 0);
        let lines = widget.build_lines();
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        assert!(!text.contains("secret"));
        assert!(text.contains("Hello"));
        assert!(text.contains("world"));
    }

    #[test]
    fn test_tool_result_truncation() {
        let long_output: String = (0..30)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let messages = vec![ChatMessage {
            role: MessageRole::Assistant,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: "t1".into(),
                content: long_output,
                is_error: false,
            }],
        }];
        let widget = ConversationWidget::new(&messages, 0);
        let lines = widget.build_lines();
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        assert!(text.contains("more lines"));
    }

    #[test]
    fn test_diff_rendering() {
        let diff_content = "Edited file.rs: 1 replacement(s), 1 addition(s) and 1 removal(s)\n--- a/file.rs\n+++ b/file.rs\n@@ -10,3 +10,3 @@\n context\n-old\n+new";
        let messages = vec![ChatMessage {
            role: MessageRole::Assistant,
            content: vec![
                ContentBlock::ToolUse {
                    id: "t1".into(),
                    name: "edit_file".into(),
                    input: serde_json::json!({}),
                },
                ContentBlock::ToolResult {
                    tool_use_id: "t1".into(),
                    content: diff_content.into(),
                    is_error: false,
                },
            ],
        }];
        let widget = ConversationWidget::new(&messages, 0);
        let lines = widget.build_lines();
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        // Should contain line numbers and reformatted summary
        assert!(text.contains("10"), "Should contain line number");
        assert!(text.contains("+ new"), "Should contain '+ new'");
        assert!(text.contains("- old"), "Should contain '- old'");
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

    #[test]
    fn test_spinner_active_tools() {
        let messages = vec![ChatMessage::user("Hello")];
        let tools = vec![ActiveTool {
            tool_id: "test-id".into(),
            name: "bash".into(),
            args: serde_json::json!({"command": "ls -la"}),
            started_at: std::time::Instant::now(),
        }];
        let widget = ConversationWidget::new(&messages, 0)
            .active_tools(&tools, SPINNER_FRAMES[0]);
        let spinner = widget.build_spinner_lines();
        let text: String = spinner
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        assert!(text.contains("(0s)"), "Should show elapsed time");
    }

    #[test]
    fn test_no_spinner_when_idle() {
        let messages = vec![ChatMessage::user("Hello")];
        let widget = ConversationWidget::new(&messages, 0);
        let spinner = widget.build_spinner_lines();
        assert!(spinner.is_empty());
    }

    #[test]
    fn test_render_reserves_bottom_row_gap() {
        let messages = vec![ChatMessage::user("Hello")];
        let widget = ConversationWidget::new(&messages, 0);

        let area = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        // The last row must be blank — reserved gap
        for x in 0..40 {
            let cell = &buf[(x, 9)];
            assert_eq!(cell.symbol(), " ", "Bottom gap row should be blank at column {x}");
        }
    }

}
