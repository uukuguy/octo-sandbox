//! Animated welcome panel with Coral breathing border and gradient title.
//!
//! Clean layout: double-line border containing title + subtitle,
//! keyboard shortcuts below. No rain — minimal and focused.
//!
//! Visual identity:
//! - Coral-gold (hue 25° ± 15°) brand palette matching 🦑
//! - Double-line border (╔═╗║╚═╝) with clockwise sweep animation
//! - ASCII Art "GRID" (Tier 3) / compact (Tier 2) / text (Tier 1)
//! - Synchronized breathing animation across all elements (~6s cycle)

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

    /// Breathing curve with dwell at peak only.
    /// Peak holds at 1.0 for ~25% of cycle; trough flows smoothly without pause.
    /// Returns 0.0..1.0.
    #[inline]
    fn breathe_ease(&self) -> f64 {
        let raw = self.state.breathe_phase.sin(); // -1..1
        // Map to 0..1, stretch top only: multiply by 1.3 and clamp ceiling
        let mapped = raw * 0.5 + 0.5; // 0..1
        (mapped * 1.3).min(1.0) // 0..1.0, top 23% of sin clamps to 1.0
    }

    /// Write a centered string with breathing animation synced to logo.
    fn write_breathing_line(
        &self,
        buf: &mut Buffer,
        area: Rect,
        y: u16,
        text: &str,
        lightness_base: f64,
        lightness_amplitude: f64,
    ) {
        if y >= area.y + area.height {
            return;
        }
        let text_len = text.chars().count() as u16;
        let start_x = area.x + area.width.saturating_sub(text_len) / 2;
        let fade = self.state.fade_progress as f64;
        let ease = self.breathe_ease();

        for (i, ch) in text.chars().enumerate() {
            if ch == ' ' {
                continue;
            }
            let center = text_len as f64 / 2.0;
            let dist = ((i as f64 - center) / center.max(1.0)).abs();
            let hue = self.state.accent_hue + dist * 15.0;
            let b = lightness_base + lightness_amplitude * ease;
            let color = hsl_to_rgb(hue, 0.85 * fade, b * fade);
            Self::put(buf, area, start_x + i as u16, y, ch, color);
        }
    }

    // 5-row line-drawing "GRID" (GRID_UI_UX_DESIGN §2.3, D: left-square right-round)
    //   G        R        I        D
    //  ╭───╮   ╭───╮   ╶─┬─╴   ┌───╮
    //  │       │   │     │     │    │
    //  │ ──╮   ├───┘     │     │    │
    //  │   │   │  ╲      │     │    │
    //  ╰───╯   ╵   ╲   ╶─┴─╴   └───╯
    const LOGO_LINES: [&'static str; 5] = [
        "  \u{256d}\u{2500}\u{2500}\u{2500}\u{256e}  \u{256d}\u{2500}\u{2500}\u{2500}\u{256e}  \u{2576}\u{2500}\u{252c}\u{2500}\u{2574}  \u{250c}\u{2500}\u{2500}\u{2500}\u{256e}",
        "  \u{2502}      \u{2502}   \u{2502}    \u{2502}    \u{2502}   \u{2502}",
        "  \u{2502} \u{2500}\u{2500}\u{256e}  \u{251c}\u{2500}\u{2500}\u{2500}\u{2518}    \u{2502}    \u{2502}   \u{2502}",
        "  \u{2502}   \u{2502}  \u{2502}  \u{2572}     \u{2502}    \u{2502}   \u{2502}",
        "  \u{2570}\u{2500}\u{2500}\u{2500}\u{256f}  \u{2575}   \u{2572}  \u{2576}\u{2500}\u{2534}\u{2500}\u{2574}  \u{2514}\u{2500}\u{2500}\u{2500}\u{256f}",
    ];
    const LOGO_WIDTH: usize = 28;
    const LOGO_HEIGHT: usize = 5;

    /// Render the dot-grid background with pulsing intersections and GRID logo as negative space.
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
        let ease = self.breathe_ease();

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
                            // Symmetric hue shift: center=accent, edges=+15°
                            let center = Self::LOGO_WIDTH as f64 / 2.0;
                            let dist = ((lc as f64 - center) / center).abs();
                            let logo_hue = self.state.accent_hue + dist * 15.0;
                            // Breathing: 0.35→0.70 (ratio 2:1, synced with all text)
                            let b = 0.35 + 0.35 * ease;
                            let color = hsl_to_rgb(logo_hue, 0.90 * fade, b * fade);
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
                    let intensity = 0.18 + 0.12 * (wave * 0.5 + 0.5);

                    let dot_hue = self.state.accent_hue + norm_dist * 15.0;
                    let color = hsl_to_rgb(dot_hue, 0.55 * fade, intensity * fade);
                    Self::put(buf, area, ax, ay, '\u{00B7}', color);
                }
            }
        }
    }

    /// Draw double-line border with clockwise sweep + breathing animation.
    fn draw_border(&self, buf: &mut Buffer, area: Rect, bx: u16, by: u16, bw: u16, bh: u16) {
        let fade = self.state.fade_progress as f64;
        let perimeter = 2 * (bw + bh);
        let offset = self.state.gradient_offset;
        let ease = self.breathe_ease();

        let border_color = |idx: u16| -> Color {
            // Sweep: offset drives a focused bright spot orbiting the border
            // Subtract offset so bright spot moves clockwise (same direction as idx)
            let t = ((idx as f64 / perimeter as f64) - offset as f64 / 360.0).rem_euclid(1.0);
            // Hue: accent ±15° symmetric
            let hue = self.state.accent_hue + (t - 0.5).abs() * 30.0;
            // Focused cosine²: sharper bright spot with wider dim region
            let cos_val = (t * std::f64::consts::TAU).cos() * 0.5 + 0.5;
            let sweep = cos_val * cos_val; // cos² — concentrated peak
            let b = 0.15 + 0.35 * sweep + 0.06 * ease;
            hsl_to_rgb(hue, 0.75 * fade, b * fade)
        };

        // Clockwise perimeter: Top(L→R) → Right(T→B) → Bottom(R→L) → Left(B→T)
        // Top: ╔═══╗  idx 0..bw
        Self::put(buf, area, bx, by, '\u{2554}', border_color(0));
        for i in 1..bw.saturating_sub(1) {
            Self::put(buf, area, bx + i, by, '\u{2550}', border_color(i));
        }
        Self::put(buf, area, bx + bw - 1, by, '\u{2557}', border_color(bw));

        // Bottom: ╚═══╝  idx (bw+bh)..(2*bw+bh), rendered RIGHT→LEFT
        Self::put(buf, area, bx + bw - 1, by + bh - 1, '\u{255d}', border_color(bw + bh));
        for i in 1..bw.saturating_sub(1) {
            Self::put(buf, area, bx + bw - 1 - i, by + bh - 1, '\u{2550}', border_color(bw + bh + i));
        }
        Self::put(buf, area, bx, by + bh - 1, '\u{255a}', border_color(2 * bw + bh));

        // Right ║: idx bw+1..(bw+bh-1) top→bottom
        // Left  ║: idx (2*bw+bh+1)..(2*bw+2*bh-1) bottom→top
        for j in 1..bh.saturating_sub(1) {
            Self::put(buf, area, bx + bw - 1, by + j, '\u{2551}', border_color(bw + j));
            Self::put(buf, area, bx, by + bh - 1 - j, '\u{2551}', border_color(2 * bw + bh + j));
        }
    }
}

impl Widget for WelcomePanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 10 || area.height < 3 {
            return;
        }

        // Layout constants
        let subtitle = "Autonomous Agent Studio";
        let help = "/help  \u{2502}  Enter: send  \u{2502}  Esc: interrupt  \u{2502}  Ctrl+C: quit";

        if area.height < 5 {
            // ── Tier 1: tiny terminal — breathing brand line ──
            let cy = area.y + area.height / 2;
            self.write_breathing_line(
                buf, area, cy,
                "\u{25C6} Grid \u{2014} Autonomous Agent Studio",
                0.30, 0.30,
            );
        } else if area.height < 12 {
            // ── Tier 2: small — border + compact GRID + subtitle ──
            let box_w = (area.width.saturating_sub(4)).min(50);
            let box_h = 6u16.min(area.height);
            let bx = area.x + (area.width.saturating_sub(box_w)) / 2;
            let by = area.y + (area.height.saturating_sub(box_h)) / 2;

            self.draw_border(buf, area, bx, by, box_w, box_h);
            self.write_breathing_line(buf, area, by + 1, "\u{2554}\u{2550}\u{2557}  \u{2566}\u{2550}\u{2557}  \u{2566}  \u{2554}\u{2550}\u{2550}\u{2557}", 0.35, 0.35);
            self.write_breathing_line(buf, area, by + 2, "\u{2551} \u{2557}  \u{2560}\u{2566}\u{2518}  \u{2551}  \u{2551}  \u{2551}", 0.35, 0.35);
            self.write_breathing_line(buf, area, by + 3, "\u{255a}\u{2550}\u{255d}  \u{2569}\u{255a}\u{2550}  \u{2569}  \u{255a}\u{2550}\u{2550}\u{255d}", 0.35, 0.35);
            self.write_breathing_line(buf, area, by + 4, subtitle, 0.38, 0.27);
        } else {
            // ── Tier 3: full — grid background with GRID logo + info box ──
            // Info box width fits content (help text + 4 padding), capped by terminal
            let help_len = help.chars().count() as u16;
            let box_w = (help_len + 4).min(area.width.saturating_sub(4));
            let box_h = 3u16; // info box: border + help text + border
            let grid_h = (area.height.saturating_sub(box_h + 2)).clamp(5, 18) as usize;
            let grid_max = (area.width.saturating_sub(4)).min(80) as usize;
            let grid_w = grid_max.clamp(Self::LOGO_WIDTH, 80);

            // Center vertically: grid + 1 blank + subtitle + 1 blank + info box
            let total_h = grid_h as u16 + 1 + 1 + 1 + box_h;
            let start_y = area.y + area.height.saturating_sub(total_h) / 2;
            let center_x = area.x + (area.width.saturating_sub(box_w)) / 2;

            // Grid background with GRID logo as negative space
            let grid_x = area.x + (area.width.saturating_sub(grid_w as u16)) / 2;
            self.render_grid_bg(buf, area, grid_x, start_y, grid_w, grid_h);

            // Subtitle — high base for readability, peak near logo's 0.70
            let subtitle_y = start_y + grid_h as u16 + 1;
            self.write_breathing_line(buf, area, subtitle_y, subtitle, 0.38, 0.27);

            // Info box below with help text (1 blank line gap)
            let by = subtitle_y + 2;
            self.draw_border(buf, area, center_x, by, box_w, box_h);

            let max_inner = (box_w as usize).saturating_sub(4);
            let help_chars = help.chars().count();
            let display_text: String = if help_chars > max_inner {
                help.chars().take(max_inner - 1).chain(std::iter::once('\u{2026}')).collect()
            } else {
                help.to_string()
            };
            let info_y = by + 1;
            // Help text — always readable (base 0.40), peak near logo (0.65)
            self.write_breathing_line(buf, area, info_y, &display_text, 0.40, 0.25);
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
        assert!(content.contains("Grid"), "Tier 1 should contain Grid brand");
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
        assert!(content.contains("\u{00B7}") || content.contains("\u{256d}") || content.contains("\u{2502}"),
            "Tier 3 should contain grid dots or line-drawing chars");
        assert!(!content.contains("AGENT"), "Should NOT contain AGENT");
    }

    #[test]
    fn welcome_panel_full_layout() {
        let state = WelcomePanelState::new();
        let widget = WelcomePanel::new(&state, "gpt-4o");
        let area = Rect::new(0, 0, 100, 24);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Autonomous Agent Studio"), "Should show subtitle");
        assert!(content.contains("Enter"), "Should show help text");
    }
}
