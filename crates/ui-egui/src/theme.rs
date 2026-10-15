//! Colour tokens for the dark and light themes. Every colour in the UI comes from here.

use egui::{Color32, CornerRadius, Stroke, Visuals};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeKind {
    #[default]
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy)]
pub struct Tokens {
    pub kind: ThemeKind,
    pub app_bar: Color32,
    pub toolbar: Color32,
    pub panel: Color32,
    pub panel_header: Color32,
    pub canvas_top: Color32,
    pub canvas_bottom: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub accent: Color32,
    pub hover: Color32,
    pub selection: Color32,
    pub separator: Color32,
    pub grid_minor: Color32,
    pub grid_major: Color32,
    pub axis_x: Color32,
    pub axis_y: Color32,
    pub axis_z: Color32,
    pub cube_face: Color32,
    pub cube_edge: Color32,
    pub cube_text: Color32,
    pub error: Color32,
    pub warning: Color32,
    pub ok: Color32,
    pub ai: Color32,
}

impl Tokens {
    pub fn get(kind: ThemeKind) -> Tokens {
        match kind {
            ThemeKind::Dark => Tokens {
                kind,
                app_bar: Color32::from_rgb(0x1e, 0x21, 0x26),
                toolbar: Color32::from_rgb(0x2a, 0x2e, 0x34),
                panel: Color32::from_rgb(0x25, 0x29, 0x2f),
                panel_header: Color32::from_rgb(0x30, 0x35, 0x3c),
                canvas_top: Color32::from_rgb(0x3b, 0x41, 0x4a),
                canvas_bottom: Color32::from_rgb(0x1f, 0x23, 0x29),
                text: Color32::from_rgb(0xe3, 0xe6, 0xea),
                text_dim: Color32::from_rgb(0x9a, 0xa1, 0xab),
                accent: Color32::from_rgb(0x3d, 0x9b, 0xe9),
                hover: Color32::from_rgb(0x4f, 0xb6, 0xff),
                selection: Color32::from_rgb(0x2f, 0x86, 0xd6),
                separator: Color32::from_rgb(0x3a, 0x40, 0x48),
                grid_minor: Color32::from_rgba_unmultiplied(0xc8, 0xd0, 0xda, 0x22),
                grid_major: Color32::from_rgba_unmultiplied(0xc8, 0xd0, 0xda, 0x50),
                axis_x: Color32::from_rgb(0xe0, 0x5a, 0x4f),
                axis_y: Color32::from_rgb(0x6c, 0xc0, 0x4a),
                axis_z: Color32::from_rgb(0x4a, 0x8c, 0xe8),
                cube_face: Color32::from_rgb(0xd5, 0xda, 0xe0),
                cube_edge: Color32::from_rgb(0x70, 0x78, 0x82),
                cube_text: Color32::from_rgb(0x3a, 0x40, 0x48),
                error: Color32::from_rgb(0xf0, 0x5a, 0x5a),
                warning: Color32::from_rgb(0xf0, 0xb4, 0x3c),
                ok: Color32::from_rgb(0x5c, 0xc8, 0x7a),
                ai: Color32::from_rgb(0xb4, 0x8c, 0xff),
            },
            ThemeKind::Light => Tokens {
                kind,
                app_bar: Color32::from_rgb(0xe9, 0xec, 0xf0),
                toolbar: Color32::from_rgb(0xf6, 0xf7, 0xf9),
                panel: Color32::from_rgb(0xfb, 0xfb, 0xfc),
                panel_header: Color32::from_rgb(0xed, 0xef, 0xf2),
                canvas_top: Color32::from_rgb(0xfa, 0xfb, 0xfd),
                canvas_bottom: Color32::from_rgb(0xd9, 0xde, 0xe5),
                text: Color32::from_rgb(0x22, 0x26, 0x2c),
                text_dim: Color32::from_rgb(0x6b, 0x73, 0x7d),
                accent: Color32::from_rgb(0x1f, 0x7a, 0xd1),
                hover: Color32::from_rgb(0x2b, 0x92, 0xf0),
                selection: Color32::from_rgb(0x1f, 0x6f, 0xc4),
                separator: Color32::from_rgb(0xd2, 0xd7, 0xde),
                grid_minor: Color32::from_rgba_unmultiplied(0x40, 0x48, 0x55, 0x1c),
                grid_major: Color32::from_rgba_unmultiplied(0x40, 0x48, 0x55, 0x40),
                axis_x: Color32::from_rgb(0xd0, 0x40, 0x36),
                axis_y: Color32::from_rgb(0x3e, 0x9e, 0x2a),
                axis_z: Color32::from_rgb(0x2a, 0x6c, 0xd0),
                cube_face: Color32::from_rgb(0xf2, 0xf4, 0xf7),
                cube_edge: Color32::from_rgb(0x8a, 0x93, 0x9e),
                cube_text: Color32::from_rgb(0x4a, 0x52, 0x5c),
                error: Color32::from_rgb(0xd0, 0x30, 0x30),
                warning: Color32::from_rgb(0xc8, 0x8a, 0x10),
                ok: Color32::from_rgb(0x2a, 0x9a, 0x4c),
                ai: Color32::from_rgb(0x7a, 0x4c, 0xe0),
            },
        }
    }

    pub fn visuals(&self) -> Visuals {
        let mut v = match self.kind {
            ThemeKind::Dark => Visuals::dark(),
            ThemeKind::Light => Visuals::light(),
        };
        v.panel_fill = self.panel;
        v.window_fill = self.panel;
        v.extreme_bg_color = self.app_bar;
        v.selection.bg_fill = self.selection;
        v.hyperlink_color = self.accent;
        v.override_text_color = Some(self.text);
        v.window_corner_radius = CornerRadius::same(6);
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, self.separator);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, self.hover);
        v
    }
}
