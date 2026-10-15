//! Toolbar and browser icons, drawn in code (no image assets, nothing from Autodesk).
//!
//! `paint(painter, rect, id, color, tokens)` draws icon `id` inside `rect`. Unknown ids fall back
//! to a rounded square with the first letter, so a plugin icon id never breaks the UI.

use egui::{Color32, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2, pos2};

use crate::theme::Tokens;

/// Map a point in a 0..1 unit square to the rect.
fn u(r: Rect, x: f32, y: f32) -> Pos2 {
    pos2(r.left() + x * r.width(), r.top() + y * r.height())
}

fn poly(p: &Painter, r: Rect, pts: &[(f32, f32)], s: Stroke, closed: bool) {
    let v: Vec<Pos2> = pts.iter().map(|&(x, y)| u(r, x, y)).collect();
    if closed {
        p.add(egui::Shape::closed_line(v, s));
    } else {
        p.add(egui::Shape::line(v, s));
    }
}

/// Isometric box outline (used by several solid icons).
fn iso_box(p: &Painter, r: Rect, s: Stroke, fill: Option<Color32>) {
    let top = [(0.5, 0.12), (0.88, 0.3), (0.5, 0.48), (0.12, 0.3)];
    let front = [(0.12, 0.3), (0.5, 0.48), (0.5, 0.88), (0.12, 0.7)];
    let side = [(0.5, 0.48), (0.88, 0.3), (0.88, 0.7), (0.5, 0.88)];
    if let Some(f) = fill {
        for face in [&top[..], &front[..], &side[..]] {
            let v: Vec<Pos2> = face.iter().map(|&(x, y)| u(r, x, y)).collect();
            p.add(egui::Shape::convex_polygon(v, f, Stroke::NONE));
        }
    }
    poly(p, r, &top, s, true);
    poly(p, r, &front, s, true);
    poly(p, r, &side, s, true);
}

pub fn paint(p: &Painter, r: Rect, id: &str, color: Color32, t: &Tokens) {
    let s = Stroke::new((r.width() / 14.0).clamp(1.0, 2.0), color);
    let accent = Stroke::new(s.width, t.accent);
    let fill = color.gamma_multiply(0.18);
    let c = r.center();
    let rad = r.width().min(r.height()) * 0.5;
    match id {
        "box" => iso_box(p, r, s, Some(fill)),
        "extrude" => {
            iso_box(p, Rect::from_min_max(u(r, 0.0, 0.35), u(r, 1.0, 1.0)), s, Some(fill));
            p.arrow(u(r, 0.5, 0.42), Vec2::new(0.0, -0.38 * r.height()), accent);
        }
        "sketch" => {
            p.rect_stroke(r.shrink(rad * 0.2), 1.0, s, StrokeKind::Inside);
            poly(p, r, &[(0.22, 0.75), (0.5, 0.3), (0.78, 0.75)], accent, true);
        }
        "line" => {
            p.line_segment([u(r, 0.15, 0.85), u(r, 0.85, 0.15)], s);
            p.circle_filled(u(r, 0.15, 0.85), s.width * 1.5, t.accent);
            p.circle_filled(u(r, 0.85, 0.15), s.width * 1.5, t.accent);
        }
        "rectangle" => {
            p.rect_stroke(Rect::from_min_max(u(r, 0.12, 0.25), u(r, 0.88, 0.75)), 0.0, s, StrokeKind::Inside);
        }
        "circle" | "sphere" => {
            p.circle_stroke(c, rad * 0.78, s);
            if id == "sphere" {
                p.circle_filled(c, rad * 0.78, fill);
                let e = Rect::from_center_size(c, Vec2::new(rad * 1.56, rad * 0.5));
                p.add(egui::Shape::ellipse_stroke(e.center(), e.size() * 0.5, s));
            } else {
                p.circle_filled(c, s.width * 1.3, t.accent);
            }
        }
        "cylinder" | "hole" => {
            let top = Rect::from_center_size(u(r, 0.5, 0.25), Vec2::new(rad * 1.5, rad * 0.5));
            let bot = Rect::from_center_size(u(r, 0.5, 0.78), Vec2::new(rad * 1.5, rad * 0.5));
            p.add(egui::Shape::ellipse_stroke(top.center(), top.size() * 0.5, s));
            p.add(egui::Shape::ellipse_stroke(bot.center(), bot.size() * 0.5, s));
            p.line_segment([top.left_center(), bot.left_center()], s);
            p.line_segment([top.right_center(), bot.right_center()], s);
            if id == "hole" {
                p.add(egui::Shape::ellipse_filled(top.center(), top.size() * 0.3, t.accent));
            }
        }
        "revolve" => {
            p.line_segment([u(r, 0.5, 0.05), u(r, 0.5, 0.95)], Stroke::new(1.0, color.gamma_multiply(0.6)));
            p.add(egui::Shape::ellipse_stroke(c, Vec2::new(rad * 0.85, rad * 0.35), s));
            p.arrow(u(r, 0.2, 0.6), Vec2::new(0.25 * r.width(), 0.05 * r.height()), accent);
        }
        "fillet" => {
            poly(p, r, &[(0.15, 0.15), (0.15, 0.85), (0.85, 0.85)], s, false);
            let arc: Vec<(f32, f32)> = (0..=8)
                .map(|i| {
                    let a = std::f32::consts::FRAC_PI_2 * i as f32 / 8.0;
                    (0.15 + 0.45 * (1.0 - a.cos()), 0.85 - 0.45 * (1.0 - a.sin()))
                })
                .collect();
            poly(p, r, &arc, accent, false);
        }
        "chamfer" => {
            poly(p, r, &[(0.15, 0.15), (0.15, 0.85), (0.85, 0.85)], s, false);
            p.line_segment([u(r, 0.15, 0.45), u(r, 0.55, 0.85)], accent);
        }
        "ai" => {
            let star = |cx: f32, cy: f32, k: f32| {
                let pts: Vec<(f32, f32)> = (0..8)
                    .map(|i| {
                        let a = i as f32 * std::f32::consts::FRAC_PI_4;
                        let rr = if i % 2 == 0 { k } else { k * 0.32 };
                        (cx + rr * a.cos(), cy + rr * a.sin())
                    })
                    .collect();
                let v: Vec<Pos2> = pts.iter().map(|&(x, y)| u(r, x, y)).collect();
                p.add(egui::Shape::convex_polygon(v, t.ai, Stroke::NONE));
            };
            star(0.42, 0.55, 0.38);
            star(0.78, 0.22, 0.16);
        }
        "finish" => {
            p.circle_filled(c, rad * 0.8, t.ok);
            poly(p, r, &[(0.3, 0.52), (0.45, 0.67), (0.72, 0.36)], Stroke::new(s.width * 1.3, Color32::WHITE), false);
        }
        "eye" => {
            p.add(egui::Shape::ellipse_stroke(c, Vec2::new(rad * 0.9, rad * 0.5), s));
            p.circle_filled(c, rad * 0.28, color);
        }
        "folder" => {
            poly(p, r, &[(0.1, 0.25), (0.4, 0.25), (0.5, 0.35), (0.9, 0.35), (0.9, 0.8), (0.1, 0.8)], s, true);
        }
        "plane" => {
            poly(p, r, &[(0.1, 0.65), (0.45, 0.3), (0.9, 0.35), (0.55, 0.7)], s, true);
        }
        "axis" => {
            p.line_segment([u(r, 0.1, 0.9), u(r, 0.9, 0.1)], Stroke::new(s.width, color));
            p.circle_filled(c, s.width * 1.5, t.accent);
        }
        "undo" | "redo" => {
            let flip = if id == "redo" { -1.0 } else { 1.0 };
            let pt = |k: f32| {
                let a = std::f32::consts::PI * (0.15 + 0.85 * k);
                (0.5 + flip * 0.32 * a.cos(), 0.6 - 0.32 * a.sin())
            };
            let arc: Vec<(f32, f32)> = (0..=10).map(|i| pt(i as f32 / 10.0)).collect();
            poly(p, r, &arc, s, false);
            let (tx, ty) = pt(1.0);
            poly(p, r, &[(tx - 0.12, ty - 0.14), (tx, ty), (tx + 0.14, ty - 0.1)], s, false);
        }
        "dropdown" => {
            let v = vec![u(r, 0.2, 0.35), u(r, 0.8, 0.35), u(r, 0.5, 0.7)];
            p.add(egui::Shape::convex_polygon(v, color, Stroke::NONE));
        }
        "expand" => {
            let v = vec![u(r, 0.35, 0.2), u(r, 0.7, 0.5), u(r, 0.35, 0.8)];
            p.add(egui::Shape::convex_polygon(v, color, Stroke::NONE));
        }
        "close" => {
            p.line_segment([u(r, 0.25, 0.25), u(r, 0.75, 0.75)], s);
            p.line_segment([u(r, 0.75, 0.25), u(r, 0.25, 0.75)], s);
        }
        "float" | "dock" => {
            p.rect_stroke(Rect::from_min_max(u(r, 0.12, 0.32), u(r, 0.68, 0.88)), 1.0, s, StrokeKind::Inside);
            let (a, b) = if id == "float" { ((0.45, 0.55), (0.88, 0.12)) } else { ((0.88, 0.12), (0.45, 0.55)) };
            p.arrow(u(r, a.0, a.1), u(r, b.0, b.1) - u(r, a.0, a.1), s);
        }
        "style" => {
            p.circle_filled(c, rad * 0.75, fill);
            p.circle_stroke(c, rad * 0.75, s);
            let half = vec![u(r, 0.5, 0.125), u(r, 0.5, 0.875), u(r, 0.15, 0.5)];
            p.add(egui::Shape::convex_polygon(half, color.gamma_multiply(0.7), Stroke::NONE));
        }
        "grid" => {
            for k in [0.25, 0.5, 0.75] {
                p.line_segment([u(r, k, 0.1), u(r, k, 0.9)], Stroke::new(1.0, color));
                p.line_segment([u(r, 0.1, k), u(r, 0.9, k)], Stroke::new(1.0, color));
            }
        }
        "ortho" => {
            p.rect_stroke(Rect::from_min_max(u(r, 0.15, 0.32), u(r, 0.75, 0.88)), 0.0, s, StrokeKind::Inside);
            poly(p, r, &[(0.15, 0.32), (0.32, 0.12), (0.92, 0.12), (0.75, 0.32)], s, false);
            poly(p, r, &[(0.92, 0.12), (0.92, 0.68), (0.75, 0.88)], s, false);
        }
        "orbit" => {
            p.add(egui::Shape::ellipse_stroke(c, Vec2::new(rad * 0.85, rad * 0.4), s));
            p.circle_filled(c, rad * 0.22, color);
            p.arrow(u(r, 0.15, 0.55), Vec2::new(0.0, -0.18 * r.height()), s);
        }
        "pan" => {
            for (dx, dy) in [(0.0, -1.0), (0.0, 1.0), (-1.0, 0.0), (1.0, 0.0)] {
                p.arrow(c, Vec2::new(dx, dy) * rad * 0.8, s);
            }
        }
        "zoom" => {
            p.circle_stroke(u(r, 0.42, 0.42), rad * 0.5, s);
            p.line_segment([u(r, 0.6, 0.6), u(r, 0.88, 0.88)], Stroke::new(s.width * 1.5, color));
        }
        "fit" => {
            for (x, y, dx, dy) in
                [(0.12, 0.12, 1.0, 1.0), (0.88, 0.12, -1.0, 1.0), (0.12, 0.88, 1.0, -1.0), (0.88, 0.88, -1.0, -1.0)]
            {
                poly(p, r, &[(x, y + dy * 0.25), (x, y), (x + dx * 0.25, y)], s, false);
            }
            p.rect_stroke(Rect::from_min_max(u(r, 0.35, 0.35), u(r, 0.65, 0.65)), 0.0, s, StrokeKind::Inside);
        }
        "tl_start" | "tl_back" | "tl_play" | "tl_fwd" | "tl_end" => {
            let left = matches!(id, "tl_start" | "tl_back");
            let (a, b) = if left { (0.7, 0.3) } else { (0.3, 0.7) };
            let v = vec![u(r, a, 0.2), u(r, a, 0.8), u(r, b, 0.5)];
            p.add(egui::Shape::convex_polygon(v, color, Stroke::NONE));
            match id {
                "tl_start" => {
                    p.line_segment([u(r, 0.25, 0.2), u(r, 0.25, 0.8)], s);
                }
                "tl_end" => {
                    p.line_segment([u(r, 0.75, 0.2), u(r, 0.75, 0.8)], s);
                }
                "tl_fwd" => {
                    p.line_segment([u(r, 0.15, 0.2), u(r, 0.15, 0.8)], s);
                }
                _ => {}
            }
        }
        "doc" => {
            let v = vec![u(r, 0.5, 0.1), u(r, 0.9, 0.5), u(r, 0.5, 0.9), u(r, 0.1, 0.5)];
            p.add(egui::Shape::convex_polygon(v, t.accent, Stroke::NONE));
        }
        _ => {
            p.rect_stroke(r.shrink(1.0), 3.0, s, StrokeKind::Inside);
            let letter = id.chars().find(|ch| ch.is_alphanumeric()).unwrap_or('?').to_ascii_uppercase();
            p.text(c, egui::Align2::CENTER_CENTER, letter, egui::FontId::proportional(r.height() * 0.6), color);
        }
    }
}

/// A small clickable icon (panel headers, app bar, navigation bar).
pub fn button(
    ui: &mut egui::Ui,
    t: &Tokens,
    id: &str,
    tip: &str,
    size: f32,
    enabled: bool,
    selected: bool,
) -> egui::Response {
    let sense = if enabled { egui::Sense::click() } else { egui::Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), sense);
    if selected {
        ui.painter().rect_filled(rect, 3.0, t.selection.gamma_multiply(0.7));
    } else if enabled && resp.hovered() {
        ui.painter().rect_filled(rect, 3.0, t.panel_header);
    }
    let color = if enabled { t.text } else { t.text_dim.gamma_multiply(0.5) };
    paint(ui.painter(), rect.shrink(size * 0.18), id, color, t);
    resp.on_hover_text(tip)
}
