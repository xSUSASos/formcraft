//! Ground grid on the XY plane with adaptive spacing, and clipped projection of 3D segments.

use glam::{DVec2, DVec3, DVec4};

use crate::camera::Camera;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Minor,
    Major,
    AxisX,
    AxisY,
}

/// Grid spacing for the current zoom: a power of ten so that ~10–100 minor cells are visible.
pub fn spacing(cam: &Camera) -> f64 {
    let h = cam.half_height().max(1e-6);
    10f64.powf((h / 10.0).log10().floor()).clamp(1e-3, 1e6)
}

/// Grid lines around the camera target: (from, to, kind). Major lines every 10 minor cells;
/// the lines through the origin are the X and Y axes.
pub fn lines(cam: &Camera) -> Vec<(DVec3, DVec3, LineKind)> {
    let s = spacing(cam);
    let major = s * 10.0;
    let half = major * 5.0;
    let cx = (cam.target.x / major).round() * major;
    let cy = (cam.target.y / major).round() * major;
    let n = (half / s).round() as i64;
    let mut out = Vec::with_capacity((4 * n + 4) as usize);
    for k in -n..=n {
        let off = k as f64 * s;
        for vertical in [true, false] {
            let (a, b, coord) = if vertical {
                let x = cx + off;
                (DVec3::new(x, cy - half, 0.0), DVec3::new(x, cy + half, 0.0), x)
            } else {
                let y = cy + off;
                (DVec3::new(cx - half, y, 0.0), DVec3::new(cx + half, y, 0.0), y)
            };
            let kind = if coord.abs() < s * 1e-6 {
                if vertical { LineKind::AxisY } else { LineKind::AxisX }
            } else if (coord / major - (coord / major).round()).abs() < 1e-6 {
                LineKind::Major
            } else {
                LineKind::Minor
            };
            out.push((a, b, kind));
        }
    }
    out
}

/// Project a world segment to screen pixels (y down), clipping it against the near plane.
pub fn project_segment(cam: &Camera, a: DVec3, b: DVec3, size: DVec2) -> Option<(DVec2, DVec2)> {
    let vp = cam.view_proj(size.x / size.y.max(1.0));
    let mut ca = vp * a.extend(1.0);
    let mut cb = vp * b.extend(1.0);
    // Clip against z >= 0 (wgpu depth range) and w > 0.
    let da = ca.z;
    let db = cb.z;
    if da < 0.0 && db < 0.0 {
        return None;
    }
    let lerp = |p: DVec4, q: DVec4, t: f64| p + (q - p) * t;
    if da < 0.0 {
        ca = lerp(ca, cb, da / (da - db));
    } else if db < 0.0 {
        cb = lerp(cb, ca, db / (db - da));
    }
    let to_px = |c: DVec4| {
        if c.w.abs() < 1e-12 {
            return None;
        }
        let n = c.truncate() / c.w;
        Some(DVec2::new((n.x + 1.0) * 0.5 * size.x, (1.0 - n.y) * 0.5 * size.y))
    };
    Some((to_px(ca)?, to_px(cb)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spacing_follows_zoom() {
        let mut c = Camera::home();
        let s1 = spacing(&c);
        c.zoom(20.0, None);
        assert!(spacing(&c) < s1);
        c.zoom(-20.0, None);
        c.zoom(-20.0, None);
        assert!(spacing(&c) > s1);
    }

    #[test]
    fn axes_are_present_when_origin_is_near() {
        let c = Camera::home();
        let l = lines(&c);
        assert!(l.iter().any(|x| x.2 == LineKind::AxisX));
        assert!(l.iter().any(|x| x.2 == LineKind::AxisY));
        assert!(l.iter().any(|x| x.2 == LineKind::Major));
    }

    #[test]
    fn segment_behind_camera_is_clipped() {
        let c = Camera::home();
        let size = DVec2::new(800.0, 600.0);
        let eye = c.eye();
        let behind = eye - c.forward() * 10.0;
        assert!(project_segment(&c, behind, behind - c.forward(), size).is_none());
        let (p, q) = project_segment(&c, behind, c.target, size).unwrap();
        assert!(p.is_finite() && q.is_finite());
    }
}
