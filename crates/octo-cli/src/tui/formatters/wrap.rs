//! Manual word-wrapping utility that preserves per-line prefix spans.
//!
//! Unlike ratatui's `Wrap { trim: false }`, this pre-wraps lines so each
//! visual line already contains the correct indentation prefix. This makes
//! ratatui's wrapping a no-op and gives us full control over continuation
//! indentation.

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;

use super::style_tokens::CODE_BG;

/// Returns true if any span in `line` has a background color matching `CODE_BG`,
/// indicating this line is inside a code block and should not be word-wrapped.
fn is_code_line(line: &Line<'_>) -> bool {
    line.spans
        .iter()
        .any(|s| s.style.bg.is_some_and(|bg| bg == CODE_BG))
}

/// Compute the display width of a span's content using unicode widths.
fn span_width(s: &Span<'_>) -> usize {
    s.content
        .chars()
        .map(|c| UnicodeWidthChar::width(c).unwrap_or(0))
        .sum()
}

/// Compute the total display width of a slice of spans.
fn spans_width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(|s| span_width(s)).sum()
}

/// Split a markdown line's spans into structural prefix (bullet/list marker)
/// and content. Strips redundant leading whitespace from the prefix since the
/// outer `cont_prefix` already provides base indentation.
///
/// Returns `(stripped_prefix_spans, content_spans, stripped_prefix_width)`.
fn split_structural_prefix<'a>(
    spans: &[Span<'a>],
    strip_indent: usize,
) -> (Vec<Span<'a>>, Vec<Span<'a>>, usize) {
    if spans.is_empty() {
        return (vec![], vec![], 0);
    }
    let first_text = spans[0].content.as_ref();
    let trimmed = first_text.trim_start();

    let is_bullet =
        trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ");
    let is_ordered = !is_bullet
        && trimmed.find(". ").is_some_and(|dot_pos| {
            dot_pos > 0 && trimmed[..dot_pos].chars().all(|c| c.is_ascii_digit())
        });

    if is_bullet || is_ordered {
        // Strip up to `strip_indent` chars of leading whitespace
        let leading_ws = first_text.len() - trimmed.len();
        let strip = leading_ws.min(strip_indent);
        let stripped_text = &first_text[strip..];
        let stripped_span = Span::styled(stripped_text.to_string(), spans[0].style);
        let prefix_w: usize = stripped_text
            .chars()
            .map(|c| UnicodeWidthChar::width(c).unwrap_or(0))
            .sum();
        (vec![stripped_span], spans[1..].to_vec(), prefix_w)
    } else {
        (vec![], spans.to_vec(), 0)
    }
}

/// Pre-wrap a sequence of markdown-rendered lines, prepending the appropriate
/// prefix to each output line.
///
/// - `md_lines`: the lines produced by `MarkdownRenderer::render()` (or `render_muted()`)
/// - `first_prefix`: spans to prepend to the very first non-empty content line
/// - `cont_prefix`: spans to prepend to all other lines (continuations + blank)
/// - `max_width`: the maximum display width (typically `terminal_width - 1`)
///
/// Lines whose spans contain a `CODE_BG` background are passed through without
/// wrapping — code blocks should be truncated, not reflowed.
pub fn wrap_spans_to_lines<'a>(
    md_lines: Vec<Line<'a>>,
    first_prefix: Vec<Span<'a>>,
    cont_prefix: Vec<Span<'a>>,
    max_width: usize,
) -> Vec<Line<'a>> {
    if max_width == 0 {
        return Vec::new();
    }

    let first_prefix_w = spans_width(&first_prefix);
    let cont_prefix_w = spans_width(&cont_prefix);
    let mut output: Vec<Line<'a>> = Vec::new();
    let mut leading_consumed = false;

    for md_line in md_lines {
        // Check if this line has visible content
        let line_text: String = md_line.spans.iter().map(|s| s.content.as_ref()).collect();
        let has_content = !line_text.trim().is_empty();

        // Determine which prefix to use
        let (prefix, prefix_w) = if !leading_consumed && has_content {
            leading_consumed = true;
            (first_prefix.clone(), first_prefix_w)
        } else {
            (cont_prefix.clone(), cont_prefix_w)
        };

        // Code lines: pass through without wrapping
        if is_code_line(&md_line) {
            let mut spans = prefix;
            spans.extend(md_line.spans);
            output.push(Line::from(spans));
            continue;
        }

        // Empty/blank lines: just prefix
        if !has_content {
            output.push(Line::from(prefix));
            continue;
        }

        // Split structural prefix (bullet/list marker) from content,
        // stripping redundant leading whitespace that the outer prefix provides
        let (struct_prefix, content_spans, struct_prefix_w) =
            split_structural_prefix(&md_line.spans, cont_prefix_w);

        // Available width for content after both outer prefix and structural prefix
        let content_avail = max_width.saturating_sub(prefix_w + struct_prefix_w).max(1);

        // Wrap only the content spans
        let wrapped = if content_spans.is_empty() {
            vec![vec![]]
        } else {
            wrap_spans(content_spans, content_avail)
        };

        for (i, chunk) in wrapped.into_iter().enumerate() {
            let mut spans = if i == 0 {
                // First visual line: outer_prefix + stripped_bullet + content
                let mut s = prefix.clone();
                s.extend(struct_prefix.clone());
                s
            } else if struct_prefix_w > 0 {
                // Continuation of a bullet: pad to align with content start
                vec![Span::raw(" ".repeat(cont_prefix_w + struct_prefix_w))]
            } else {
                // No bullet: use normal continuation prefix
                cont_prefix.clone()
            };
            spans.extend(chunk);
            output.push(Line::from(spans));
        }
    }

    output
}

/// Word-wrap a sequence of styled spans to fit within `max_width` display columns.
///
/// Returns a `Vec<Vec<Span>>` where each inner vec represents one visual line.
/// Breaks at word boundaries (spaces) when possible; falls back to mid-word
/// breaks when a single word exceeds `max_width`.
fn wrap_spans(spans: Vec<Span<'_>>, max_width: usize) -> Vec<Vec<Span<'_>>> {
    if max_width == 0 {
        return vec![spans];
    }

    let mut result: Vec<Vec<Span>> = Vec::new();
    let mut current_line: Vec<Span> = Vec::new();
    let mut line_width: usize = 0;

    // Split all spans into word-level tokens preserving styles
    let tokens = tokenize_spans(&spans);

    for token in tokens {
        let token_w = token
            .chars()
            .map(|c| UnicodeWidthChar::width(c).unwrap_or(0))
            .sum::<usize>();

        if line_width + token_w <= max_width {
            // Token fits on current line
            let style = find_style_for_pos(&spans, &token);
            let merged = if let Some(last) = current_line.last_mut() {
                if last.style == style {
                    let mut s = last.content.to_string();
                    s.push_str(&token);
                    *last = Span::styled(s, last.style);
                    true
                } else {
                    false
                }
            } else {
                false
            };
            if !merged {
                current_line.push(Span::styled(token, style));
            }
            line_width += token_w;
        } else if token.trim().is_empty() {
            // It's whitespace that would overflow — start a new line
            if !current_line.is_empty() {
                result.push(std::mem::take(&mut current_line));
            }
            line_width = 0;
        } else if token_w > max_width {
            // Word is wider than max_width — we need to split it
            if !current_line.is_empty() {
                result.push(std::mem::take(&mut current_line));
                line_width = 0;
            }
            // Split the oversized word character by character
            let style = find_style_for_pos(&spans, &token);
            let mut chunk = String::new();
            let mut chunk_w = 0;
            for c in token.chars() {
                let cw = UnicodeWidthChar::width(c).unwrap_or(0);
                if chunk_w + cw > max_width && !chunk.is_empty() {
                    current_line.push(Span::styled(std::mem::take(&mut chunk), style));
                    result.push(std::mem::take(&mut current_line));
                    chunk_w = 0;
                }
                chunk.push(c);
                chunk_w += cw;
            }
            if !chunk.is_empty() {
                current_line.push(Span::styled(chunk, style));
                line_width = chunk_w;
            }
        } else {
            // Word doesn't fit — start a new line
            if !current_line.is_empty() {
                result.push(std::mem::take(&mut current_line));
            }
            let style = find_style_for_pos(&spans, &token);
            current_line.push(Span::styled(token, style));
            line_width = token_w;
        }
    }

    // Flush remaining
    if !current_line.is_empty() {
        result.push(current_line);
    }

    if result.is_empty() {
        result.push(Vec::new());
    }

    result
}

/// Find the style that applies to a given token text by scanning the original spans.
fn find_style_for_pos<'a>(spans: &[Span<'a>], token: &str) -> Style {
    if token.is_empty() {
        return Style::default();
    }
    let first_char = token.chars().next().unwrap();
    let token_first_bytes = &token[..first_char.len_utf8()];

    for span in spans {
        let span_str = span.content.as_ref();
        if span_str.find(token_first_bytes).is_some() {
            return span.style;
        }
    }
    spans.last().map(|s| s.style).unwrap_or_default()
}

/// Tokenize spans into words and whitespace, preserving the original text exactly.
fn tokenize_spans(spans: &[Span<'_>]) -> Vec<String> {
    let full_text: String = spans.iter().map(|s| s.content.as_ref()).collect();
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_space = false;

    for c in full_text.chars() {
        let is_space = c == ' ';
        if is_space != in_space && !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
        current.push(c);
        in_space = is_space;
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_line_no_wrap() {
        let md_lines = vec![Line::from(vec![Span::raw("Hello world")])];
        let first = vec![Span::raw("* ")];
        let cont = vec![Span::raw("  ")];

        let result = wrap_spans_to_lines(md_lines, first, cont, 80);
        assert_eq!(result.len(), 1);
        let text: String = result[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "* Hello world");
    }

    #[test]
    fn test_wraps_long_line() {
        let long = "word ".repeat(20).trim().to_string();
        let md_lines = vec![Line::from(vec![Span::raw(long)])];
        let first = vec![Span::raw("* ")];
        let cont = vec![Span::raw("  ")];

        let result = wrap_spans_to_lines(md_lines, first, cont, 40);
        assert!(result.len() > 1, "should have wrapped into multiple lines");

        assert!(result[0].spans[0].content.as_ref().starts_with("* "));
        for line in &result[1..] {
            assert_eq!(line.spans[0].content.as_ref(), "  ");
        }

        for line in &result {
            let w: usize = line.spans.iter().map(|s| span_width(s)).sum();
            assert!(w <= 40, "line width {w} exceeds max 40");
        }
    }

    #[test]
    fn test_blank_lines_preserved() {
        let md_lines = vec![
            Line::from(vec![Span::raw("Hello")]),
            Line::from(vec![Span::raw("")]),
            Line::from(vec![Span::raw("World")]),
        ];
        let first = vec![Span::raw("* ")];
        let cont = vec![Span::raw("  ")];

        let result = wrap_spans_to_lines(md_lines, first, cont, 80);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_code_line_not_wrapped() {
        let code_style = Style::default().bg(CODE_BG);
        let long_code = "x".repeat(200);
        let md_lines = vec![Line::from(vec![Span::styled(long_code, code_style)])];
        let first = vec![Span::raw("* ")];
        let cont = vec![Span::raw("  ")];

        let result = wrap_spans_to_lines(md_lines, first, cont, 40);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_bullet_indent_stripped() {
        let md_lines = vec![
            Line::from(vec![Span::raw("Header line")]),
            Line::from(vec![Span::raw("  - "), Span::raw("Bullet text here")]),
        ];
        let first = vec![Span::raw("\u{23fa} ")];
        let cont = vec![Span::raw("  ")];

        let result = wrap_spans_to_lines(md_lines, first, cont, 80);
        assert_eq!(result.len(), 2);
        let text: String = result[1].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "  - Bullet text here");
    }
}
