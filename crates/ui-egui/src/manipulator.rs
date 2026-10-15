//! On-canvas manipulators for the open feature dialog: a distance arrow (extrude, press pull)
//! and an angle ring (revolve). Dragging writes the new value into the dialog field, which
//! re-runs the live preview.

use egui::{Id, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};
use formcraft_kernel::{DVec2, DVec3};
use formcraft_render::camera::Camera;

use crate::theme::Tokens;

fn screen(cam: &Camera, rect: Rect, p: DVec3) -> Option<Pos2> {
    let size = DVec2::new(rect.width() as f64, rect.height() as f64);
    cam.project(p, size).map(|s| rect.min + Vec2::new(s.x as f32, s.y as f32))
}

/// Distance arrow from `origin` along `dir` (unit). Returns the new value while dragging.
pub fn arrow(ui: &mut Ui, rect: Rect, cam: &Camera, t: &Tokens, origin: DVec3, dir: DVec3, value: f64) -> Option<f64> {
    let p0 = screen(cam, rect, origin)?;
    let unit = screen(cam, rect, origin + dir)? - p0; // pixels per mm along the arrow
    let len_px = unit.length();
    if len_px.is_nan() || len_px <= 1e-4 {
        return None;
    }
    // The handle sits at the value, but never closer than 30 px to the base.
    let sign = if value < 0.0 { -1.0 } else { 1.0 };
    let shown = if (value.abs() * len_px as f64) < 30.0 { sign * 30.0 / len_px as f64 } else { value };
    let tip = p0 + unit * shown as f32;
    let handle = Rect::from_center_size(tip, Vec2::splat(18.0));
    let resp = ui.interact(handle, Id::new("manip_arrow"), Sense::drag());
    let hot = resp.hovered() || resp.dragged();
    let c = if hot { t.accent_hover } else { t.accent };
    let p = ui.painter_at(rect);
    p.line_segment([p0, tip], Stroke::new(2.5, c));
    let d = (tip - p0).normalized();
    let side = Vec2::new(-d.y, d.x) * 7.0;
    p.add(Shape::convex_polygon(vec![tip + d * 12.0, tip + side, tip - side], c, Stroke::NONE));
    p.circle_filled(p0, 3.5, c);
    if resp.dragged() {
        let delta = resp.drag_delta();
        let along = (delta.x * unit.x + delta.y * unit.y) / (len_px * len_px);
        let nv = value + along as f64;
        // Snap to 0.1 mm while dragging.
        return Some((nv * 10.0).round() / 10.0);
    }
    None
}

/// Angle ring around `axis` through `center`, radius `r` (mm). Returns the new angle (deg).
pub fn ring(
    ui: &mut Ui,
    rect: Rect,
    cam: &Camera,
    t: &Tokens,
    center: DVec3,
    axis: DVec3,
    r: f64,
    start: DVec3,
    deg: f64,
) -> Option<f64> {
    let a = axis.normalize_or_zero();
    let u = (start - a * start.dot(a)).normalize_or_zero();
    if u.length_squared() < 0.5 {
        return None;
    }
    let v = a.cross(u);
    let pt = |ang: f64| center + (u * ang.to_radians().cos() + v * ang.to_radians().sin()) * r;
    let pts: Vec<Pos2> = (0..=48).filter_map(|k| screen(cam, rect, pt(deg * k as f64 / 48.0))).collect();
    let p = ui.painter_at(rect);
    if pts.len() >= 2 {
        p.add(Shape::line(pts, Stroke::new(2.0, t.accent)));
    }
    let tip = screen(cam, rect, pt(deg))?;
    let handle = Rect::from_center_size(tip, Vec2::splat(18.0));
    let resp = ui.interact(handle, Id::new("manip_ring"), Sense::drag());
    let c = if resp.hovered() || resp.dragged() { t.accent_hover } else { t.accent };
    p.circle_filled(tip, 7.0, c);
    if resp.dragged() {
        // Angle of the pointer around the ring centre on screen.
        let cs = screen(cam, rect, center)?;
        let ptr = resp.interact_pointer_pos()?;
        let s0 = screen(cam, rect, pt(0.0))? - cs;
        let s90 = screen(cam, rect, pt(90.0))? - cs;
        let w = ptr - cs;
        // Solve w = s0·cos + s90·sin for the angle (screen basis of the ring plane).
        let det = s0.x * s90.y - s0.y * s90.x;
        if det.abs() < 1e-3 {
            return None;
        }
        let cosv = (w.x * s90.y - w.y * s90.x) / det;
        let sinv = (s0.x * w.y - s0.y * w.x) / det;
        let mut ang = (sinv as f64).atan2(cosv as f64).to_degrees();
        if ang <= 0.0 {
            ang += 360.0;
        }
        return Some(ang.round().clamp(1.0, 360.0));
    }
    None
}
