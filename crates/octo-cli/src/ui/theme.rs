//! Color theme management for CLI output

use owo_colors::Style;

/// Named color themes for the CLI
#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
pub enum ThemeName {
    /// Ocean cyan (recommended)
    #[default]
    Cyan,
    /// State Grid green
    Sgcc,
    /// Dodge blue
    Blue,
    /// Deep indigo
    Indigo,
    /// Violet
    Violet,
    /// Emerald green
    Emerald,
    /// Amber gold
    Amber,
    /// Coral orange
    Coral,
    /// Rose red
    Rose,
    /// Teal
    Teal,
    /// Sunset gradient
    Sunset,
    /// Moonlight slate
    Slate,
}

/// Active theme with computed styles
pub struct Theme {
    pub name: ThemeName,
    pub accent: Style,
    pub accent_dim: Style,
    pub success: Style,
    pub error: Style,
    pub warning: Style,
    pub muted: Style,
    pub bold: Style,
}

impl Theme {
    pub fn from_name(name: ThemeName) -> Self {
        let (r, g, b) = match name {
            ThemeName::Cyan => (6, 182, 212),
            ThemeName::Sgcc => (0, 132, 61),
            ThemeName::Blue => (59, 130, 246),
            ThemeName::Indigo => (99, 102, 241),
            ThemeName::Violet => (139, 92, 246),
            ThemeName::Emerald => (16, 185, 129),
            ThemeName::Amber => (245, 158, 11),
            ThemeName::Coral => (249, 115, 22),
            ThemeName::Rose => (244, 63, 94),
            ThemeName::Teal => (20, 184, 166),
            ThemeName::Sunset => (236, 72, 153),
            ThemeName::Slate => (148, 163, 184),
        };

        Self {
            name,
            accent: Style::new().truecolor(r, g, b),
            accent_dim: Style::new().truecolor(r / 2, g / 2, b / 2),
            success: Style::new().truecolor(34, 197, 94),
            error: Style::new().truecolor(239, 68, 68),
            warning: Style::new().truecolor(234, 179, 8),
            muted: Style::new().truecolor(100, 116, 139),
            bold: Style::new().bold(),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::from_name(ThemeName::default())
    }
}
