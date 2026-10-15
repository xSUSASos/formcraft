//! 3D rendering of bodies, edges, previews and manipulators: camera, grid, projection helpers.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod camera;
pub mod grid;

pub use camera::{Camera, CubeView, Projection};
