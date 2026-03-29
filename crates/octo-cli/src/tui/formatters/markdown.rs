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
        let raw_lines: Vec<&str> = text.lines().collect();
        let mut i = 0;

        while i < raw_lines.len() {
            let raw_line = raw_lines[i];

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
                i += 1;
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
                i += 1;
                continue;
            }

            // Table detection: collect consecutive table lines
            if is_table_line(raw_line) {
                let table_start = i;
                while i < raw_lines.len() && is_table_line(raw_lines[i]) {
                    i += 1;
                }
                render_table(&raw_lines[table_start..i], palette, &mut lines);
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
            i += 1;
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

/// Check if a line looks like a markdown table row (`| ... | ... |`).
fn is_table_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.len() >= 3
}

/// Check if a table row is a separator (`|---|---|`).
fn is_separator_row(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.starts_with('|') || !trimmed.ends_with('|') {
        return false;
    }
    trimmed[1..trimmed.len() - 1]
        .split('|')
        .all(|cell| {
            let c = cell.trim();
            !c.is_empty()
                && c.chars()
                    .all(|ch| ch == '-' || ch == ':' || ch == ' ')
        })
}

/// Parse a table row into cells (trimmed content between `|`).
fn parse_table_cells(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    let inner = &trimmed[1..trimmed.len() - 1]; // strip leading/trailing |
    inner
        .split('|')
        .map(|c| {
            let s = c.trim().to_string();
            // Clean HTML tags commonly found in markdown tables (e.g. <br>, <br/>, <b>, etc.)
            clean_html_tags(&s)
        })
        .collect()
}

/// Remove common HTML tags from cell content, replacing <br> with space.
fn clean_html_tags(s: &str) -> String {
    let mut result = s.to_string();
    // Replace <br>, <br/>, <br /> with a space
    for pat in &["<br>", "<br/>", "<br />", "<BR>", "<BR/>", "<BR />"] {
        result = result.replace(pat, " ");
    }
    // Strip other simple HTML tags like <b>, </b>, <i>, </i>, <em>, </em>, <strong>, </strong>
    let tag_re_simple = ["b", "i", "em", "strong", "code", "u", "s"];
    for tag in &tag_re_simple {
        result = result.replace(&format!("<{}>", tag), "");
        result = result.replace(&format!("</{}>", tag), "");
    }
    // Collapse multiple spaces into one
    while result.contains("  ") {
        result = result.replace("  ", " ");
    }
    result.trim().to_string()
}

/// Render a group of consecutive table lines into styled `Line`s.
fn render_table(table_lines: &[&str], palette: &MdPalette, out: &mut Vec<Line<'static>>) {
    if table_lines.is_empty() {
        return;
    }

    let base_mod = palette.base_modifier;

    // Parse all rows, skipping separator rows
    let mut header_cells: Option<Vec<String>> = None;
    let mut data_rows: Vec<Vec<String>> = Vec::new();

    for (idx, line) in table_lines.iter().enumerate() {
        if is_separator_row(line) {
            continue;
        }
        let cells = parse_table_cells(line);
        if idx == 0 {
            header_cells = Some(cells);
        } else {
            data_rows.push(cells);
        }
    }

    let header = match header_cells {
        Some(h) => h,
        None => return,
    };

    let num_cols = header.len();

    // Compute column widths (display width of widest cell in each column)
    let mut col_widths: Vec<usize> = header.iter().map(|c| display_width(c)).collect();
    // Pad to num_cols
    col_widths.resize(num_cols, 0);

    for row in &data_rows {
        for (j, cell) in row.iter().enumerate() {
            if j < num_cols {
                col_widths[j] = col_widths[j].max(display_width(cell));
            }
        }
    }

    // Fit column widths to terminal width.
    // Each column uses: 1 border + 1 space + content + 1 space = content + 3 per column,
    // plus 1 for the leading border.
    // Subtract 3 from terminal width to account for:
    //   - 1 column scrollbar (conversation widget)
    //   - 2 chars prefix (⏺ or continuation indent prepended by ConversationWidget)
    let term_width = crossterm::terminal::size().map(|(w, _)| w as usize).unwrap_or(120);
    let effective_width = term_width.saturating_sub(3);
    let overhead = 1 + num_cols * 3; // leading │ + (space + content + space│) per col
    let available = effective_width.saturating_sub(overhead);

    let total_natural: usize = col_widths.iter().sum();
    if total_natural > available && available > 0 {
        // Proportionally shrink columns, with a minimum of 6 per column
        let min_col = 6usize;
        let min_total = min_col * num_cols;
        if available >= min_total {
            for w in &mut col_widths {
                let scaled = (*w as f64 / total_natural as f64 * available as f64) as usize;
                *w = scaled.max(min_col);
            }
            // Adjust rounding error on the last column
            let sum: usize = col_widths.iter().sum();
            if sum > available {
                let last = col_widths.last_mut().unwrap();
                *last = last.saturating_sub(sum - available);
            }
        } else {
            // Terminal too narrow — use min_col for all
            for w in &mut col_widths {
                *w = min_col;
            }
        }
    }

    let header_style = Style::default()
        .fg(palette.heading)
        .add_modifier(Modifier::BOLD | base_mod);
    let border_style = Style::default()
        .fg(style_tokens::GREY)
        .add_modifier(base_mod);
    let cell_style = Style::default()
        .fg(palette.text)
        .add_modifier(base_mod);

    // Blank line before table for spacing
    out.push(Line::from(""));

    // Top border: ┌──────┬──────┐
    let mut top_spans: Vec<Span<'static>> = Vec::new();
    top_spans.push(Span::styled(
        Cow::<'static, str>::Borrowed("\u{250C}"),
        border_style,
    ));
    for (j, &w) in col_widths.iter().enumerate() {
        let dash: Cow<'static, str> = Cow::Owned("\u{2500}".repeat(w + 2));
        top_spans.push(Span::styled(dash, border_style));
        if j < num_cols - 1 {
            top_spans.push(Span::styled(
                Cow::<'static, str>::Borrowed("\u{252C}"),
                border_style,
            ));
        }
    }
    top_spans.push(Span::styled(
        Cow::<'static, str>::Borrowed("\u{2510}"),
        border_style,
    ));
    out.push(Line::from(top_spans));

    // Render header row
    render_table_row(&header, &col_widths, num_cols, header_style, border_style, cell_style, palette, out);

    // Render separator line: ├──────┼──────┤
    let mut sep_spans: Vec<Span<'static>> = Vec::new();
    sep_spans.push(Span::styled(
        Cow::<'static, str>::Borrowed("\u{251C}"),
        border_style,
    ));
    for (j, &w) in col_widths.iter().enumerate() {
        let dash: Cow<'static, str> = Cow::Owned("\u{2500}".repeat(w + 2));
        sep_spans.push(Span::styled(dash, border_style));
        if j < num_cols - 1 {
            sep_spans.push(Span::styled(
                Cow::<'static, str>::Borrowed("\u{253C}"),
                border_style,
            ));
        }
    }
    sep_spans.push(Span::styled(
        Cow::<'static, str>::Borrowed("\u{2524}"),
        border_style,
    ));
    out.push(Line::from(sep_spans));

    // Render data rows with inline markdown (bold, code, links)
    for row in &data_rows {
        render_table_data_row(row, &col_widths, num_cols, border_style, cell_style, palette, out);
    }

    // Bottom border: └──────┴──────┘
    let mut bot_spans: Vec<Span<'static>> = Vec::new();
    bot_spans.push(Span::styled(
        Cow::<'static, str>::Borrowed("\u{2514}"),
        border_style,
    ));
    for (j, &w) in col_widths.iter().enumerate() {
        let dash: Cow<'static, str> = Cow::Owned("\u{2500}".repeat(w + 2));
        bot_spans.push(Span::styled(dash, border_style));
        if j < num_cols - 1 {
            bot_spans.push(Span::styled(
                Cow::<'static, str>::Borrowed("\u{2534}"),
                border_style,
            ));
        }
    }
    bot_spans.push(Span::styled(
        Cow::<'static, str>::Borrowed("\u{2518}"),
        border_style,
    ));
    out.push(Line::from(bot_spans));

    // Blank line after table for spacing
    out.push(Line::from(""));
}

/// Render a table header row with padding.
fn render_table_row(
    cells: &[String],
    col_widths: &[usize],
    num_cols: usize,
    content_style: Style,
    border_style: Style,
    _cell_style: Style,
    _palette: &MdPalette,
    out: &mut Vec<Line<'static>>,
) {
    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::styled(Cow::<'static, str>::Borrowed("\u{2502}"), border_style));
    for (j, cell) in cells.iter().enumerate() {
        let width = col_widths.get(j).copied().unwrap_or(0);
        let truncated = truncate_to_width(cell, width);
        let padded: Cow<'static, str> = Cow::Owned(pad_cell(&truncated, width));
        spans.push(Span::styled(
            Cow::<'static, str>::Borrowed(" "),
            border_style,
        ));
        spans.push(Span::styled(padded, content_style));
        spans.push(Span::styled(
            Cow::<'static, str>::Borrowed(" \u{2502}"),
            border_style,
        ));
    }
    // Fill missing columns
    for j in cells.len()..num_cols {
        let width = col_widths.get(j).copied().unwrap_or(0);
        let pad: Cow<'static, str> = Cow::Owned(" ".repeat(width + 2));
        spans.push(Span::styled(pad, border_style));
        spans.push(Span::styled(
            Cow::<'static, str>::Borrowed("\u{2502}"),
            border_style,
        ));
    }
    out.push(Line::from(spans));
}

/// Render a table data row with inline markdown and truncation.
fn render_table_data_row(
    row: &[String],
    col_widths: &[usize],
    num_cols: usize,
    border_style: Style,
    cell_style: Style,
    palette: &MdPalette,
    out: &mut Vec<Line<'static>>,
) {
    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::styled(
        Cow::<'static, str>::Borrowed("\u{2502}"),
        border_style,
    ));
    for j in 0..num_cols {
        let cell = row.get(j).map(|s| s.as_str()).unwrap_or("");
        let width = col_widths.get(j).copied().unwrap_or(0);
        spans.push(Span::styled(
            Cow::<'static, str>::Borrowed(" "),
            border_style,
        ));
        // Truncate cell content to fit column width, then render inline markdown
        let truncated = truncate_to_width(cell, width);
        let cell_spans = parse_inline_spans_with_palette(&truncated, palette);
        let cell_display_width: usize = cell_spans.iter().map(|s| display_width(&s.content)).sum();
        spans.extend(cell_spans);
        // Pad remaining width
        if cell_display_width < width {
            let pad: Cow<'static, str> = Cow::Owned(" ".repeat(width - cell_display_width));
            spans.push(Span::styled(pad, cell_style));
        }
        spans.push(Span::styled(
            Cow::<'static, str>::Borrowed(" \u{2502}"),
            border_style,
        ));
    }
    out.push(Line::from(spans));
}

/// Truncate text to fit within a given display width, adding ellipsis if needed.
fn truncate_to_width(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    let dw = display_width(text);
    if dw <= max_width {
        return text.to_string();
    }
    // Truncate character by character
    let mut result = String::new();
    let mut current_width = 0;
    for c in text.chars() {
        let cw = if ('\u{1100}'..='\u{115F}').contains(&c)
            || ('\u{2E80}'..='\u{A4CF}').contains(&c)
            || ('\u{AC00}'..='\u{D7A3}').contains(&c)
            || ('\u{F900}'..='\u{FAFF}').contains(&c)
            || ('\u{FE10}'..='\u{FE6F}').contains(&c)
            || ('\u{FF01}'..='\u{FF60}').contains(&c)
            || ('\u{FFE0}'..='\u{FFE6}').contains(&c)
            || c > '\u{1F000}'
        {
            2
        } else {
            1
        };
        if current_width + cw > max_width.saturating_sub(1) {
            result.push('\u{2026}'); // …
            break;
        }
        result.push(c);
        current_width += cw;
    }
    result
}

/// Pad a cell string to the given display width using spaces.
fn pad_cell(text: &str, width: usize) -> String {
    let dw = display_width(text);
    if dw >= width {
        text.to_string()
    } else {
        format!("{}{}", text, " ".repeat(width - dw))
    }
}

/// Approximate display width of a string (accounts for CJK double-width).
fn display_width(s: &str) -> usize {
    s.chars()
        .map(|c| {
            if ('\u{1100}'..='\u{115F}').contains(&c)
                || ('\u{2E80}'..='\u{A4CF}').contains(&c)
                || ('\u{AC00}'..='\u{D7A3}').contains(&c)
                || ('\u{F900}'..='\u{FAFF}').contains(&c)
                || ('\u{FE10}'..='\u{FE6F}').contains(&c)
                || ('\u{FF01}'..='\u{FF60}').contains(&c)
                || ('\u{FFE0}'..='\u{FFE6}').contains(&c)
                || c > '\u{1F000}'
            {
                2
            } else {
                1
            }
        })
        .sum()
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

    #[test]
    fn test_table_basic() {
        let md = "| Name | Age |\n|------|-----|\n| Alice | 30 |\n| Bob | 25 |";
        let lines = MarkdownRenderer::render(md);
        // blank + top border + header + separator + 2 data rows + bottom border + blank = 8 lines
        assert_eq!(lines.len(), 8);
        // header is at index 2 (after blank + top border)
        let header_text: String = lines[2].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(header_text.contains("Name"));
        assert!(header_text.contains("Age"));
        // Top border uses box-drawing chars
        let top_text: String = lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(top_text.contains("\u{2500}")); // ─
        assert!(top_text.contains("\u{250C}")); // ┌
    }

    #[test]
    fn test_table_detection() {
        assert!(is_table_line("| a | b |"));
        assert!(is_table_line("| --- | --- |"));
        assert!(!is_table_line("not a table"));
        assert!(!is_table_line("|"));
        assert!(!is_table_line("| only start"));
    }

    #[test]
    fn test_separator_row() {
        assert!(is_separator_row("|---|---|"));
        assert!(is_separator_row("| --- | --- |"));
        assert!(is_separator_row("|:---:|:---|"));
        assert!(!is_separator_row("| Name | Age |"));
    }

    #[test]
    fn test_table_with_surrounding_text() {
        let md = "Before table\n| H1 | H2 |\n|---|---|\n| A | B |\nAfter table";
        let lines = MarkdownRenderer::render(md);
        // 1 text + blank + top + header + sep + data + bottom + blank + 1 text = 9
        assert_eq!(lines.len(), 9);
    }

    #[test]
    fn test_display_width_ascii() {
        assert_eq!(display_width("hello"), 5);
        assert_eq!(display_width(""), 0);
    }

    #[test]
    fn test_display_width_cjk() {
        assert_eq!(display_width("中文"), 4); // 2 chars * 2 width each
        assert_eq!(display_width("a中b"), 4); // 1 + 2 + 1
    }

    #[test]
    fn test_pad_cell() {
        assert_eq!(pad_cell("hi", 5), "hi   ");
        assert_eq!(pad_cell("hello", 5), "hello");
        assert_eq!(pad_cell("longer", 3), "longer");
    }
}
