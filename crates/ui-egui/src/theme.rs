//! Design tokens (UI_SPEC.md §2–3): every colour and size of the UI comes from here. Widgets
//! never use literal colours or dimensions. Light theme is the default (§2.1).

use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Visuals};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeKind {
    Dark,
    #[default]
    Light,
}

const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// `rgba(r, g, b, a)` with a in 0..=1 (unmultiplied, as in the spec).
fn rgba(v: u32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied((v >> 16) as u8, (v >> 8) as u8, v as u8, (a.clamp(0.0, 1.0) * 255.0) as u8)
}

/// Blend `top` over `base` by `k` (opaque result; for 3D colours given as "colour N%").
pub fn mix(base: Color32, top: Color32, k: f32) -> Color32 {
    let k = k.clamp(0.0, 1.0);
    let m = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * k).round() as u8;
    Color32::from_rgb(m(base.r(), top.r()), m(base.g(), top.g()), m(base.b(), top.b()))
}

#[derive(Debug, Clone, Copy)]
pub struct Tokens {
    pub kind: ThemeKind,
    // --- chrome (§2.1) ---
    pub appbar_bg: Color32,
    pub appbar_fg: Color32,
    pub doc_tab_active: Color32,
    pub toolbar_bg: Color32,
    pub toolbar_fg: Color32,
    pub group_label: Color32,
    pub toolbar_hover: Color32,
    pub toolbar_pressed: Color32,
    pub divider: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub panel_bg: Color32,
    pub panel_shadow: Color32,
    pub panel_header: Color32,
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_disabled: Color32,
    pub canvas_top: Color32,
    pub canvas_bottom: Color32,
    pub grid_minor: Color32,
    pub grid_major: Color32,
    pub axis_x: Color32,
    pub axis_y: Color32,
    pub axis_z: Color32,
    pub timeline_bg: Color32,
    pub warning: Color32,
    pub error: Color32,
    /// Everything AI: ✦, AI groups, AI previews.
    pub ai: Color32,
    /// Positive actions (Finish Sketch, status dot "online").
    pub ok: Color32,
    // --- ViewCube (derived from the chrome palette) ---
    pub cube_face: Color32,
    pub cube_edge: Color32,
    pub cube_text: Color32,
    // --- 3D (§2.3) ---
    pub body: Color32,
    pub body_edge: Color32,
    pub hover_3d: Color32,
    pub selected_3d: Color32,
    pub preview_join: Color32,
    pub preview_cut: Color32,
    pub preview_new: Color32,
    pub origin_plane: Color32,
    pub hidden_edge: Color32,
    // --- sketch (§2.4) ---
    pub sketch_under: Color32,
    pub sketch_full: Color32,
    pub sketch_over: Color32,
    pub sketch_construction: Color32,
    pub sketch_profile: Color32,
    pub sketch_dimension: Color32,
    // --- icons (§4) ---
    pub icon_outline: Color32,
    pub icon_create: Color32,
    pub icon_sketch: Color32,
    pub icon_construct: Color32,
}

impl Tokens {
    pub fn get(kind: ThemeKind) -> Tokens {
        let light = Tokens {
            kind,
            appbar_bg: hex(0x2B2F33),
            appbar_fg: hex(0xE6E8EA),
            doc_tab_active: hex(0x3A3F44),
            toolbar_bg: hex(0xF2F3F4),
            toolbar_fg: hex(0x2E3236),
            group_label: hex(0x5F666D),
            toolbar_hover: hex(0xE1E5E9),
            toolbar_pressed: hex(0xCFD8E0),
            divider: hex(0xD5D9DD),
            accent: hex(0x1A8BD8),
            accent_hover: hex(0x3AA0E6),
            panel_bg: rgba(0xFFFFFF, 0.92),
            panel_shadow: rgba(0x000000, 0.18),
            panel_header: hex(0xE9ECEF),
            text_primary: hex(0x1F2326),
            text_secondary: hex(0x6B737A),
            text_disabled: hex(0xA9B0B6),
            canvas_top: hex(0xFFFFFF),
            canvas_bottom: hex(0xC9D3DC),
            grid_minor: rgba(0x6E7D8C, 0.18),
            grid_major: rgba(0x6E7D8C, 0.35),
            axis_x: hex(0xD9534F),
            axis_y: hex(0x5CB85C),
            axis_z: hex(0x2F7FD8),
            timeline_bg: hex(0xE8EBEE),
            warning: hex(0xE8A317),
            error: hex(0xD9372B),
            ai: hex(0x8A5CF6),
            ok: hex(0x2E9E5B),
            cube_face: hex(0xEEF0F2),
            cube_edge: hex(0x9AA2AA),
            cube_text: hex(0x4A5056),
            body: hex(0xB8BEC4),
            body_edge: hex(0x2A2E33),
            hover_3d: hex(0x5BB4F0),
            selected_3d: hex(0x1A8BD8),
            preview_join: hex(0x4C9BE8),
            preview_cut: hex(0xE2534A),
            preview_new: hex(0x58B368),
            origin_plane: hex(0xF0B429),
            hidden_edge: hex(0x7B848C),
            sketch_under: hex(0x1A8BD8),
            sketch_full: hex(0x1F2326),
            sketch_over: hex(0xD9372B),
            sketch_construction: hex(0xE07B39),
            sketch_profile: rgba(0xFFD580, 0.30),
            sketch_dimension: hex(0x2E3236),
            icon_outline: hex(0x4A5056),
            icon_create: hex(0x1A8BD8),
            icon_sketch: hex(0xE07B39),
            icon_construct: hex(0x2E9E5B),
        };
        match kind {
            ThemeKind::Light => light,
            // §2.2: listed overrides; accents stay the same.
            ThemeKind::Dark => Tokens {
                toolbar_bg: hex(0x2F3337),
                toolbar_fg: hex(0xE6E8EA),
                group_label: hex(0xA4ABB2),
                toolbar_hover: hex(0x3A3F45),
                toolbar_pressed: hex(0x45505B),
                panel_bg: rgba(0x2B2F33, 0.94),
                panel_header: hex(0x353A3F),
                text_primary: hex(0xE6E8EA),
                text_secondary: hex(0xA4ABB2),
                text_disabled: hex(0x6B737A),
                canvas_top: hex(0x4A5058),
                canvas_bottom: hex(0x262A2E),
                divider: hex(0x44494F),
                timeline_bg: hex(0x2B2F33),
                grid_minor: rgba(0xC8D0DA, 0.13),
                grid_major: rgba(0xC8D0DA, 0.30),
                sketch_full: hex(0xF2F3F4),
                sketch_dimension: hex(0xE6E8EA),
                icon_outline: hex(0xC4CAD0),
                cube_face: hex(0xD5DAE0),
                cube_edge: hex(0x70787F),
                ..light
            },
        }
    }

    pub fn visuals(&self) -> Visuals {
        let mut v = match self.kind {
            ThemeKind::Dark => Visuals::dark(),
            ThemeKind::Light => Visuals::light(),
        };
        let opaque = |c: Color32| Color32::from_rgb(c.r(), c.g(), c.b());
        v.panel_fill = self.toolbar_bg;
        v.window_fill = opaque(self.panel_bg);
        v.window_stroke = Stroke::new(1.0, self.divider);
        v.window_shadow = egui::Shadow { offset: [0, 2], blur: 12, spread: 0, color: self.panel_shadow };
        v.popup_shadow = v.window_shadow;
        v.extreme_bg_color = match self.kind {
            ThemeKind::Light => Color32::WHITE,
            ThemeKind::Dark => self.appbar_bg,
        };
        v.faint_bg_color = self.panel_header;
        v.selection.bg_fill = self.accent;
        v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
        v.hyperlink_color = self.accent;
        v.override_text_color = Some(self.text_primary);
        v.window_corner_radius = CornerRadius::same(SIZE.radius_card);
        v.menu_corner_radius = CornerRadius::same(SIZE.radius_card);
        for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            w.corner_radius = CornerRadius::same(SIZE.radius_button);
        }
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, self.divider);
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, self.text_primary);
        v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
        v.widgets.hovered.weak_bg_fill = self.toolbar_hover;
        v.widgets.hovered.bg_fill = self.toolbar_hover;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, self.accent_hover);
        v.widgets.active.weak_bg_fill = self.toolbar_pressed;
        v.widgets.active.bg_fill = self.toolbar_pressed;
        v.text_cursor.stroke = Stroke::new(1.5, self.accent);
        v
    }

    /// Text styles of §3 (sizes in points).
    pub fn apply_text_styles(style: &mut egui::Style) {
        let s = &SIZE;
        style.text_styles = [
            (TextStyle::Small, FontId::new(s.font_group, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(s.font_panel, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(s.font_panel, FontFamily::Proportional)),
            (TextStyle::Heading, FontId::new(s.font_heading, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(s.font_panel, FontFamily::Monospace)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(s.grid * 2.0, s.grid);
        style.spacing.button_padding = egui::vec2(s.grid * 1.5, s.grid * 0.75);
        style.spacing.interact_size.y = s.input_h;
    }
}

/// Sizes of UI_SPEC.md §3 and §5 (points). One instance: [`SIZE`].
#[derive(Debug, Clone, Copy)]
pub struct Sizes {
    /// Spacing grid (all paddings are multiples).
    pub grid: f32,
    pub radius_button: u8,
    pub radius_card: u8,
    pub radius_input: u8,
    pub font_toolbar: f32,
    pub font_group: f32,
    pub font_panel: f32,
    pub font_heading: f32,
    pub group_letter_spacing: f32,
    pub appbar_h: f32,
    pub tabs_h: f32,
    pub toolbar_h: f32,
    pub tab_underline: f32,
    pub icon_toolbar: f32,
    pub icon_menu: f32,
    pub icon_tree: f32,
    pub icon_nav: f32,
    pub icon_timeline: f32,
    pub card_margin: f32,
    pub browser_w: f32,
    pub tree_row_h: f32,
    pub tree_indent: f32,
    pub viewcube: f32,
    pub viewcube_idle_alpha: f32,
    pub nav_h: f32,
    pub nav_bottom: f32,
    pub timeline_h: f32,
    pub timeline_gap: f32,
    pub dialog_w: f32,
    pub dialog_label_w: f32,
    pub input_h: f32,
    pub manipulator_len: f32,
    pub marking_radius: f32,
    pub sbox_w: f32,
    pub assistant_w: f32,
    pub palette_w: f32,
    pub text_commands_h: f32,
    pub text_commands_strip: f32,
    pub edge_px: f32,
    pub edge_hover_px: f32,
    pub edge_selected_px: f32,
    /// Opacity of the selection tint over the body colour (§2.3: 45%).
    pub selected_mix: f32,
    pub hover_mix: f32,
    pub tooltip_delay: f32,
}

pub const SIZE: Sizes = Sizes {
    grid: 4.0,
    radius_button: 4,
    radius_card: 6,
    radius_input: 3,
    font_toolbar: 11.0,
    font_group: 10.0,
    font_panel: 12.0,
    font_heading: 12.0,
    group_letter_spacing: 0.4,
    appbar_h: 36.0,
    tabs_h: 28.0,
    toolbar_h: 68.0,
    tab_underline: 2.0,
    icon_toolbar: 32.0,
    icon_menu: 16.0,
    icon_tree: 16.0,
    icon_nav: 20.0,
    icon_timeline: 24.0,
    card_margin: 8.0,
    browser_w: 240.0,
    tree_row_h: 22.0,
    tree_indent: 16.0,
    viewcube: 100.0,
    viewcube_idle_alpha: 0.6,
    nav_h: 36.0,
    nav_bottom: 12.0,
    timeline_h: 40.0,
    timeline_gap: 2.0,
    dialog_w: 300.0,
    dialog_label_w: 110.0,
    input_h: 22.0,
    manipulator_len: 60.0,
    marking_radius: 90.0,
    sbox_w: 360.0,
    assistant_w: 320.0,
    palette_w: 240.0,
    text_commands_h: 160.0,
    text_commands_strip: 22.0,
    edge_px: 1.0,
    edge_hover_px: 2.0,
    edge_selected_px: 2.5,
    selected_mix: 0.45,
    hover_mix: 0.35,
    tooltip_delay: 0.5,
};

/// Name of the semibold family (card headings, §3).
pub const SEMIBOLD: &str = "inter-semibold";

/// Inter (SIL OFL 1.1, `assets/fonts/Inter-LICENSE.txt`) as the UI font, egui's default fonts
/// as fallback for symbols.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "inter".into(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!("../assets/fonts/Inter-Regular.ttf"))),
    );
    fonts.font_data.insert(
        SEMIBOLD.into(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!("../assets/fonts/Inter-SemiBold.ttf"))),
    );
    if let Some(list) = fonts.families.get_mut(&FontFamily::Proportional) {
        list.insert(0, "inter".into());
    }
    let mut semibold = vec![SEMIBOLD.to_string()];
    semibold.extend(fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default());
    fonts.families.insert(FontFamily::Name(SEMIBOLD.into()), semibold);
    ctx.set_fonts(fonts);
}

/// Heading font of cards and dialogs (semibold once the fonts are installed; fonts set with
/// `set_fonts` only apply from the next frame).
pub fn heading(ctx: &egui::Context) -> FontId {
    let family = FontFamily::Name(SEMIBOLD.into());
    let ready = ctx.fonts(|f| f.families().contains(&family));
    FontId::new(SIZE.font_heading, if ready { family } else { FontFamily::Proportional })
}
