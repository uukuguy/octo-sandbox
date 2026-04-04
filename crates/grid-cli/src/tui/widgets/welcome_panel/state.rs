//! WelcomePanelState — animation state ticked by the TUI event loop.
//!
//! Drives gradient sweep and breathing animation for the welcome panel border and title.

/// Persistent animation state for the welcome panel.
#[derive(Debug, Clone)]
pub struct WelcomePanelState {
    /// Gradient sweep offset (0..360), rotated each tick.
    pub(super) gradient_offset: u16,
    /// Breathing phase (0..TAU), controls border/title brightness oscillation.
    pub(super) breathe_phase: f64,
    /// Fade multiplier (1.0 = fully visible, 0.0 = invisible).
    pub(super) fade_progress: f32,
    /// Whether the panel is currently fading out.
    pub is_fading: bool,
    /// Set to `true` once fade completes; panel should no longer render.
    pub fade_complete: bool,
    /// Accent hue derived from TuiTheme (0..360°). Default: 235° (Indigo).
    pub(super) accent_hue: f64,
}

impl WelcomePanelState {
    pub fn new() -> Self {
        Self {
            gradient_offset: 0,
            breathe_phase: 0.0,
            fade_progress: 1.0,
            is_fading: false,
            fade_complete: false,
            accent_hue: 235.0, // Indigo default
        }
    }

    /// Update accent hue from theme accent color (call once at init or on theme change).
    pub fn set_accent_hue(&mut self, r: u8, g: u8, b: u8) {
        self.accent_hue = super::color::rgb_to_hue(r, g, b);
    }

    /// Advance animations by one tick (~60ms).
    pub fn tick(&mut self, _terminal_width: u16, _terminal_height: u16) {
        if self.is_fading {
            self.fade_progress -= 0.1;
            if self.fade_progress <= 0.0 {
                self.fade_progress = 0.0;
                self.fade_complete = true;
            }
            return;
        }

        // Gradient rotation (slower, more elegant sweep)
        self.gradient_offset = (self.gradient_offset + 3) % 360;

        // Breathing phase: full cycle in ~67 ticks (~4s at 60ms)
        self.breathe_phase += std::f64::consts::TAU / 67.0;
        if self.breathe_phase >= std::f64::consts::TAU {
            self.breathe_phase -= std::f64::consts::TAU;
        }
    }

    /// Begin the fade-out animation.
    pub fn start_fade(&mut self) {
        self.is_fading = true;
    }
}

impl Default for WelcomePanelState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_tick_gradient() {
        let mut state = WelcomePanelState::new();
        assert_eq!(state.gradient_offset, 0);
        state.tick(80, 24);
        assert_eq!(state.gradient_offset, 3);
        state.tick(80, 24);
        assert_eq!(state.gradient_offset, 6);
    }

    #[test]
    fn test_breathe_advances() {
        let mut state = WelcomePanelState::new();
        let before = state.breathe_phase;
        state.tick(80, 24);
        assert!(state.breathe_phase > before);
    }

    #[test]
    fn test_fade_completes() {
        let mut state = WelcomePanelState::new();
        state.start_fade();
        for _ in 0..10 {
            state.tick(80, 24);
        }
        assert!(state.fade_complete);
        assert!(state.fade_progress <= 0.0);
    }

    #[test]
    fn test_accent_hue_from_theme() {
        let mut state = WelcomePanelState::new();
        assert!((state.accent_hue - 235.0).abs() < 0.01);
        // Set to cyan-ish (6, 182, 212)
        state.set_accent_hue(6, 182, 212);
        assert!(state.accent_hue > 180.0 && state.accent_hue < 200.0,
            "cyan hue should be ~187, got {}", state.accent_hue);
    }

    #[test]
    fn test_no_tick_during_fade() {
        let mut state = WelcomePanelState::new();
        state.gradient_offset = 100;
        state.start_fade();
        state.tick(80, 24);
        // Gradient should not advance during fade
        assert_eq!(state.gradient_offset, 100);
    }
}
