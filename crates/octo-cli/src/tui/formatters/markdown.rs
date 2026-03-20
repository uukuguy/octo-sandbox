//! Markdown rendering for terminal output.
//!
//! Converts markdown text to styled ratatui `Line`s with basic formatting:
//! headers, bold, italic, code blocks, and inline code.

use std::borrow::Cow;

use super::style_tokens;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

/// A color palette for markdown rendering.
/// The default uses the standard bright colors; `muted()` produces a
/// subdued palette suitable for thinking/reasoning display.
#[derive(Debug, Clone)]
pub struct MdPalette {
    pub heading: Color,
    pub code_fg: Color,
    pub code_bg: Color,
    pub bullet: Color,
    pub bold_fg: Color,
    pub link: Color,
    pub text: Color,
    /// Extra modifier applied to every span (e.g. `ITALIC` for thinking).
    pub base_modifier: Modifier,
}

impl Default for MdPalette {
    fn default() -> Self {
        Self {
            heading: style_tokens::HEADING_1,
            code_fg: style_tokens::CODE_FG,
            code_bg: style_tokens::CODE_BG,
            bullet: style_tokens::BULLET,
            bold_fg: style_tokens::BOLD_FG,
            link: style_tokens::BLUE_BRIGHT,
            text: style_tokens::PRIMARY,
            base_modifier: Modifier::empty(),
        }
    }
}

impl MdPalette {
    /// Build a muted palette for thinking/reasoning display.
    pub fn muted(base: Color) -> Self {
        let heading = dim_color(style_tokens::HEADING_1, 0.50);
        let code_fg = dim_color(style_tokens::CODE_FG, 0.50);
        let bold_fg = dim_color(style_tokens::BOLD_FG, 0.55);
        let link = dim_color(style_tokens::BLUE_BRIGHT, 0.50);
        Self {
            heading,
            code_fg,
            code_bg: style_tokens::CODE_BG,
            bullet: base,
            bold_fg,
            link,
            text: base,
            base_modifier: Modifier::empty(),
        }
    }
}

/// Dim an RGB color by mixing it toward black. `factor` in 0.0..=1.0.
fn dim_color(color: Color, factor: f32) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(
            (r as f32 * factor) as u8,
            (g as f32 * factor) as u8,
            (b as f32 * factor) as u8,
        ),
        other => other,
    }
}

/// Renders markdown text into styled terminal lines.
pub struct MarkdownRenderer;

impl MarkdownRenderer {
    /// Render markdown text into a vector of styled lines using the default palette.
    pub fn render(text: &str) -> Vec<Line<'static>> {
        Self::render_with_palette(text, &MdPalette::default())
    }

    /// Render markdown with a muted palette (for thinking/reasoning display).
    pub fn render_muted(text: &str, base_color: Color) -> Vec<Line<'static>> {
        Self::render_with_palette(text, &MdPalette::muted(base_color))
    }

    /// Render markdown text with a given color palette.
    pub fn render_with_palette(text: &str, palette: &MdPalette) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let mut in_code_block = false;
        let base_mod = palette.base_modifier;

        for raw_line in text.lines() {
            if raw_line.starts_with("```") {
                in_code_block = !in_code_block;
                if in_code_block {
                    let lang = raw_line.trim_start_matches('`').trim();
                    if !lang.is_empty() {
                        let hint: Cow<'static, str> = Cow::Owned(format!("--- {lang} ---"));
                        lines.push(Line::from(Span::styled(
                            hint,
                            Style::default()
                                .fg(style_tokens::GREY)
                                .add_modifier(base_mod),
                        )));
                    }
                }
                continue;
            }

            if in_code_block {
                let code: Cow<'static, str> = Cow::Owned(raw_line.to_string());
                lines.push(Line::from(Span::styled(
                    code,
                    Style::default()
                        .fg(palette.code_fg)
                        .bg(palette.code_bg)
                        .add_modifier(base_mod),
                )));
                continue;
            }

            // Headers
            if let Some(header) = raw_line.strip_prefix("### ") {
                let h: Cow<'static, str> = Cow::Owned(header.to_string());
                lines.push(Line::from(Span::styled(
                    h,
                    Style::default()
                        .fg(palette.heading)
                        .add_modifier(Modifier::BOLD | base_mod),
                )));
            } else if let Some(header) = raw_line.strip_prefix("## ") {
                let h: Cow<'static, str> = Cow::Owned(header.to_string());
                lines.push(Line::from(Span::styled(
                    h,
                    Style::default()
                        .fg(palette.heading)
                        .add_modifier(Modifier::BOLD | base_mod),
                )));
            } else if let Some(header) = raw_line.strip_prefix("# ") {
                let h: Cow<'static, str> = Cow::Owned(header.to_string());
                lines.push(Line::from(Span::styled(
                    h,
                    Style::default()
                        .fg(palette.heading)
                        .add_modifier(Modifier::BOLD | base_mod),
                )));
            } else if is_bullet_line(raw_line) {
                let trimmed = raw_line.trim_start();
                let indent_len = raw_line.len() - trimmed.len();
                let indent_level = indent_len / 2;
                let content = &trimmed[2..];
                let prefix: Cow<'static, str> = if indent_level == 0 {
                    Cow::Borrowed("  - ")
                } else {
                    Cow::Owned(format!("{}  - ", "  ".repeat(indent_level)))
                };
                let mut spans = vec![Span::styled(
                    prefix,
                    Style::default().fg(palette.bullet).add_modifier(base_mod),
                )];
                spans.extend(parse_inline_spans_with_palette(content, palette));
                lines.push(Line::from(spans));
            } else if is_ordered_list_line(raw_line) {
                let trimmed = raw_line.trim_start();
                let indent_len = raw_line.len() - trimmed.len();
                let indent_level = indent_len / 2;
                let dot_pos = trimmed.find(". ").unwrap();
                let number = &trimmed[..dot_pos];
                let content = &trimmed[dot_pos + 2..];
                let prefix: Cow<'static, str> =
                    Cow::Owned(format!("{}  {}. ", "  ".repeat(indent_level), number));
                let mut spans = vec![Span::styled(
                    prefix,
                    Style::default().fg(palette.bullet).add_modifier(base_mod),
                )];
                spans.extend(parse_inline_spans_with_palette(content, palette));
                lines.push(Line::from(spans));
            } else {
                lines.push(render_inline_line_with_palette(raw_line, palette));
            }
        }

        lines
    }
}

fn render_inline_line_with_palette(text: &str, palette: &MdPalette) -> Line<'static> {
    let spans = parse_inline_spans_with_palette(text, palette);
    Line::from(spans)
}

fn is_bullet_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ")
}

fn is_ordered_list_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    if let Some(dot_pos) = trimmed.find(". ") {
        dot_pos > 0 && trimmed[..dot_pos].chars().all(|c| c.is_ascii_digit())
    } else {
        false
    }
}

fn find_markdown_link(text: &str) -> Option<(usize, &str, &str, usize)> {
    let open_bracket = text.find('[')?;
    let after_bracket = &text[open_bracket + 1..];
    let close_bracket = after_bracket.find(']')?;
    let link_text = &after_bracket[..close_bracket];

    let after_close = &after_bracket[close_bracket + 1..];
    if !after_close.starts_with('(') {
        return None;
    }
    let after_paren = &after_close[1..];
    let close_paren = after_paren.find(')')?;
    let url = &after_paren[..close_paren];

    let end = open_bracket + 1 + close_bracket + 1 + 1 + close_paren + 1;
    Some((open_bracket, link_text, url, end))
}

fn parse_inline_spans_with_palette(text: &str, palette: &MdPalette) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut remaining = text;
    let base_mod = palette.base_modifier;

    while !remaining.is_empty() {
        let next_backtick = remaining.find('`');
        let next_link = find_markdown_link(remaining);

        let use_link = match (next_backtick, &next_link) {
            (_, None) => false,
            (None, Some(_)) => true,
            (Some(bt), Some((ls, _, _, _))) => *ls < bt,
        };

        if use_link {
            let (link_start, link_text, _url, link_end) = next_link.unwrap();
            if link_start > 0 {
                spans.extend(parse_bold_spans_with_palette(
                    &remaining[..link_start],
                    palette,
                ));
            }
            let display: Cow<'static, str> = Cow::Owned(link_text.to_string());
            spans.push(Span::styled(
                display,
                Style::default().fg(palette.link).add_modifier(base_mod),
            ));
            remaining = &remaining[link_end..];
        } else if let Some(code_start) = next_backtick {
            if code_start > 0 {
                spans.extend(parse_bold_spans_with_palette(
                    &remaining[..code_start],
                    palette,
                ));
            }
            let after_start = &remaining[code_start + 1..];
            if let Some(code_end) = after_start.find('`') {
                let code: Cow<'static, str> = Cow::Owned(after_start[..code_end].to_string());
                spans.push(Span::styled(
                    code,
                    Style::default()
                        .fg(palette.code_fg)
                        .bg(palette.code_bg)
                        .add_modifier(base_mod),
                ));
                remaining = &after_start[code_end + 1..];
            } else {
                spans.extend(parse_bold_spans_with_palette(remaining, palette));
                break;
            }
        } else {
            spans.extend(parse_bold_spans_with_palette(remaining, palette));
            break;
        }
    }

    if spans.is_empty() {
        spans.push(Span::styled(
            Cow::Owned(String::new()),
            Style::default().add_modifier(base_mod),
        ));
    }

    spans
}

fn parse_bold_spans_with_palette(text: &str, palette: &MdPalette) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut remaining = text;
    let base_mod = palette.base_modifier;

    while !remaining.is_empty() {
        if let Some(bold_start) = remaining.find("**") {
            if bold_start > 0 {
                let plain: Cow<'static, str> = Cow::Owned(remaining[..bold_start].to_string());
                spans.push(Span::styled(
                    plain,
                    Style::default().fg(palette.text).add_modifier(base_mod),
                ));
            }
            let after_start = &remaining[bold_start + 2..];
            if let Some(bold_end) = after_start.find("**") {
                let bold_text: Cow<'static, str> = Cow::Owned(after_start[..bold_end].to_string());
                spans.push(Span::styled(
                    bold_text,
                    Style::default()
                        .fg(palette.bold_fg)
                        .add_modifier(Modifier::BOLD | base_mod),
                ));
                remaining = &after_start[bold_end + 2..];
            } else {
                let rest: Cow<'static, str> = Cow::Owned(remaining.to_string());
                spans.push(Span::styled(
                    rest,
                    Style::default().fg(palette.text).add_modifier(base_mod),
                ));
                break;
            }
        } else {
            let rest: Cow<'static, str> = Cow::Owned(remaining.to_string());
            spans.push(Span::styled(
                rest,
                Style::default().fg(palette.text).add_modifier(base_mod),
            ));
            break;
        }
    }

    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text() {
        let lines = MarkdownRenderer::render("Hello world");
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_headers() {
        let lines = MarkdownRenderer::render("# Title\n## Subtitle\n### Section");
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_code_block() {
        let md = "```rust\nfn main() {}\n```";
        let lines = MarkdownRenderer::render(md);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_bullet_list() {
        let md = "- item one\n- item two";
        let lines = MarkdownRenderer::render(md);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_nested_bullets() {
        let md = "- top\n  - nested\n    - deep";
        let lines = MarkdownRenderer::render(md);
        assert_eq!(lines.len(), 3);
        let first: String = lines[0]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert!(first.starts_with("  - "));
    }

    #[test]
    fn test_ordered_list() {
        let md = "1. first\n2. second\n3. third";
        let lines = MarkdownRenderer::render(md);
        assert_eq!(lines.len(), 3);
        let first: String = lines[0]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert!(first.contains("1. "));
    }

    #[test]
    fn test_inline_code() {
        let spans = parse_inline_spans_with_palette("use `tokio` for async", &MdPalette::default());
        assert!(spans.len() >= 3);
    }

    #[test]
    fn test_bold_text() {
        let spans =
            parse_bold_spans_with_palette("this is **bold** text", &MdPalette::default());
        assert!(spans.len() >= 3);
    }

    #[test]
    fn test_markdown_link() {
        let spans = parse_inline_spans_with_palette(
            "visit [example](http://example.com) now",
            &MdPalette::default(),
        );
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "visit example now");
    }

    #[test]
    fn test_muted_palette() {
        let lines = MarkdownRenderer::render_muted("# Hello\nsome text", style_tokens::THINKING_BG);
        assert_eq!(lines.len(), 2);
    }
}
