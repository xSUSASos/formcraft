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
pub mod gpu;
pub mod grid;
pub mod pick;
pub mod scene;

pub use camera::{Camera, CubeView, Projection};

/// Encode RGBA8 pixels as PNG.
pub fn encode_png(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let img = image::RgbaImage::from_raw(width, height, rgba.to_vec()).ok_or("pixel buffer does not match the size")?;
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod gpu_tests;
