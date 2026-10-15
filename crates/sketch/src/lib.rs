//! 2D sketches: geometry, constraints and dimensions, the constraint solver and closed-region
//! (profile) detection.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod model;
pub mod profile;
pub mod solver;

use formcraft_kernel::{Plane, Profile, Region as KRegion};

pub use model::{Constraint, DimKind, Dimension, Eid, Entity, Sketch, SketchError};
pub use profile::{Region, default_regions, region_at, regions};
pub use solver::{SolveReport, solve};

/// Kernel profile from selected sketch regions on `plane`.
pub fn to_profile(plane: Plane, regions: &[&Region]) -> Profile {
    Profile {
        plane,
        regions: regions.iter().map(|r| KRegion { outer: r.outer.clone(), holes: r.holes.clone() }).collect(),
    }
}

#[cfg(test)]
mod tests;
