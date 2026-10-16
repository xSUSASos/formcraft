//! ViewCube: a small cube in the canvas corner that mirrors the camera orientation. Each
//! visible face is split into a 3×3 grid: the centre picks the face view, the borders the edge
//! views and the corners the corner views (26 views in total). A Home button sits beside it.

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2, pos2};
use formcraft_render::camera::{Camera, CubeView};
use glam::DVec3;

use crate::theme::Tokens;

/// Result of interacting with the cube this frame.
pub enum CubeAction {
    View(CubeView),
    Home,
}

const FACES: [(DVec3, DVec3, DVec3); 6] = [
    // (normal, in-face axis a, in-face axis b)
    (DVec3::X, DVec3::Y, DVec3::Z),
    (DVec3::NEG_X, DVec3::Y, DVec3::Z),
    (DVec3::Y, DVec3::X, DVec3::Z),
    (DVec3::NEG_Y, DVec3::X, DVec3::Z),
    (DVec3::Z, DVec3::X, DVec3::Y),
    (DVec3::NEG_Z, DVec3::X, DVec3::Y),
];

/// Border cells take this fraction of the half-size of a face.
const INNER: f64 = 0.6;

struct Proj {
    center: Pos2,
    scale: f32,
    right: DVec3,
    up: DVec3,
}

impl Proj {
    fn to_screen(&self, p: DVec3) -> Pos2 {
        self.center + Vec2::new(p.dot(self.right) as f32, -p.dot(self.up) as f32) * self.scale
    }
}

/// Cell polygon (in cube coordinates) for cell (i, j) ∈ {-1, 0, 1}² of a face.
fn cell(n: DVec3, a: DVec3, b: DVec3, i: i8, j: i8) -> [DVec3; 4] {
    let span = |k: i8| match k {
        -1 => (-1.0, -INNER),
        0 => (-INNER, INNER),
        _ => (INNER, 1.0),
    };
    let (a0, a1) = span(i);
    let (b0, b1) = span(j);
    [n + a * a0 + b * b0, n + a * a1 + b * b0, n + a * a1 + b * b1, n + a * a0 + b * b1]
}

fn view_of(n: DVec3, a: DVec3, b: DVec3, i: i8, j: i8) -> CubeView {
    let d = n + a * i as f64 + b * j as f64;
    CubeView([d.x.round() as i8, d.y.round() as i8, d.z.round() as i8])
}

fn inside(poly: &[Pos2], p: Pos2) -> bool {
    // Convex polygon test, either winding.
    let mut sign = 0.0f32;
    for k in 0..poly.len() {
        let (Some(a), Some(b)) = (poly.get(k), poly.get((k + 1) % poly.len())) else { return false };
        let c = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
        if c.abs() < 1e-6 {
            continue;
        }
        if sign == 0.0 {
            sign = c.signum();
        } else if c.signum() != sign {
            return false;
        }
    }
    true
}

/// Draw the cube in `area` (top-right corner of the canvas) and handle clicks.
pub fn show(ui: &mut Ui, area: Rect, cam: &Camera, t: &Tokens) -> Option<CubeAction> {
    let resp = ui.interact(area, ui.id().with("viewcube"), Sense::click());
    let painter = ui.painter_at(area);
    let proj = Proj {
        center: area.center() + Vec2::new(8.0, 6.0),
        scale: area.width() * 0.24,
        right: cam.right(),
        up: cam.up(),
    };
    let back = cam.back();
    let hover = resp.hover_pos();
    // translucent at rest, opaque under the pointer
    let alpha = if hover.is_some() { 1.0 } else { crate::theme::SIZE.viewcube_idle_alpha };
    let mut hovered: Option<(CubeView, [Pos2; 4])> = None;

    // Visible faces, far to near.
    let mut faces: Vec<_> = FACES.iter().filter(|(n, _, _)| n.dot(back) > 1e-3).collect();
    faces.sort_by(|x, y| x.0.dot(back).total_cmp(&y.0.dot(back)));
    for &&(n, a, b) in &faces {
        let shade = (0.75 + 0.25 * n.dot(back)) as f32;
        let quad = [n - a - b, n + a - b, n + a + b, n - a + b].map(|p| proj.to_screen(p));
        let fill = t.cube_face.gamma_multiply(shade);
        painter.add(Shape::convex_polygon(
            quad.to_vec(),
            Color32::from_rgb(fill.r(), fill.g(), fill.b()).gamma_multiply(alpha),
            Stroke::new(1.0, t.cube_edge.gamma_multiply(alpha)),
        ));
        for i in -1..=1i8 {
            for j in -1..=1i8 {
                let poly = cell(n, a, b, i, j).map(|p| proj.to_screen(p));
                if let Some(h) = hover
                    && inside(&poly, h)
                {
                    hovered = Some((view_of(n, a, b, i, j), poly));
                }
            }
        }
        let v = CubeView([n.x as i8, n.y as i8, n.z as i8]);
        if let Some(label) = v.label()
            && n.dot(back) > 0.35
        {
            let size = 9.0 + 2.5 * n.dot(back) as f32;
            painter.text(
                proj.to_screen(n),
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(size),
                t.cube_text.gamma_multiply(alpha),
            );
        }
    }
    if let Some((_, poly)) = &hovered {
        painter.add(Shape::convex_polygon(poly.to_vec(), t.accent.gamma_multiply(0.55), Stroke::NONE));
    }
    // Axis tripod at the bottom-left corner of the cube.
    let origin = proj.to_screen(DVec3::new(-1.0, -1.0, -1.0));
    for (d, c) in [(DVec3::X, t.axis_x), (DVec3::Y, t.axis_y), (DVec3::Z, t.axis_z)] {
        let tip = proj.to_screen(DVec3::new(-1.0, -1.0, -1.0) + d * 0.9);
        painter.line_segment([origin, tip], Stroke::new(2.0, c));
    }

    // Home button.
    let home = Rect::from_min_size(area.min + Vec2::new(2.0, 2.0), Vec2::splat(18.0));
    let home_resp = ui.interact(home, ui.id().with("viewcube_home"), Sense::click()).on_hover_text("Home");
    let hc = if home_resp.hovered() { t.accent_hover } else { t.text_secondary };
    let roof = [
        pos2(home.center().x, home.top() + 2.0),
        pos2(home.right() - 2.0, home.center().y),
        pos2(home.left() + 2.0, home.center().y),
    ];
    painter.add(Shape::convex_polygon(roof.to_vec(), hc, Stroke::NONE));
    painter.rect_filled(
        Rect::from_min_max(pos2(home.left() + 5.0, home.center().y), pos2(home.right() - 5.0, home.bottom() - 2.0)),
        0.0,
        hc,
    );
    if home_resp.clicked() {
        return Some(CubeAction::Home);
    }

    if resp.clicked()
        && let Some((v, _)) = hovered
    {
        return Some(CubeAction::View(v));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_cell_maps_to_one_of_26_views() {
        let all = CubeView::all();
        let mut seen = std::collections::HashSet::new();
        for (n, a, b) in FACES {
            for i in -1..=1 {
                for j in -1..=1 {
                    let v = view_of(n, a, b, i, j);
                    assert!(all.contains(&v), "{v:?}");
                    seen.insert(v);
                }
            }
        }
        assert_eq!(seen.len(), 26, "faces, edges and corners are all reachable");
    }

    #[test]
    fn point_in_quad() {
        let q = [pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(10.0, 10.0), pos2(0.0, 10.0)];
        assert!(inside(&q, pos2(5.0, 5.0)));
        assert!(!inside(&q, pos2(15.0, 5.0)));
    }
}
