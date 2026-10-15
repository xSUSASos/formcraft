//! Tessellation helpers for rendering and STL: deflection policy on top of `Kernel::tessellate`.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use formcraft_kernel::{Aabb, Kernel, Mesh, Result, Shape};

/// Quality preset for triangulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    /// Interactive display.
    Display,
    /// Export (STL/3MF): finer.
    Export,
}

/// Chordal deflection for a body of this size: relative to the bounding-box diagonal, clamped
/// so tiny parts stay smooth and huge parts stay cheap.
pub fn deflection(bbox: &Aabb, q: Quality) -> f64 {
    let diag = if bbox.is_empty() { 1.0 } else { bbox.size().length() };
    let rel = match q {
        Quality::Display => 1.5e-3,
        Quality::Export => 4e-4,
    };
    (diag * rel).clamp(0.002, 2.0)
}

/// Triangulate a shape with the deflection policy.
pub fn mesh(kernel: &dyn Kernel, shape: &Shape, q: Quality) -> Result<Mesh> {
    let bbox = kernel.mass_props(shape)?.bbox;
    kernel.tessellate(shape, deflection(&bbox, q))
}

#[cfg(test)]
mod tests {
    use formcraft_kernel::DVec3;

    use super::*;

    #[test]
    fn deflection_scales_and_clamps() {
        let small = Aabb { min: DVec3::ZERO, max: DVec3::splat(0.1) };
        let big = Aabb { min: DVec3::ZERO, max: DVec3::splat(100_000.0) };
        assert_eq!(deflection(&small, Quality::Display), 0.002);
        assert_eq!(deflection(&big, Quality::Display), 2.0);
        let mid = Aabb { min: DVec3::ZERO, max: DVec3::new(100.0, 0.0, 0.0) };
        assert!(deflection(&mid, Quality::Export) < deflection(&mid, Quality::Display));
        assert_eq!(deflection(&Aabb::empty(), Quality::Display), 0.002);
    }
}
