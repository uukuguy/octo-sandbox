//! Animated welcome panel with amber-gold breathing border and gradient title.
//!
//! Clean layout: double-line border containing title + subtitle,
//! model info and keyboard shortcuts below. No rain — minimal and focused.
//!
//! Visual identity (vs opendev-tui):
//! - Amber-gold (hue 30-60) instead of cyan-blue (190-250)
//! - Double-line border (╔═╗║╚═╝) instead of rounded (╭─╮│╰─╯)
//! - ASCII Art "OCTO" (Tier 3) / "O C T O" (Tier 2) / 🦑 (Tier 1)
//! - Breathing gradient animation on border + title

mod color;
mod state;

pub use state::WelcomePanelState;

use ratatui::{buffer::Buffer, layout::Rect, style::Color, widgets::Widget};

use color::hsl_to_rgb;

/// Stateless widget that renders the welcome panel from `WelcomePanelState`.
pub struct WelcomePanel<'a> {
    state: &'a WelcomePanelState,
}

impl<'a> WelcomePanel<'a> {
    pub fn new(state: &'a WelcomePanelState, _model_name: &'a str) -> Self {
        Self { state }
    }

    #[inline]
    fn put(buf: &mut Buffer, area: Rect, x: u16, y: u16, ch: char, fg: Color) {
        if x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_char(ch);
                cell.set_fg(fg);
            }
        }
    }

    /// Write a centered string with a single color.
    fn center_text(buf: &mut Buffer, area: Rect, y: u16, text: &str, fg: Color) {
        if y >= area.y + area.height {
            return;
        }
        let len = text.len() as u16;
        let x = area.x + area.width.saturating_sub(len) / 2;
        buf.set_string(x, y, text, ratatui::style::Style::default().fg(fg));
    }

    /// Write a centered string with per-character amber gradient sweep.
    fn write_gradient_line(
        &self,
        buf: &mut Buffer,
        area: Rect,
        y: u16,
        text: &str,
        base_lightness: f64,
    ) {
        if y >= area.y + area.height {
            return;
        }
        let text_len = text.chars().count() as u16;
        let start_x = area.x + area.width.saturating_sub(text_len) / 2;
        let fade = self.state.fade_progress as f64;
        let breathe = 0.85 + 0.15 * self.state.breathe_phase.sin();

        for (i, ch) in text.chars().enumerate() {
            if ch == ' ' {
                continue;
            }
            let sweep = (i as u16 * 5 + self.state.gradient_offset) % 360;
            let hue = 30.0 + (sweep as f64 / 360.0) * 30.0;
            let lit = base_lightness * breathe * fade;
            let color = hsl_to_rgb(hue, 0.80 * fade, lit);
            Self::put(buf, area, start_x + i as u16, y, ch, color);
        }
    }

    // 5-row ASCII art: "OCTO" using half-block characters for smooth rounded look (width 37)
    const LOGO_LINES: [&'static str; 5] = [
        " \u{2584}\u{2584}\u{2584}\u{2584}\u{2584}   \u{2584}\u{2584}\u{2584}\u{2584}\u{2584}  \u{2584}\u{2584}\u{2584}\u{2584}\u{2584}\u{2584}  \u{2584}\u{2584}\u{2584}\u{2584}\u{2584} ",
        "\u{2588}\u{2588}   \u{2588}\u{2588} \u{2588}\u{2588}        \u{2588}\u{2588}   \u{2588}\u{2588}   \u{2588}\u{2588}",
        "\u{2588}\u{2588}   \u{2588}\u{2588} \u{2588}\u{2588}        \u{2588}\u{2588}   \u{2588}\u{2588}   \u{2588}\u{2588}",
        "\u{2588}\u{2588}   \u{2588}\u{2588} \u{2588}\u{2588}        \u{2588}\u{2588}   \u{2588}\u{2588}   \u{2588}\u{2588}",
        " \u{2580}\u{2580}\u{2580}\u{2580}\u{2580}   \u{2580}\u{2580}\u{2580}\u{2580}\u{2580}  \u{2580}\u{2580}\u{2580}\u{2580}\u{2580}\u{2580}  \u{2580}\u{2580}\u{2580}\u{2580}\u{2580} ",
    ];
    const LOGO_WIDTH: usize = 37;
    const LOGO_HEIGHT: usize = 5;

    /// Render the dot-grid background with pulsing intersections and OCTO logo as negative space.
    fn render_grid_bg(
        &self,
        buf: &mut Buffer,
        area: Rect,
        gx: u16,
        gy: u16,
        gw: usize,
        gh: usize,
    ) {
        let fade = self.state.fade_progress as f64;
        let breathe = self.state.breathe_phase;

        // Logo exclusion zone (centered in grid)
        let show_logo = gh >= Self::LOGO_HEIGHT + 2 && gw >= Self::LOGO_WIDTH + 2;
        let logo_start_col = gw.saturating_sub(Self::LOGO_WIDTH) / 2;
        let logo_end_col = logo_start_col + Self::LOGO_WIDTH;
        let logo_start_row = gh.saturating_sub(Self::LOGO_HEIGHT) / 2;
        let logo_end_row = logo_start_row + Self::LOGO_HEIGHT;

        // Grid spacing: dot every 3 cols, every 2 rows
        let col_space = 3;
        let row_space = 2;

        for row in 0..gh {
            for col in 0..gw {
                let ax = gx + col as u16;
                let ay = gy + row as u16;

                // Logo zone: render block art letters
                if show_logo
                    && row >= logo_start_row
                    && row < logo_end_row
                    && col >= logo_start_col
                    && col < logo_end_col
                {
                    let lr = row - logo_start_row;
                    let lc = col - logo_start_col;
                    if let Some(ch) = Self::LOGO_LINES[lr].chars().nth(lc) {
                        if ch != ' ' {
                            // Block chars: bright breathing amber
                            let letter_t = lc as f64 / Self::LOGO_WIDTH as f64;
                            let letter_hue = 30.0 + letter_t * 20.0;
                            let b = 0.40 + 0.20 * (1.0 + breathe.sin());
                            let color = hsl_to_rgb(letter_hue, 0.85 * fade, b * fade);
                            Self::put(buf, area, ax, ay, ch, color);
                        }
                    }
                    continue;
                }

                // Grid dots at intersections
                let is_intersection = col % col_space == 0 && row % row_space == 0;
                if is_intersection {
                    // Distance from center for radial pulse
                    let cx = gw as f64 / 2.0;
                    let cy = gh as f64 / 2.0;
                    let dx = col as f64 - cx;
                    let dy = (row as f64 - cy) * 1.5; // aspect ratio correction
                    let dist = (dx * dx + dy * dy).sqrt();
                    let max_dist = (cx * cx + cy * cy).sqrt();
                    let norm_dist = dist / max_dist;

                    // Radial wave: dots pulse outward from center
                    let wave = (breathe - norm_dist * 4.0).sin();
                    let intensity = 0.08 + 0.12 * (wave * 0.5 + 0.5);

                    let dot_hue = 35.0 + norm_dist * 15.0;
                    let color = hsl_to_rgb(dot_hue, 0.4 * fade, intensity * fade);
                    Self::put(buf, area, ax, ay, '\u{00B7}', color);
                }
            }
        }
    }

    /// Draw double-line border with animated amber gradient.
    fn draw_border(&self, buf: &mut Buffer, area: Rect, bx: u16, by: u16, bw: u16, bh: u16) {
        let offset = self.state.gradient_offset;
        let fade = self.state.fade_progress as f64;
        let breathe = 0.85 + 0.15 * self.state.breathe_phase.sin();
        let perimeter = 2 * (bw + bh);

        let border_color = |idx: u16| -> Color {
            let t = ((idx as f64 / perimeter as f64) + offset as f64 / 360.0) % 1.0;
            let hue = 30.0 + t * 30.0;
            hsl_to_rgb(hue, 0.60 * fade, 0.28 * breathe * fade)
        };

        // Top: ╔═══╗
        Self::put(buf, area, bx, by, '\u{2554}', border_color(0));
        for i in 1..bw.saturating_sub(1) {
            Self::put(buf, area, bx + i, by, '\u{2550}', border_color(i));
        }
        Self::put(buf, area, bx + bw - 1, by, '\u{2557}', border_color(bw));

        // Bottom: ╚═══╝
        Self::put(buf, area, bx, by + bh - 1, '\u{255a}', border_color(bw + bh));
        for i in 1..bw.saturating_sub(1) {
            Self::put(buf, area, bx + i, by + bh - 1, '\u{2550}', border_color(bw + bh + i));
        }
        Self::put(buf, area, bx + bw - 1, by + bh - 1, '\u{255d}', border_color(2 * bw + bh));

        // Sides: ║
        for j in 1..bh.saturating_sub(1) {
            Self::put(buf, area, bx, by + j, '\u{2551}', border_color(bw + j));
            Self::put(buf, area, bx + bw - 1, by + j, '\u{2551}', border_color(2 * bw + bh + j));
        }
    }
}

impl Widget for WelcomePanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 10 || area.height < 3 {
            return;
        }

        let fade = self.state.fade_progress as f64;
        let dim = hsl_to_rgb(40.0, 0.25 * fade, 0.35 * fade);

        // Layout constants
        let subtitle = "Autonomous AI Workbench";
        let help = "Enter: send  |  Ctrl+C: cancel  |  Ctrl+D: debug  |  Ctrl+E: eval";

        if area.height < 5 {
            // ── Tier 1: tiny terminal — emoji brand ──
            let cy = area.y + area.height / 2;
            Self::center_text(buf, area, cy, "\u{1F991} octo \u{2014} autonomous ai workbench", dim);
        } else if area.height < 12 {
            // ── Tier 2: small — border + spaced title + subtitle ──
            let box_w = (area.width.saturating_sub(4)).min(50);
            let box_h = 5u16.min(area.height);
            let bx = area.x + (area.width.saturating_sub(box_w)) / 2;
            let by = area.y + (area.height.saturating_sub(box_h)) / 2;

            self.draw_border(buf, area, bx, by, box_w, box_h);
            self.write_gradient_line(buf, area, by + 1, "O C T O", 0.55);
            Self::center_text(buf, area, by + 3, subtitle, dim);
        } else {
            // ── Tier 3: full — grid background with OCTO logo + info box ──
            let box_w = (area.width.saturating_sub(4)).min(70);
            let box_h = 3u16; // info box: border + help text + border
            let grid_h = (area.height.saturating_sub(box_h + 2)).clamp(5, 18) as usize;
            let grid_w = ((box_w as f32 * 0.9) as usize).clamp(Self::LOGO_WIDTH, 80);

            // Center vertically
            let total_h = grid_h as u16 + 1 + box_h;
            let start_y = area.y + area.height.saturating_sub(total_h) / 2;
            let center_x = area.x + (area.width.saturating_sub(box_w)) / 2;

            // Grid background with OCTO logo as negative space
            let grid_x = area.x + (area.width.saturating_sub(grid_w as u16)) / 2;
            self.render_grid_bg(buf, area, grid_x, start_y, grid_w, grid_h);

            // "a u t o n o m o u s   a i   w o r k b e n c h" subtitle
            let subtitle_y = start_y + grid_h as u16;
            self.write_gradient_line(buf, area, subtitle_y, subtitle, 0.40);

            // Info box below with model + help
            let by = subtitle_y + 1;
            self.draw_border(buf, area, center_x, by, box_w, box_h);

            let max_inner = (box_w as usize).saturating_sub(4); // 2 border chars + 2 padding
            let display_text: String = if help.len() > max_inner {
                help.chars().take(max_inner - 1).chain(std::iter::once('\u{2026}')).collect()
            } else {
                help.to_string()
            };
            let info_y = by + 1;
            self.write_gradient_line(buf, area, info_y, &display_text, 0.35);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn welcome_panel_renders_without_panic() {
        let state = WelcomePanelState::new();
        let widget = WelcomePanel::new(&state, "test-model");
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        let modified: usize = (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| (x, y)))
            .filter(|&(x, y)| buf.cell((x, y)).unwrap().symbol() != " ")
            .count();
        assert!(modified > 20, "Expected visible output, got {modified} cells");
    }

    #[test]
    fn welcome_panel_small_terminal() {
        let state = WelcomePanelState::new();
        let widget = WelcomePanel::new(&state, "test-model");
        let area = Rect::new(0, 0, 80, 4);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);
        let modified: usize = (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| (x, y)))
            .filter(|&(x, y)| buf.cell((x, y)).unwrap().symbol() != " ")
            .count();
        assert!(modified > 5, "Tier 1 should render text, got {modified} cells");
    }

    #[test]
    fn welcome_panel_tier1_emoji() {
        let state = WelcomePanelState::new();
        let widget = WelcomePanel::new(&state, "test-model");
        let area = Rect::new(0, 0, 60, 4);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("octo"), "Tier 1 should contain 'octo' brand");
    }

    #[test]
    fn welcome_panel_tier2() {
        let state = WelcomePanelState::new();
        let widget = WelcomePanel::new(&state, "gpt-4o");
        let area = Rect::new(0, 0, 60, 8);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);
        let modified: usize = (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| (x, y)))
            .filter(|&(x, y)| buf.cell((x, y)).unwrap().symbol() != " ")
            .count();
        assert!(modified > 15, "Tier 2 should render border + text, got {modified} cells");
    }

    #[test]
    fn welcome_panel_tier3_grid_background() {
        let state = WelcomePanelState::new();
        let widget = WelcomePanel::new(&state, "test-model");
        let area = Rect::new(0, 0, 80, 20);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        // Grid background renders dot characters and block art
        assert!(content.contains("\u{00B7}") || content.contains("\u{2588}"),
            "Tier 3 should contain grid dots or block chars");
        assert!(!content.contains("AGENT"), "Should NOT contain AGENT");
    }

    #[test]
    fn welcome_panel_full_layout() {
        let state = WelcomePanelState::new();
        let widget = WelcomePanel::new(&state, "gpt-4o");
        let area = Rect::new(0, 0, 100, 20);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Autonomous AI Workbench"), "Should show subtitle");
        assert!(content.contains("Enter"), "Should show help text");
    }
}
