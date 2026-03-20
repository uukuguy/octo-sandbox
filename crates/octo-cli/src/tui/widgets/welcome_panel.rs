//! Welcome panel widget shown when the conversation is empty.
//!
//! Displays ASCII art logo, version info, and keyboard shortcuts.

use ratatui::prelude::*;
use ratatui::widgets::Widget;

/// ASCII art for the Octo Agent welcome screen.
const LOGO: &[&str] = &[
    r"   ___       _           ",
    r"  / _ \  ___| |_ ___    ",
    r" | | | |/ __| __/ _ \   ",
    r" | |_| | (__| || (_) |  ",
    r"  \___/ \___|\__\___/   ",
];

/// Welcome panel widget.
pub struct WelcomePanel<'a> {
    model_name: &'a str,
}

impl<'a> WelcomePanel<'a> {
    pub fn new(model_name: &'a str) -> Self {
        Self { model_name }
    }
}

impl Widget for WelcomePanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 5 || area.width < 30 {
            // Too small for welcome — just show minimal text
            let text = "Type a message to start.";
            let x = area.x + area.width.saturating_sub(text.len() as u16) / 2;
            let y = area.y + area.height / 2;
            if y < area.y + area.height {
                buf.set_string(x, y, text, Style::default().fg(Color::DarkGray));
            }
            return;
        }

        // Center the logo vertically
        let total_content_height = LOGO.len() as u16 + 6; // logo + spacing + info lines
        let start_y = area.y + area.height.saturating_sub(total_content_height) / 2;
        let mut y = start_y;

        // Render logo
        let logo_style = Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD);
        for line in LOGO {
            if y >= area.y + area.height {
                break;
            }
            let x = area.x + area.width.saturating_sub(line.len() as u16) / 2;
            buf.set_string(x, y, line, logo_style);
            y += 1;
        }

        y += 1; // spacing

        // Subtitle
        if y < area.y + area.height {
            let subtitle = "Conversation-Centric AI Agent";
            let x = area.x + area.width.saturating_sub(subtitle.len() as u16) / 2;
            buf.set_string(
                x,
                y,
                subtitle,
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            );
            y += 1;
        }

        y += 1; // spacing

        // Model info
        if y < area.y + area.height {
            let model_line = format!("Model: {}", self.model_name);
            let x = area.x + area.width.saturating_sub(model_line.len() as u16) / 2;
            buf.set_string(x, y, &model_line, Style::default().fg(Color::DarkGray));
            y += 1;
        }

        y += 1; // spacing

        // Shortcuts
        let shortcuts = [
            "Type a message and press Enter to start",
            "Ctrl+C: cancel | Ctrl+D: debug | Ctrl+E: eval",
        ];
        for shortcut in &shortcuts {
            if y >= area.y + area.height {
                break;
            }
            let x = area.x + area.width.saturating_sub(shortcut.len() as u16) / 2;
            buf.set_string(x, y, shortcut, Style::default().fg(Color::DarkGray));
            y += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn welcome_panel_renders_without_panic() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 24));
        let widget = WelcomePanel::new("test-model");
        widget.render(Rect::new(0, 0, 80, 24), &mut buf);
        // Check that the subtitle appears somewhere in the buffer
        let content = buf.content().iter().map(|c| c.symbol()).collect::<String>();
        assert!(content.contains("Conversation-Centric AI Agent"));
    }

    #[test]
    fn welcome_panel_small_terminal() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 4));
        let widget = WelcomePanel::new("test-model");
        widget.render(Rect::new(0, 0, 20, 4), &mut buf);
        // Should not panic, just show minimal text
    }
}
