//! CPU picking: which body, face or edge is under the cursor.

use formcraft_kernel::Mesh;
use glam::{DVec2, DVec3};

use crate::camera::Camera;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hit {
    Face { key: u64, face: u32, depth: f64 },
    Edge { key: u64, edge: u32, depth: f64 },
}

impl Hit {
    pub fn key(&self) -> u64 {
        match *self {
            Hit::Face { key, .. } | Hit::Edge { key, .. } => key,
        }
    }
}

/// Möller–Trumbore; returns the ray parameter.
fn ray_triangle(o: DVec3, d: DVec3, a: DVec3, b: DVec3, c: DVec3) -> Option<f64> {
    let e1 = b - a;
    let e2 = c - a;
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-14 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 0.0).then_some(t)
}

fn v3(p: [f32; 3]) -> DVec3 {
    DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64)
}

fn dist_to_segment(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let ab = b - a;
    let t = if ab.length_squared() > 0.0 { ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
    p.distance(a + ab * t)
}

/// Pick at `pixel` (viewport pixels, y down) among `meshes` (key, mesh). Edges within
/// `edge_px` pixels win over faces, unless they are hidden behind a nearer face.
pub fn pick(cam: &Camera, pixel: DVec2, size: DVec2, meshes: &[(u64, &Mesh)], edge_px: f64) -> Option<Hit> {
    let (o, d) = cam.ray(pixel, size);
    let mut best_face: Option<Hit> = None;
    for (key, m) in meshes {
        for (t, face) in m.indices.as_chunks::<3>().0.iter().zip(&m.triangle_face) {
            let (Some(a), Some(b), Some(c)) =
                (m.positions.get(t[0] as usize), m.positions.get(t[1] as usize), m.positions.get(t[2] as usize))
            else {
                continue;
            };
            if let Some(depth) = ray_triangle(o, d, v3(*a), v3(*b), v3(*c))
                && best_face.is_none_or(|h| matches!(h, Hit::Face { depth: bd, .. } if depth < bd))
            {
                best_face = Some(Hit::Face { key: *key, face: *face, depth });
            }
        }
    }
    let face_depth = match best_face {
        Some(Hit::Face { depth, .. }) => depth,
        _ => f64::INFINITY,
    };
    let mut best_edge: Option<(f64, Hit)> = None;
    for (key, m) in meshes {
        for e in &m.edges {
            for w in e.points.windows(2) {
                let [a, b] = w else { continue };
                let (wa, wb) = (v3(*a), v3(*b));
                let (Some(sa), Some(sb)) = (cam.project(wa, size), cam.project(wb, size)) else { continue };
                let dpx = dist_to_segment(pixel, sa, sb);
                if dpx > edge_px {
                    continue;
                }
                let mid = (wa + wb) * 0.5;
                let depth = (mid - o).dot(d);
                // Hidden behind the picked face by more than a small tolerance? Skip.
                if depth > face_depth + face_depth.abs() * 1e-3 + 1e-3 {
                    continue;
                }
                if best_edge.is_none_or(|(bd, _)| dpx < bd) {
                    best_edge = Some((dpx, Hit::Edge { key: *key, edge: e.edge, depth }));
                }
            }
        }
    }
    best_edge.map(|(_, h)| h).or(best_face)
}

#[cfg(test)]
mod tests {
    use formcraft_kernel::EdgePolyline;

    use super::*;

    fn quad() -> Mesh {
        // A 10x10 square at z = 0 (face 0) with one edge along its bottom (edge 7).
        Mesh {
            positions: vec![[0., 0., 0.], [10., 0., 0.], [10., 10., 0.], [0., 10., 0.]],
            normals: vec![[0., 0., 1.]; 4],
            indices: vec![0, 1, 2, 0, 2, 3],
            triangle_face: vec![0, 0],
            edges: vec![EdgePolyline { edge: 7, points: vec![[0., 0., 0.], [10., 0., 0.]] }],
        }
    }

    #[test]
    fn face_and_edge_picking() {
        let mut cam = Camera::home();
        cam.target = DVec3::new(5.0, 5.0, 0.0);
        cam.distance = 40.0;
        cam.look_from(DVec3::Z);
        let size = DVec2::new(800.0, 600.0);
        let m = quad();
        let center = cam.project(DVec3::new(5.0, 5.0, 0.0), size).unwrap();
        assert!(matches!(pick(&cam, center, size, &[(3, &m)], 6.0), Some(Hit::Face { key: 3, face: 0, .. })));
        let on_edge = cam.project(DVec3::new(5.0, 0.0, 0.0), size).unwrap();
        assert!(matches!(
            pick(&cam, on_edge + DVec2::new(0.0, 2.0), size, &[(3, &m)], 6.0),
            Some(Hit::Edge { edge: 7, .. })
        ));
        assert!(pick(&cam, DVec2::new(1.0, 1.0), size, &[(3, &m)], 6.0).is_none());
    }
}
