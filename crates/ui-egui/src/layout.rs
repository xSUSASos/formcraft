//! Panel layout: which panels are open, docked or floating, and their sizes. Persisted between
//! runs; "Reset layout" restores the defaults.

use serde::{Deserialize, Serialize};

use crate::theme::ThemeKind;

/// Where a dockable panel lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dock {
    Docked,
    Floating,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PanelState {
    pub open: bool,
    pub dock: Dock,
    /// Docked width or height in points.
    pub size: f32,
}

impl PanelState {
    const fn docked(open: bool, size: f32) -> Self {
        PanelState { open, dock: Dock::Docked, size }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    pub theme: ThemeKind,
    pub browser: PanelState,
    pub timeline: PanelState,
    pub text_commands: PanelState,
    pub assistant: PanelState,
    pub nav_bar: bool,
    pub view_cube: bool,
    pub grid: bool,
    pub visual_style: VisualStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum VisualStyle {
    Shaded,
    #[default]
    ShadedEdges,
    Wireframe,
}

impl VisualStyle {
    pub const ALL: [VisualStyle; 3] = [VisualStyle::Shaded, VisualStyle::ShadedEdges, VisualStyle::Wireframe];
    pub fn label(&self) -> &'static str {
        match self {
            VisualStyle::Shaded => "Shaded",
            VisualStyle::ShadedEdges => "Shaded with Edges",
            VisualStyle::Wireframe => "Wireframe",
        }
    }
}

impl Default for Layout {
    fn default() -> Self {
        Layout {
            theme: ThemeKind::Dark,
            browser: PanelState::docked(true, 250.0),
            timeline: PanelState::docked(true, 44.0),
            // Text Commands starts collapsed, like Fusion's.
            text_commands: PanelState::docked(false, 160.0),
            assistant: PanelState::docked(false, 300.0),
            nav_bar: true,
            view_cube: true,
            grid: true,
            visual_style: VisualStyle::ShadedEdges,
        }
    }
}

impl Layout {
    /// Defaults, keeping the user's theme.
    pub fn reset(&mut self) {
        let theme = self.theme;
        *self = Layout { theme, ..Layout::default() };
    }

    /// Clamp sizes coming from a stored file (it may be edited or corrupt).
    pub fn sanitize(&mut self) {
        let clamp = |p: &mut PanelState, lo: f32, hi: f32, def: f32| {
            p.size = if p.size.is_finite() { p.size.clamp(lo, hi) } else { def };
        };
        clamp(&mut self.browser, 150.0, 700.0, 250.0);
        clamp(&mut self.timeline, 36.0, 120.0, 44.0);
        clamp(&mut self.text_commands, 80.0, 600.0, 160.0);
        clamp(&mut self.assistant, 200.0, 700.0, 300.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_keeps_theme() {
        let mut l = Layout { theme: ThemeKind::Light, ..Layout::default() };
        l.browser.open = false;
        l.browser.dock = Dock::Floating;
        l.reset();
        assert_eq!(l.theme, ThemeKind::Light);
        assert!(l.browser.open);
        assert_eq!(l.browser.dock, Dock::Docked);
    }

    #[test]
    fn stored_layout_is_sanitised() {
        let mut l: Layout = serde_json::from_str(r#"{"browser":{"open":true,"dock":"Docked","size":1e30}}"#).unwrap();
        l.sanitize();
        assert_eq!(l.browser.size, 700.0);
        assert!(!l.text_commands.open, "missing fields fall back to defaults");
        let mut l: Layout = serde_json::from_str(r#"{"timeline":{"open":true,"dock":"Docked","size":-5}}"#).unwrap();
        l.sanitize();
        assert_eq!(l.timeline.size, 36.0);
    }
}
