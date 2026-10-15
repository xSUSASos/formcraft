//! Orbit camera for the canvas: Z is up, the camera orbits a target point.
//!
//! Fusion-style navigation: orbit (Shift + middle drag), pan (middle drag), zoom (wheel, towards
//! the cursor), fit, and the ViewCube's 26 standard views.

use glam::{DMat4, DVec2, DVec3, Mat4};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Projection {
    #[default]
    Perspective,
    Orthographic,
}

/// Orbit camera. `yaw` rotates around +Z (0 = looking along +Y), `pitch` is elevation
/// (+ = looking down from above).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Camera {
    pub target: DVec3,
    pub distance: f64,
    pub yaw: f64,
    pub pitch: f64,
    /// Vertical field of view in radians (perspective).
    pub fov_y: f64,
    pub projection: Projection,
}

pub const MIN_DISTANCE: f64 = 1e-3;
pub const MAX_DISTANCE: f64 = 1e7;
const MAX_PITCH: f64 = std::f64::consts::FRAC_PI_2 - 1e-4;

impl Default for Camera {
    fn default() -> Self {
        Camera::home()
    }
}

impl Camera {
    /// Home view: isometric-like, from front-right-top.
    pub fn home() -> Self {
        let mut c = Camera {
            target: DVec3::ZERO,
            distance: 300.0,
            yaw: 0.0,
            pitch: 0.0,
            fov_y: 35f64.to_radians(),
            projection: Projection::Perspective,
        };
        c.look_from(DVec3::new(1.0, -1.0, 1.0));
        c
    }

    /// Unit vector from the target to the eye.
    pub fn back(&self) -> DVec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        // Looking direction (eye → target) is (sy*cp, cy*cp, -sp); eye sits opposite.
        -DVec3::new(sy * cp, cy * cp, -sp)
    }

    pub fn eye(&self) -> DVec3 {
        self.target + self.back() * self.distance
    }

    pub fn forward(&self) -> DVec3 {
        -self.back()
    }

    pub fn right(&self) -> DVec3 {
        let r = self.forward().cross(DVec3::Z);
        if r.length_squared() < 1e-12 { DVec3::X } else { r.normalize() }
    }

    pub fn up(&self) -> DVec3 {
        self.right().cross(self.forward()).normalize()
    }

    /// Point the camera so that the eye lies in direction `dir` from the target.
    pub fn look_from(&mut self, dir: DVec3) {
        if !dir.is_finite() || dir.length_squared() < 1e-12 {
            return;
        }
        let f = -dir.normalize();
        self.pitch = (-f.z).asin().clamp(-MAX_PITCH, MAX_PITCH);
        // Straight down/up views keep a front-facing yaw (as the ViewCube top/bottom faces do).
        self.yaw = if f.x.abs() < 1e-9 && f.y.abs() < 1e-9 { 0.0 } else { f.x.atan2(f.y) };
    }

    pub fn view(&self) -> DMat4 {
        DMat4::look_at_rh(self.eye(), self.target, self.up())
    }

    /// Half-height of the view at the target in world units (orthographic extent).
    pub fn half_height(&self) -> f64 {
        self.distance * (self.fov_y * 0.5).tan()
    }

    /// wgpu-style projection (depth 0..1).
    pub fn proj(&self, aspect: f64) -> DMat4 {
        let aspect = if aspect.is_finite() && aspect > 1e-6 { aspect } else { 1.0 };
        let near = (self.distance * 1e-3).max(1e-4);
        let far = self.distance * 1e3 + 1e4;
        match self.projection {
            Projection::Perspective => DMat4::perspective_rh(self.fov_y, aspect, near, far),
            Projection::Orthographic => {
                let h = self.half_height();
                DMat4::orthographic_rh(-h * aspect, h * aspect, -h, h, -far, far)
            }
        }
    }

    pub fn view_proj(&self, aspect: f64) -> DMat4 {
        self.proj(aspect) * self.view()
    }

    pub fn view_proj_f32(&self, aspect: f64) -> Mat4 {
        self.view_proj(aspect).as_mat4()
    }

    /// Orbit by a screen drag in pixels.
    pub fn orbit(&mut self, delta_px: DVec2) {
        if !delta_px.is_finite() {
            return;
        }
        const RAD_PER_PX: f64 = 0.008;
        self.yaw += delta_px.x * RAD_PER_PX;
        self.pitch = (self.pitch + delta_px.y * RAD_PER_PX).clamp(-MAX_PITCH, MAX_PITCH);
    }

    /// Pan by a screen drag (pixels) in a viewport of `height_px` pixels.
    pub fn pan(&mut self, delta_px: DVec2, height_px: f64) {
        if !delta_px.is_finite() || height_px.is_nan() || height_px <= 1.0 {
            return;
        }
        let world_per_px = 2.0 * self.half_height() / height_px;
        self.target += (-self.right() * delta_px.x + self.up() * delta_px.y) * world_per_px;
    }

    /// Zoom by `steps` (wheel notches, + = in) keeping `anchor` (world point under the cursor)
    /// fixed on screen as far as possible.
    pub fn zoom(&mut self, steps: f64, anchor: Option<DVec3>) {
        if !steps.is_finite() {
            return;
        }
        let factor = 0.85f64.powf(steps.clamp(-20.0, 20.0));
        let new_distance = (self.distance * factor).clamp(MIN_DISTANCE, MAX_DISTANCE);
        if let Some(a) = anchor.filter(|a| a.is_finite()) {
            let k = 1.0 - new_distance / self.distance;
            self.target += (a - self.target) * k;
        }
        self.distance = new_distance;
    }

    /// Frame a bounding box (min, max) so it fills the view.
    pub fn fit(&mut self, min: DVec3, max: DVec3, aspect: f64) {
        if !(min.is_finite() && max.is_finite()) || min.x > max.x {
            return;
        }
        self.target = (min + max) * 0.5;
        let radius = ((max - min).length() * 0.5).max(1.0);
        let half_fov = (self.fov_y * 0.5).min((self.fov_y * 0.5).tan().atan2(1.0 / aspect.max(1e-3)));
        self.distance = (radius / half_fov.sin() * 1.1).clamp(MIN_DISTANCE, MAX_DISTANCE);
    }

    /// World ray (origin, direction) through a pixel of a viewport `size` (pixels, y down).
    pub fn ray(&self, pixel: DVec2, size: DVec2) -> (DVec3, DVec3) {
        let ndc = DVec2::new(pixel.x / size.x.max(1.0) * 2.0 - 1.0, 1.0 - pixel.y / size.y.max(1.0) * 2.0);
        let inv = self.view_proj(size.x / size.y.max(1.0)).inverse();
        let near = inv.project_point3(ndc.extend(0.0));
        let far = inv.project_point3(ndc.extend(1.0));
        let d = far - near;
        let d = if d.length_squared() > 0.0 { d.normalize() } else { self.forward() };
        (near, d)
    }

    /// Screen position (pixels, y down) of a world point, or None behind the camera.
    pub fn project(&self, p: DVec3, size: DVec2) -> Option<DVec2> {
        let clip = self.view_proj(size.x / size.y.max(1.0)) * p.extend(1.0);
        if clip.w <= 1e-12 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        Some(DVec2::new((ndc.x + 1.0) * 0.5 * size.x, (1.0 - ndc.y) * 0.5 * size.y))
    }
}

/// A ViewCube target: face, edge or corner, identified by the direction from the model to the
/// eye with components in {-1, 0, 1}.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CubeView(pub [i8; 3]);

impl CubeView {
    pub const FRONT: CubeView = CubeView([0, -1, 0]);
    pub const BACK: CubeView = CubeView([0, 1, 0]);
    pub const RIGHT: CubeView = CubeView([1, 0, 0]);
    pub const LEFT: CubeView = CubeView([-1, 0, 0]);
    pub const TOP: CubeView = CubeView([0, 0, 1]);
    pub const BOTTOM: CubeView = CubeView([0, 0, -1]);

    /// All 26 views (6 faces, 12 edges, 8 corners).
    pub fn all() -> Vec<CubeView> {
        let mut v = Vec::with_capacity(26);
        for x in -1..=1i8 {
            for y in -1..=1i8 {
                for z in -1..=1i8 {
                    if (x, y, z) != (0, 0, 0) {
                        v.push(CubeView([x, y, z]));
                    }
                }
            }
        }
        v
    }

    pub fn dir(&self) -> DVec3 {
        DVec3::new(self.0[0] as f64, self.0[1] as f64, self.0[2] as f64)
    }

    /// Faces are orthographic in Fusion's "perspective with ortho faces" mode.
    pub fn is_face(&self) -> bool {
        self.0.iter().filter(|c| **c != 0).count() == 1
    }

    pub fn label(&self) -> Option<&'static str> {
        Some(match self.0 {
            [0, -1, 0] => "FRONT",
            [0, 1, 0] => "BACK",
            [1, 0, 0] => "RIGHT",
            [-1, 0, 0] => "LEFT",
            [0, 0, 1] => "TOP",
            [0, 0, -1] => "BOTTOM",
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_looks_at_target_from_front_right_top() {
        let c = Camera::home();
        let e = c.eye();
        assert!(e.x > 0.0 && e.y < 0.0 && e.z > 0.0, "{e:?}");
        assert!((c.eye().distance(c.target) - c.distance).abs() < 1e-9);
        assert!(c.up().z > 0.0);
    }

    #[test]
    fn every_cube_view_points_the_eye_along_its_direction() {
        for v in CubeView::all() {
            let mut c = Camera::home();
            c.look_from(v.dir());
            let got = c.back();
            let want = v.dir().normalize();
            assert!(got.dot(want) > 0.999, "{v:?}: {got:?}");
            assert!(c.up().is_finite() && c.right().is_finite());
        }
    }

    #[test]
    fn top_view_has_y_up_on_screen() {
        let mut c = Camera::home();
        c.look_from(DVec3::Z);
        assert!(c.up().y > 0.99, "{:?}", c.up());
        assert!(c.right().x > 0.99);
    }

    #[test]
    fn project_and_ray_agree() {
        let c = Camera::home();
        let size = DVec2::new(800.0, 600.0);
        let p = DVec3::new(10.0, 20.0, 5.0);
        let s = c.project(p, size).unwrap();
        let (o, d) = c.ray(s, size);
        let closest = o + d * (p - o).dot(d);
        assert!(closest.distance(p) < 1e-6);
    }

    #[test]
    fn zoom_keeps_anchor_in_place() {
        let mut c = Camera::home();
        let size = DVec2::new(800.0, 600.0);
        let anchor = DVec3::new(30.0, -10.0, 0.0);
        let before = c.project(anchor, size).unwrap();
        c.zoom(3.0, Some(anchor));
        let after = c.project(anchor, size).unwrap();
        assert!(before.distance(after) < 1e-6, "{before:?} {after:?}");
    }

    #[test]
    fn pan_moves_target_in_screen_plane() {
        let mut c = Camera::home();
        let t = c.target;
        c.pan(DVec2::new(100.0, 0.0), 600.0);
        assert!((c.target - t).dot(c.forward()).abs() < 1e-9);
        assert!(c.target.distance(t) > 0.0);
    }

    #[test]
    fn hostile_input_is_ignored() {
        let mut c = Camera::home();
        let before = c;
        c.orbit(DVec2::new(f64::NAN, 1.0));
        c.pan(DVec2::splat(f64::INFINITY), 600.0);
        c.zoom(f64::NAN, None);
        c.look_from(DVec3::ZERO);
        c.fit(DVec3::splat(f64::NAN), DVec3::ONE, 1.0);
        assert_eq!(c, before);
        c.zoom(1e9, None);
        assert!(c.distance >= MIN_DISTANCE && c.distance.is_finite());
    }

    #[test]
    fn fit_contains_box() {
        let mut c = Camera::home();
        c.fit(DVec3::ZERO, DVec3::new(100.0, 50.0, 20.0), 1.5);
        let size = DVec2::new(1500.0, 1000.0);
        for x in [0.0, 100.0] {
            for y in [0.0, 50.0] {
                for z in [0.0, 20.0] {
                    let s = c.project(DVec3::new(x, y, z), size).unwrap();
                    assert!(s.x >= 0.0 && s.x <= size.x && s.y >= 0.0 && s.y <= size.y, "{s:?}");
                }
            }
        }
    }
}
