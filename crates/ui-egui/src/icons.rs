//! Toolbar and browser icons, drawn in code (no image assets, nothing from Autodesk).
//!
//! Spec: `docs/icons.md` (UI_SPEC.md §4). Flat two-colour style: outline `t.icon_outline` plus one
//! accent by category (`accent_of`). `paint_state(painter, rect, id, state, tokens)` is the
//! primary entry; `paint(painter, rect, id, color, tokens)` keeps the older call shape where
//! `color` is the outline colour. Unknown ids fall back to a rounded square with the first
//! letter, so a plugin icon id never breaks the UI.

use std::f32::consts::{FRAC_PI_4, PI};

use egui::{Color32, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2, pos2};

use crate::theme::Tokens;

/// Visual state of an icon. Hover and Pressed use the Normal colours (the button draws the
/// background); Disabled greys everything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconState {
    Normal,
    Hover,
    Pressed,
    Disabled,
}

/// Accent colour of an icon by category; `None` for service icons (outline only).
pub fn accent_of(id: &str, t: &Tokens) -> Option<Color32> {
    match id {
        "box" | "cylinder" | "sphere" | "extrude" | "revolve" | "hole" | "press_pull" | "fillet" | "chamfer"
        | "shell" | "combine" | "move" | "parameters" | "measure" | "section" | "properties" => Some(t.icon_create),
        "sketch" | "line" | "rectangle" | "circle" | "arc" | "dimension" | "c_coincident" | "c_horizontal"
        | "c_vertical" | "c_parallel" | "c_perpendicular" | "c_tangent" | "c_equal" | "c_concentric" | "c_midpoint"
        | "c_fix" | "c_point_on_curve" => Some(t.icon_sketch),
        "finish" => Some(t.ok),
        "plane" | "axis" | "point" => Some(t.icon_construct),
        "ai" => Some(t.ai),
        _ => None,
    }
}

/// Points of an ellipse (unit-square coordinates), rotated by `rot` degrees, from angle `a0` to
/// `a1` degrees.
fn ell(cx: f32, cy: f32, rx: f32, ry: f32, rot: f32, a0: f32, a1: f32) -> Vec<(f32, f32)> {
    let n = (((a1 - a0).abs() / 10.0).ceil() as usize).clamp(4, 48);
    let (sr, cr) = rot.to_radians().sin_cos();
    // a full turn must not repeat its first point (zero-length segment → miter spike)
    let last = if (a1 - a0).abs() >= 360.0 { n - 1 } else { n };
    (0..=last)
        .map(|k| {
            let a = (a0 + (a1 - a0) * k as f32 / n as f32).to_radians();
            let (x, y) = (rx * a.cos(), ry * a.sin());
            (cx + x * cr - y * sr, cy + x * sr + y * cr)
        })
        .collect()
}

type P2 = (f32, f32);

struct Ic<'a> {
    p: &'a Painter,
    r: Rect,
    o: Color32,
    w: f32,
    small: bool,
}

impl Ic<'_> {
    fn q(&self, x: f32, y: f32) -> Pos2 {
        pos2(self.r.left() + x * self.r.width(), self.r.top() + y * self.r.height())
    }

    fn st(&self, c: Color32) -> Stroke {
        Stroke::new(self.w, c)
    }

    fn pts(&self, pts: &[P2]) -> Vec<Pos2> {
        pts.iter().map(|&(x, y)| self.q(x, y)).collect()
    }

    fn ln(&self, pts: &[P2], c: Color32, closed: bool) {
        self.lnw(pts, c, 1.0, closed);
    }

    fn lnw(&self, pts: &[P2], c: Color32, k: f32, closed: bool) {
        let s = Stroke::new(self.w * k, c);
        if closed {
            self.p.add(egui::Shape::closed_line(self.pts(pts), s));
        } else {
            self.p.add(egui::Shape::line(self.pts(pts), s));
        }
    }

    /// Filled convex polygon.
    fn fill(&self, pts: &[P2], c: Color32) {
        self.p.add(egui::Shape::convex_polygon(self.pts(pts), c, Stroke::NONE));
    }

    fn seg(&self, a: P2, b: P2, c: Color32) {
        self.p.line_segment([self.q(a.0, a.1), self.q(b.0, b.1)], self.st(c));
    }

    fn dot(&self, x: f32, y: f32, k: f32, c: Color32) {
        self.p.circle_filled(self.q(x, y), (self.w * k).max(1.2), c);
    }

    fn rect(&self, x0: f32, y0: f32, x1: f32, y1: f32, c: Color32, fill: Option<Color32>) {
        let rc = Rect::from_min_max(self.q(x0, y0), self.q(x1, y1));
        if let Some(f) = fill {
            self.p.rect_filled(rc, 1.0, f);
        }
        self.p.rect_stroke(rc, 1.0, self.st(c), StrokeKind::Inside);
    }

    /// Filled triangular arrow head at `tip`, pointing away from `from`.
    fn head(&self, tip: P2, from: P2, size: f32, c: Color32) {
        let (dx, dy) = (tip.0 - from.0, tip.1 - from.1);
        let len = (dx * dx + dy * dy).sqrt().max(1e-4);
        let (ux, uy) = (dx / len, dy / len);
        let (bx, by) = (tip.0 - ux * size, tip.1 - uy * size);
        let (px, py) = (-uy * size * 0.6, ux * size * 0.6);
        self.fill(&[tip, (bx + px, by + py), (bx - px, by - py)], c);
    }

    fn arrow(&self, from: P2, to: P2, c: Color32) {
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let len = (dx * dx + dy * dy).sqrt().max(1e-4);
        let hs = if self.small { 0.26 } else { 0.22 };
        let back = (to.0 - dx / len * hs * 0.6, to.1 - dy / len * hs * 0.6);
        self.seg(from, back, c);
        self.head(to, from, hs, c);
    }

    fn dashed(&self, a: P2, b: P2, c: Color32, n: usize) {
        for k in 0..n {
            let (t0, t1) = (k as f32 / n as f32, (k as f32 + 0.55) / n as f32);
            let at = |t: f32| (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            self.seg(at(t0), at(t1), c);
        }
    }

    /// Isometric cube in the box `(ox, oy, sx, sy)` of the unit square.
    fn cube(&self, ox: f32, oy: f32, sx: f32, sy: f32, top_fill: Option<Color32>) {
        let m = |v: &[P2]| -> Vec<P2> { v.iter().map(|&(x, y)| (ox + x * sx, oy + y * sy)).collect() };
        let top = m(&[(0.5, 0.0), (1.0, 0.25), (0.5, 0.5), (0.0, 0.25)]);
        let left = m(&[(0.0, 0.25), (0.5, 0.5), (0.5, 1.0), (0.0, 0.75)]);
        let right = m(&[(0.5, 0.5), (1.0, 0.25), (1.0, 0.75), (0.5, 1.0)]);
        self.fill(&right, self.o.gamma_multiply(0.14));
        self.fill(&left, self.o.gamma_multiply(0.06));
        if let Some(f) = top_fill {
            self.fill(&top, f);
        }
        self.ln(&top, self.o, true);
        self.ln(&left, self.o, true);
        self.ln(&right, self.o, true);
    }

    fn lens(&self, cx: f32, cy: f32, rad: f32) {
        self.ln(&ell(cx, cy, rad, rad, 0.0, 0.0, 360.0), self.o, true);
        let d = rad * 0.72;
        self.lnw(&[(cx + d, cy + d), (0.9, 0.9)], self.o, 1.5, false);
    }
}

fn draw(p: &Painter, rect: Rect, id: &str, o: Color32, acc: Option<Color32>, t: &Tokens, disabled: bool) {
    let w = (rect.width() / 16.0).clamp(1.0, 2.0);
    let r = rect.shrink(rect.width() * 0.1);
    let a = acc.unwrap_or(o);
    let i = Ic { p, r, o, w, small: rect.width() < 20.0 };
    let af = a.gamma_multiply(0.22);
    let of = o.gamma_multiply(0.14);
    let small = i.small;
    match id {
        // ---- sketch ----
        "sketch" => {
            let sheet = [(0.04, 0.82), (0.28, 0.2), (0.96, 0.2), (0.72, 0.82)];
            i.fill(&sheet, of);
            i.ln(&sheet, o, true);
            i.ln(&[(0.3, 0.68), (0.56, 0.3), (0.74, 0.68)], a, true);
        }
        "line" => {
            i.seg((0.15, 0.85), (0.85, 0.15), o);
            i.dot(0.15, 0.85, 1.7, a);
            i.dot(0.85, 0.15, 1.7, a);
        }
        "rectangle" => {
            i.rect(0.12, 0.25, 0.88, 0.75, o, Some(of));
            for (x, y) in [(0.12, 0.25), (0.88, 0.25), (0.12, 0.75), (0.88, 0.75)] {
                i.dot(x, y, 1.5, a);
            }
        }
        "circle" => {
            i.ln(&ell(0.5, 0.5, 0.42, 0.42, 0.0, 0.0, 360.0), o, true);
            i.dot(0.5, 0.5, 1.7, a);
            if !small {
                i.seg((0.5, 0.5), (0.8, 0.2), a);
            }
        }
        "arc" => {
            i.ln(&ell(0.5, 0.85, 0.42, 0.7, 0.0, 195.0, 345.0), o, false);
            for (x, y) in [(0.09, 0.4), (0.5, 0.15), (0.91, 0.4)] {
                i.dot(x, y, 1.6, a);
            }
        }
        "dimension" => {
            i.seg((0.1, 0.12), (0.1, 0.8), o);
            i.seg((0.9, 0.12), (0.9, 0.8), o);
            i.seg((0.1, 0.62), (0.9, 0.62), a);
            i.head((0.1, 0.62), (0.4, 0.62), 0.2, a);
            i.head((0.9, 0.62), (0.6, 0.62), 0.2, a);
        }
        "finish" => {
            let col = if disabled { o } else { t.ok };
            p.circle_filled(i.r.center(), i.r.width() * 0.5, col);
            i.lnw(&[(0.27, 0.52), (0.43, 0.68), (0.74, 0.34)], Color32::WHITE, 1.4, false);
        }
        "c_coincident" => {
            i.seg((0.1, 0.85), (0.5, 0.5), o);
            i.seg((0.9, 0.85), (0.5, 0.5), o);
            i.dot(0.5, 0.5, 2.2, a);
            if !small {
                i.ln(&ell(0.5, 0.5, 0.2, 0.2, 0.0, 0.0, 360.0), a, true);
            }
        }
        "c_horizontal" => {
            i.seg((0.1, 0.82), (0.9, 0.82), o);
            i.seg((0.28, 0.12), (0.28, 0.6), a);
            i.seg((0.72, 0.12), (0.72, 0.6), a);
            i.seg((0.28, 0.36), (0.72, 0.36), a);
        }
        "c_vertical" => {
            i.seg((0.18, 0.1), (0.18, 0.9), o);
            i.ln(&[(0.4, 0.15), (0.65, 0.78), (0.9, 0.15)], a, false);
        }
        "c_parallel" => {
            i.seg((0.12, 0.88), (0.55, 0.12), o);
            i.seg((0.45, 0.88), (0.88, 0.12), a);
            if !small {
                i.seg((0.3, 0.52), (0.37, 0.58), a);
            }
        }
        "c_perpendicular" => {
            i.seg((0.12, 0.85), (0.88, 0.85), o);
            i.seg((0.4, 0.85), (0.4, 0.12), o);
            i.ln(&[(0.4, 0.62), (0.6, 0.62), (0.6, 0.85)], a, false);
        }
        "c_tangent" => {
            i.ln(&ell(0.5, 0.62, 0.3, 0.3, 0.0, 0.0, 360.0), o, true);
            i.seg((0.05, 0.32), (0.95, 0.32), a);
            i.dot(0.5, 0.32, 1.5, a);
        }
        "c_equal" => {
            i.lnw(&[(0.2, 0.36), (0.8, 0.36)], o, 1.4, false);
            i.lnw(&[(0.2, 0.64), (0.8, 0.64)], a, 1.4, false);
        }
        "c_concentric" => {
            i.ln(&ell(0.5, 0.5, 0.42, 0.42, 0.0, 0.0, 360.0), o, true);
            i.ln(&ell(0.5, 0.5, 0.22, 0.22, 0.0, 0.0, 360.0), o, true);
            i.dot(0.5, 0.5, 1.5, a);
        }
        "c_midpoint" => {
            i.seg((0.08, 0.72), (0.92, 0.72), o);
            i.dot(0.08, 0.72, 1.3, o);
            i.dot(0.92, 0.72, 1.3, o);
            i.fill(&[(0.5, 0.2), (0.68, 0.46), (0.5, 0.72), (0.32, 0.46)], af);
            i.ln(&[(0.5, 0.2), (0.68, 0.46), (0.5, 0.72), (0.32, 0.46)], a, true);
        }
        "c_fix" => {
            i.rect(0.2, 0.46, 0.8, 0.92, o, Some(of));
            let mut sh = vec![(0.3, 0.46)];
            sh.extend(ell(0.5, 0.36, 0.2, 0.26, 0.0, 180.0, 360.0));
            sh.push((0.7, 0.46));
            i.ln(&sh, o, false);
            i.dot(0.5, 0.69, 1.6, a);
        }
        "c_point_on_curve" => {
            i.ln(&ell(0.5, 0.5, 0.42, 0.34, 0.0, 200.0, 340.0), o, false);
            i.seg((0.08, 0.62), (0.4, 0.36), o);
            i.seg((0.6, 0.36), (0.92, 0.62), o);
            i.dot(0.5, 0.31, 2.0, a);
            if !small {
                i.ln(&ell(0.5, 0.31, 0.12, 0.12, 0.0, 0.0, 360.0), a, true);
            }
        }
        // ---- solid: create ----
        "box" => i.cube(0.08, 0.08, 0.84, 0.84, Some(af)),
        "cylinder" => {
            let top = ell(0.5, 0.24, 0.38, 0.15, 0.0, 0.0, 360.0);
            i.fill(&top, af);
            i.seg((0.12, 0.24), (0.12, 0.78), o);
            i.seg((0.88, 0.24), (0.88, 0.78), o);
            i.ln(&ell(0.5, 0.78, 0.38, 0.15, 0.0, 0.0, 180.0), o, false);
            i.ln(&top, a, true);
        }
        "sphere" => {
            i.fill(&ell(0.5, 0.5, 0.42, 0.42, 0.0, 0.0, 360.0), of);
            i.ln(&ell(0.5, 0.5, 0.42, 0.42, 0.0, 0.0, 360.0), o, true);
            i.ln(&ell(0.5, 0.5, 0.42, 0.15, 0.0, 0.0, 360.0), a, true);
        }
        "extrude" => {
            i.fill(&[(0.5, 0.62), (0.92, 0.8), (0.5, 0.98), (0.08, 0.8)], af);
            i.ln(&[(0.5, 0.62), (0.92, 0.8), (0.5, 0.98), (0.08, 0.8)], o, true);
            i.seg((0.08, 0.8), (0.08, 0.5), o);
            i.seg((0.92, 0.8), (0.92, 0.5), o);
            i.seg((0.5, 0.98), (0.5, 0.68), o);
            i.arrow((0.5, 0.62), (0.5, 0.02), a);
        }
        "revolve" => {
            i.dashed((0.5, 0.02), (0.5, 0.98), o, 6);
            let prof = [(0.6, 0.12), (0.84, 0.12), (0.84, 0.58), (0.6, 0.58)];
            i.fill(&prof, af);
            i.ln(&prof, o, true);
            let arc = ell(0.5, 0.74, 0.4, 0.17, 0.0, 170.0, 20.0);
            i.ln(&arc, a, false);
            if let [.., prev, last] = arc.as_slice() {
                i.head(*last, *prev, 0.2, a);
            }
        }
        "hole" => {
            let top = [(0.5, 0.22), (0.94, 0.4), (0.5, 0.58), (0.06, 0.4)];
            i.fill(&[(0.5, 0.58), (0.94, 0.4), (0.94, 0.62), (0.5, 0.8)], of);
            i.ln(&top, o, true);
            i.ln(&[(0.06, 0.4), (0.06, 0.62), (0.5, 0.8), (0.94, 0.62), (0.94, 0.4)], o, false);
            i.seg((0.5, 0.58), (0.5, 0.8), o);
            let hole = ell(0.5, 0.4, 0.2, 0.09, 0.0, 0.0, 360.0);
            i.fill(&hole, a.gamma_multiply(0.55));
            i.ln(&hole, a, true);
            if !small {
                i.dashed((0.5, 0.02), (0.5, 0.2), a, 2);
                i.dashed((0.5, 0.84), (0.5, 0.98), a, 2);
            }
        }
        // ---- solid: modify ----
        "press_pull" => {
            i.cube(0.0, 0.18, 0.74, 0.74, Some(af));
            i.arrow((0.9, 0.5), (0.9, 0.04), a);
            i.arrow((0.9, 0.5), (0.9, 0.96), a);
        }
        "fillet" | "chamfer" => {
            let mut blk = vec![(0.1, 0.9), (0.1, 0.1), (0.5, 0.1)];
            let edge: Vec<P2> =
                if id == "fillet" { ell(0.5, 0.5, 0.4, 0.4, 0.0, -90.0, 0.0) } else { vec![(0.5, 0.1), (0.9, 0.5)] };
            blk.extend(edge.iter().copied());
            blk.push((0.9, 0.9));
            i.fill(&blk, of);
            i.ln(&[(0.9, 0.5), (0.9, 0.9), (0.1, 0.9), (0.1, 0.1), (0.5, 0.1)], o, false);
            i.lnw(&edge, a, 1.4, false);
        }
        "shell" => {
            i.cube(0.08, 0.08, 0.84, 0.84, None);
            let inner = [(0.5, 0.2), (0.78, 0.31), (0.5, 0.42), (0.22, 0.31)];
            i.fill(&inner, a.gamma_multiply(0.4));
            i.ln(&inner, a, true);
            i.seg((0.22, 0.31), (0.22, 0.52), a);
            i.seg((0.5, 0.42), (0.5, 0.63), a);
        }
        "combine" => {
            let (a0, b0) = ((0.08, 0.08, 0.62, 0.62), (0.38, 0.38, 0.92, 0.92));
            let x = Rect::from_min_max(i.q(b0.0, b0.1), i.q(a0.2, a0.3));
            p.rect_filled(x, 0.0, a.gamma_multiply(0.55));
            i.rect(a0.0, a0.1, a0.2, a0.3, o, None);
            i.rect(b0.0, b0.1, b0.2, b0.3, o, None);
            p.rect_stroke(x, 0.0, i.st(a), StrokeKind::Inside);
        }
        "move" => {
            i.cube(0.3, 0.3, 0.4, 0.4, Some(af));
            i.arrow((0.5, 0.28), (0.5, 0.02), a);
            i.arrow((0.5, 0.72), (0.5, 0.98), a);
            i.arrow((0.28, 0.5), (0.02, 0.5), a);
            i.arrow((0.72, 0.5), (0.98, 0.5), a);
        }
        "parameters" => {
            for (y, x) in [(0.2, 0.3), (0.5, 0.68), (0.8, 0.45)] {
                i.seg((0.08, y), (0.92, y), o);
                let knob = ell(x, y, 0.11, 0.11, 0.0, 0.0, 360.0);
                let hot = y == 0.5;
                i.fill(&knob, if hot { a } else { af });
                i.ln(&knob, if hot { a } else { o }, true);
            }
        }
        // ---- construction ----
        "plane" => {
            let pl = [(0.04, 0.76), (0.28, 0.26), (0.96, 0.26), (0.72, 0.76)];
            i.fill(&pl, af);
            i.ln(&pl, o, true);
        }
        "axis" => {
            i.lnw(&[(0.1, 0.9), (0.9, 0.1)], a, 1.3, false);
            i.dot(0.1, 0.9, 1.7, o);
            i.dot(0.9, 0.1, 1.7, o);
        }
        "point" => {
            i.dot(0.5, 0.5, 2.4, a);
            for (d0, d1) in [
                ((0.5, 0.08), (0.5, 0.3)),
                ((0.5, 0.7), (0.5, 0.92)),
                ((0.08, 0.5), (0.3, 0.5)),
                ((0.7, 0.5), (0.92, 0.5)),
            ] {
                i.seg(d0, d1, o);
            }
        }
        // ---- inspection ----
        "measure" => {
            let ruler = Rect::from_min_max(i.q(0.04, 0.12), i.q(0.96, 0.54));
            p.rect_filled(ruler, 1.0, of);
            p.rect_stroke(ruler, 1.0, i.st(o), StrokeKind::Inside);
            let n = if small { 4 } else { 6 };
            for k in 1..n {
                let x = 0.04 + 0.92 * k as f32 / n as f32;
                i.seg((x, 0.12), (x, if k % 2 == 0 { 0.4 } else { 0.3 }), o);
            }
            i.seg((0.1, 0.8), (0.9, 0.8), a);
            i.head((0.1, 0.8), (0.4, 0.8), 0.2, a);
            i.head((0.9, 0.8), (0.6, 0.8), 0.2, a);
        }
        "section" => {
            i.cube(0.1, 0.06, 0.8, 0.88, None);
            let cut = [(0.5, 0.3), (0.99, 0.5), (0.5, 0.7), (0.01, 0.5)];
            i.fill(&cut, a.gamma_multiply(0.32));
            i.ln(&cut, a, true);
        }
        "properties" => {
            i.rect(0.2, 0.06, 0.8, 0.94, o, Some(of));
            i.dot(0.5, 0.24, 1.3, a);
            i.lnw(&[(0.5, 0.35), (0.5, 0.52)], a, 1.3, false);
            i.seg((0.32, 0.7), (0.68, 0.7), o);
            if !small {
                i.seg((0.32, 0.84), (0.6, 0.84), o);
            }
        }
        // ---- ai ----
        "ai" => {
            let star = |cx: f32, cy: f32, k: f32| {
                let v: Vec<P2> = (0..8)
                    .map(|n| {
                        let ang = n as f32 * FRAC_PI_4 - PI / 2.0;
                        let rr = if n % 2 == 0 { k } else { k * 0.28 };
                        (cx + rr * ang.cos(), cy + rr * ang.sin())
                    })
                    .collect();
                for n in 0..8 {
                    i.fill(&[(cx, cy), v[n], v[(n + 1) % 8]], a);
                }
            };
            star(0.4, 0.58, 0.4);
            star(0.8, 0.2, 0.17);
        }
        // ---- service ----
        "eye" | "eye_off" => {
            let mut lens: Vec<P2> =
                (0..=12).map(|k| (0.04 + 0.92 * k as f32 / 12.0, 0.5 - 0.3 * (PI * k as f32 / 12.0).sin())).collect();
            lens.extend(
                (0..=12).rev().map(|k| (0.04 + 0.92 * k as f32 / 12.0, 0.5 + 0.3 * (PI * k as f32 / 12.0).sin())),
            );
            i.ln(&lens, o, true);
            i.p.circle_filled(i.q(0.5, 0.5), i.r.width() * 0.15, o);
            if id == "eye_off" {
                i.lnw(&[(0.14, 0.9), (0.86, 0.1)], o, 1.3, false);
            }
        }
        "folder" => {
            let f = [(0.06, 0.2), (0.4, 0.2), (0.5, 0.32), (0.94, 0.32), (0.94, 0.82), (0.06, 0.82)];
            i.fill(&f, of);
            i.ln(&f, o, true);
        }
        "doc" => {
            let page = [(0.2, 0.06), (0.62, 0.06), (0.82, 0.26), (0.82, 0.94), (0.2, 0.94)];
            i.fill(&page, of);
            i.ln(&page, o, true);
            i.ln(&[(0.62, 0.06), (0.62, 0.26), (0.82, 0.26)], o, false);
            if !small {
                i.seg((0.32, 0.5), (0.68, 0.5), o);
                i.seg((0.32, 0.66), (0.68, 0.66), o);
                i.seg((0.32, 0.8), (0.55, 0.8), o);
            }
        }
        "undo" | "redo" => {
            let flip = if id == "redo" { -1.0 } else { 1.0 };
            let arc: Vec<P2> = (0..=10)
                .map(|k| {
                    let ang = PI * (0.1 + 0.8 * k as f32 / 10.0);
                    (0.5 + flip * 0.36 * ang.cos(), 0.7 - 0.4 * ang.sin())
                })
                .collect();
            i.ln(&arc, o, false);
            if let [.., prev, last] = arc.as_slice() {
                i.head((last.0 + (last.0 - prev.0) * 1.6, last.1 + (last.1 - prev.1) * 1.6), *prev, 0.3, o);
            }
        }
        "save" => {
            let body = [(0.1, 0.1), (0.78, 0.1), (0.9, 0.22), (0.9, 0.9), (0.1, 0.9)];
            i.fill(&body, of);
            i.ln(&body, o, true);
            i.rect(0.28, 0.1, 0.68, 0.36, o, None);
            i.rect(0.26, 0.58, 0.74, 0.9, o, None);
        }
        "menu" => {
            for y in [0.25, 0.5, 0.75] {
                i.lnw(&[(0.14, y), (0.86, y)], o, 1.2, false);
            }
        }
        "search" => i.lens(0.42, 0.42, 0.3),
        "settings" => {
            let mut g: Vec<P2> = Vec::new();
            for n in 0..8 {
                let c = n as f32 * FRAC_PI_4;
                for (ang, rr) in [(-14.0f32, 0.34f32), (-9.0, 0.48), (9.0, 0.48), (14.0, 0.34)] {
                    let aa = c + ang.to_radians();
                    g.push((0.5 + rr * aa.cos(), 0.5 + rr * aa.sin()));
                }
            }
            i.ln(&g, o, true);
            i.ln(&ell(0.5, 0.5, 0.15, 0.15, 0.0, 0.0, 360.0), o, true);
        }
        "help" => {
            i.ln(&ell(0.5, 0.5, 0.44, 0.44, 0.0, 0.0, 360.0), o, true);
            let mut q = ell(0.5, 0.38, 0.13, 0.12, 0.0, 190.0, 450.0);
            q.push((0.5, 0.64));
            i.ln(&q, o, false);
            i.dot(0.5, 0.76, 1.1, o);
        }
        "close" => {
            i.seg((0.22, 0.22), (0.78, 0.78), o);
            i.seg((0.78, 0.22), (0.22, 0.78), o);
        }
        "plus" => {
            i.seg((0.5, 0.2), (0.5, 0.8), o);
            i.seg((0.2, 0.5), (0.8, 0.5), o);
        }
        "dropdown" => i.fill(&[(0.2, 0.36), (0.8, 0.36), (0.5, 0.72)], o),
        "expand" => i.fill(&[(0.36, 0.2), (0.72, 0.5), (0.36, 0.8)], o),
        "float" | "dock" => {
            i.rect(0.08, 0.3, 0.7, 0.9, o, Some(of));
            let (f, to) = if id == "float" { ((0.45, 0.6), (0.92, 0.1)) } else { ((0.92, 0.1), (0.5, 0.55)) };
            i.arrow(f, to, o);
        }
        // ---- navigation ----
        "orbit" => {
            i.ln(&ell(0.5, 0.5, 0.46, 0.2, -30.0, 0.0, 360.0), o, true);
            let sphere = ell(0.5, 0.5, 0.17, 0.17, 0.0, 0.0, 360.0);
            i.fill(&sphere, o.gamma_multiply(0.35));
            i.ln(&sphere, o, true);
            i.head((0.92, 0.2), (0.82, 0.3), 0.22, o);
        }
        "pan" => {
            for (dx, dy) in [(0.0f32, -1.0f32), (0.0, 1.0), (-1.0, 0.0), (1.0, 0.0)] {
                i.arrow((0.5 + dx * 0.1, 0.5 + dy * 0.1), (0.5 + dx * 0.46, 0.5 + dy * 0.46), o);
            }
        }
        "zoom" => {
            i.lens(0.42, 0.42, 0.32);
            i.seg((0.28, 0.42), (0.56, 0.42), o);
            if !small {
                i.seg((0.42, 0.28), (0.42, 0.56), o);
            }
        }
        "fit" => {
            for (x, y, dx, dy) in [
                (0.08, 0.08, 1.0f32, 1.0f32),
                (0.92, 0.08, -1.0, 1.0),
                (0.08, 0.92, 1.0, -1.0),
                (0.92, 0.92, -1.0, -1.0),
            ] {
                i.ln(&[(x, y + dy * 0.26), (x, y), (x + dx * 0.26, y)], o, false);
                i.arrow((x + dx * 0.1, y + dy * 0.1), (x + dx * 0.3, y + dy * 0.3), o);
            }
        }
        "look_at" => {
            let lens: Vec<P2> = (0..=10)
                .map(|k| (0.15 + 0.7 * k as f32 / 10.0, 0.26 - 0.18 * (PI * k as f32 / 10.0).sin()))
                .chain(
                    (0..=10).rev().map(|k| (0.15 + 0.7 * k as f32 / 10.0, 0.26 + 0.18 * (PI * k as f32 / 10.0).sin())),
                )
                .collect();
            i.ln(&lens, o, true);
            i.p.circle_filled(i.q(0.5, 0.26), i.r.width() * 0.08, o);
            i.dashed((0.5, 0.48), (0.5, 0.74), o, 3);
            let pl = [(0.06, 0.94), (0.26, 0.68), (0.94, 0.68), (0.74, 0.94)];
            i.fill(&pl, of);
            i.ln(&pl, o, true);
        }
        "style" => {
            i.ln(&ell(0.5, 0.5, 0.42, 0.42, 0.0, 0.0, 360.0), o, true);
            i.fill(&ell(0.5, 0.5, 0.42, 0.42, 0.0, 90.0, 270.0), o.gamma_multiply(0.7));
        }
        "grid" => {
            i.rect(0.1, 0.1, 0.9, 0.9, o, None);
            for k in [0.367, 0.633] {
                i.seg((k, 0.1), (k, 0.9), o);
                i.seg((0.1, k), (0.9, k), o);
            }
        }
        "ortho" => {
            let front = [(0.08, 0.36), (0.66, 0.36), (0.66, 0.94), (0.08, 0.94)];
            i.fill(&front, of);
            i.ln(&front, o, true);
            i.ln(&[(0.08, 0.36), (0.34, 0.08), (0.92, 0.08), (0.66, 0.36)], o, false);
            i.ln(&[(0.92, 0.08), (0.92, 0.66), (0.66, 0.94)], o, false);
        }
        "viewports" => {
            i.fill(&[(0.5, 0.1), (0.9, 0.1), (0.9, 0.5), (0.5, 0.5)], o.gamma_multiply(0.3));
            i.rect(0.1, 0.1, 0.9, 0.9, o, None);
            i.seg((0.5, 0.1), (0.5, 0.9), o);
            i.seg((0.1, 0.5), (0.9, 0.5), o);
        }
        // ---- timeline ----
        "tl_start" | "tl_back" | "tl_play" | "tl_fwd" | "tl_end" => match id {
            "tl_start" => {
                i.lnw(&[(0.14, 0.22), (0.14, 0.78)], o, 1.3, false);
                i.fill(&[(0.58, 0.22), (0.58, 0.78), (0.26, 0.5)], o);
                i.fill(&[(0.9, 0.22), (0.9, 0.78), (0.58, 0.5)], o);
            }
            "tl_back" => {
                i.lnw(&[(0.2, 0.22), (0.2, 0.78)], o, 1.3, false);
                i.fill(&[(0.8, 0.22), (0.8, 0.78), (0.3, 0.5)], o);
            }
            "tl_play" => i.fill(&[(0.24, 0.16), (0.24, 0.84), (0.84, 0.5)], o),
            "tl_fwd" => {
                i.lnw(&[(0.8, 0.22), (0.8, 0.78)], o, 1.3, false);
                i.fill(&[(0.2, 0.22), (0.2, 0.78), (0.7, 0.5)], o);
            }
            _ => {
                i.lnw(&[(0.86, 0.22), (0.86, 0.78)], o, 1.3, false);
                i.fill(&[(0.1, 0.22), (0.1, 0.78), (0.42, 0.5)], o);
                i.fill(&[(0.42, 0.22), (0.42, 0.78), (0.74, 0.5)], o);
            }
        },
        // ---- tools ----
        "scripts" => {
            i.ln(
                &[(0.42, 0.1), (0.32, 0.16), (0.32, 0.42), (0.18, 0.5), (0.32, 0.58), (0.32, 0.84), (0.42, 0.9)],
                o,
                false,
            );
            i.ln(
                &[(0.58, 0.1), (0.68, 0.16), (0.68, 0.42), (0.82, 0.5), (0.68, 0.58), (0.68, 0.84), (0.58, 0.9)],
                o,
                false,
            );
        }
        "plugin" => {
            let mut pz: Vec<P2> = vec![(0.12, 0.36), (0.34, 0.36)];
            pz.extend(ell(0.46, 0.36, 0.12, 0.12, 0.0, 180.0, 360.0));
            pz.push((0.8, 0.36));
            pz.extend(ell(0.8, 0.52, 0.12, 0.12, 0.0, -90.0, 90.0));
            pz.extend([(0.8, 0.92), (0.12, 0.92)]);
            i.fill(&[(0.12, 0.36), (0.8, 0.36), (0.8, 0.92), (0.12, 0.92)], of);
            i.ln(&pz, o, true);
        }
        "console" => {
            i.rect(0.06, 0.16, 0.94, 0.84, o, Some(of));
            i.ln(&[(0.22, 0.36), (0.4, 0.5), (0.22, 0.64)], o, false);
            i.seg((0.5, 0.66), (0.76, 0.66), o);
        }
        _ => {
            i.p.rect_stroke(i.r.shrink(1.0), 3.0, i.st(o), StrokeKind::Inside);
            let letter = id.chars().find(|ch| ch.is_alphanumeric()).unwrap_or('?').to_ascii_uppercase();
            p.text(
                i.r.center(),
                egui::Align2::CENTER_CENTER,
                letter,
                egui::FontId::proportional(i.r.height() * 0.6),
                o,
            );
        }
    }
}

/// Draw icon `id` in `r` in the given state, colours from the tokens.
pub fn paint_state(p: &Painter, r: Rect, id: &str, state: IconState, t: &Tokens) {
    match state {
        IconState::Disabled => {
            let g = t.text_disabled.gamma_multiply(0.4);
            draw(p, r, id, g, Some(g), t, true);
        }
        _ => draw(p, r, id, t.icon_outline, accent_of(id, t), t, false),
    }
}

/// Older entry point: `color` is the outline colour, the accent comes from the id's category.
/// The disabled grey (or a translucent colour) switches to the Disabled look.
pub fn paint(p: &Painter, r: Rect, id: &str, color: Color32, t: &Tokens) {
    if color == t.text_disabled || color.a() < 200 {
        paint_state(p, r, id, IconState::Disabled, t);
    } else {
        draw(p, r, id, color, accent_of(id, t), t, false);
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
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, tip));
    let radius = crate::theme::SIZE.radius_button;
    let state = if !enabled {
        IconState::Disabled
    } else if resp.is_pointer_button_down_on() {
        IconState::Pressed
    } else if resp.hovered() {
        IconState::Hover
    } else {
        IconState::Normal
    };
    let bg = match state {
        IconState::Pressed => t.toolbar_pressed,
        IconState::Hover => t.toolbar_hover,
        _ if selected => t.toolbar_pressed,
        _ => Color32::TRANSPARENT,
    };
    ui.painter().rect_filled(rect, radius, bg);
    if selected {
        ui.painter().rect_stroke(rect, radius, Stroke::new(1.0, t.accent), StrokeKind::Inside);
    }
    paint_state(ui.painter(), rect.shrink(size * 0.06), id, state, t);
    resp.on_hover_text(tip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeKind;

    const CREATE: &[&str] = &[
        "box",
        "cylinder",
        "sphere",
        "extrude",
        "revolve",
        "hole",
        "press_pull",
        "fillet",
        "chamfer",
        "shell",
        "combine",
        "move",
        "parameters",
        "measure",
        "section",
        "properties",
    ];
    const SKETCH: &[&str] = &[
        "sketch",
        "line",
        "rectangle",
        "circle",
        "arc",
        "dimension",
        "c_coincident",
        "c_horizontal",
        "c_vertical",
        "c_parallel",
        "c_perpendicular",
        "c_tangent",
        "c_equal",
        "c_concentric",
        "c_midpoint",
        "c_fix",
        "c_point_on_curve",
    ];
    const CONSTRUCT: &[&str] = &["plane", "axis", "point"];
    const SERVICE: &[&str] = &[
        "eye",
        "eye_off",
        "folder",
        "doc",
        "undo",
        "redo",
        "save",
        "menu",
        "search",
        "settings",
        "help",
        "close",
        "plus",
        "dropdown",
        "expand",
        "dock",
        "float",
        "orbit",
        "pan",
        "zoom",
        "fit",
        "look_at",
        "style",
        "grid",
        "ortho",
        "viewports",
        "tl_start",
        "tl_back",
        "tl_play",
        "tl_fwd",
        "tl_end",
        "scripts",
        "plugin",
        "console",
    ];

    #[test]
    fn every_icon_paints() {
        let ctx = egui::Context::default();
        for kind in [ThemeKind::Light, ThemeKind::Dark] {
            let t = Tokens::get(kind);
            for id in
                CREATE.iter().chain(SKETCH).chain(CONSTRUCT).chain(SERVICE).chain(&["finish", "ai", "no_such_icon"])
            {
                for size in [16.0, 24.0, 32.0] {
                    for state in [IconState::Normal, IconState::Hover, IconState::Pressed, IconState::Disabled] {
                        let mut out = ctx.run_ui(Default::default(), |ui| {
                            let rc = Rect::from_min_size(pos2(4.0, 4.0), Vec2::splat(size));
                            paint_state(ui.painter(), rc, id, state, &t);
                            paint(ui.painter(), rc, id, t.text_primary, &t);
                            paint(ui.painter(), rc, id, t.text_disabled, &t);
                        });
                        out.textures_delta.clear();
                        assert!(!out.shapes.is_empty(), "{id} drew nothing");
                    }
                }
            }
        }
    }

    #[test]
    fn accents_by_category() {
        let t = Tokens::get(ThemeKind::Light);
        for id in CREATE {
            assert_eq!(accent_of(id, &t), Some(t.icon_create), "{id}");
        }
        for id in SKETCH {
            assert_eq!(accent_of(id, &t), Some(t.icon_sketch), "{id}");
        }
        for id in CONSTRUCT {
            assert_eq!(accent_of(id, &t), Some(t.icon_construct), "{id}");
        }
        assert_eq!(accent_of("ai", &t), Some(t.ai));
        for id in SERVICE {
            assert_eq!(accent_of(id, &t), None, "{id}");
        }
    }
}
