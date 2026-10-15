//! Rendering bodies to a PNG without a window (CLI `render`, MCP `render`).

use formcraft_kernel::{Aabb, DVec3, Kernel, Shape};

use crate::camera::{Camera, Projection};
use crate::gpu::Renderer;
use crate::scene::{self, GridColors, Item, Scene, linear};

#[derive(Debug, Clone)]
pub struct Options {
    pub width: u32,
    pub height: u32,
    /// Direction from the model to the eye (default: front-right-top isometric).
    pub view: DVec3,
    pub orthographic: bool,
    pub edges: bool,
    pub grid: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            width: 1024,
            height: 768,
            view: DVec3::new(1.0, -1.0, 1.0),
            orthographic: false,
            edges: true,
            grid: true,
        }
    }
}

/// Named views: iso, top, bottom, front, back, left, right.
pub fn view_dir(name: &str) -> Option<DVec3> {
    Some(match name.to_ascii_lowercase().as_str() {
        "iso" | "home" => DVec3::new(1.0, -1.0, 1.0),
        "top" => DVec3::Z,
        "bottom" => DVec3::NEG_Z,
        "front" => DVec3::NEG_Y,
        "back" => DVec3::Y,
        "right" => DVec3::X,
        "left" => DVec3::NEG_X,
        _ => return None,
    })
}

pub const MAX_SIZE: u32 = 4096;

/// Render `bodies` and return PNG bytes.
pub fn render_png(
    r: &mut Renderer,
    kernel: &dyn Kernel,
    bodies: &[(u64, Shape)],
    o: &Options,
) -> Result<Vec<u8>, String> {
    let (w, h) = (o.width.clamp(16, MAX_SIZE), o.height.clamp(16, MAX_SIZE));
    let mut bb = Aabb::empty();
    let mut keys = Vec::new();
    for (key, shape) in bodies {
        let mesh = formcraft_tess::mesh(kernel, shape, formcraft_tess::Quality::Display).map_err(|e| e.to_string())?;
        bb = bb.union(&mesh.bbox());
        let (v, i) = scene::mesh_vertices(&mesh, linear([0xb4, 0xbd, 0xc8, 0xff]), &|_| None);
        let e = scene::edge_vertices(&mesh, linear([0x20, 0x24, 0x2a, 0xff]), &|_| None);
        r.set_mesh(*key, &v, &i, &e);
        keys.push(*key);
    }
    r.retain(&keys);
    let mut cam = Camera::home();
    cam.look_from(o.view);
    cam.projection = if o.orthographic { Projection::Orthographic } else { Projection::Perspective };
    if bb.is_empty() {
        cam.fit(DVec3::splat(-50.0), DVec3::splat(50.0), w as f64 / h as f64);
    } else {
        cam.fit(bb.min, bb.max, w as f64 / h as f64);
    }
    let grid = GridColors {
        minor: linear([0x40, 0x48, 0x55, 0x1c]),
        major: linear([0x40, 0x48, 0x55, 0x40]),
        x: linear([0xd0, 0x40, 0x36, 0xff]),
        y: linear([0x3e, 0x9e, 0x2a, 0xff]),
        z: linear([0x2a, 0x6c, 0xd0, 0xff]),
    };
    let scene = Scene {
        camera: cam,
        background: linear([0xee, 0xf1, 0xf5, 0xff]),
        lines: scene::grid_lines(&cam, &grid, o.grid),
        items: keys
            .iter()
            .map(|k| Item { key: *k, visible: true, shaded: true, edges: o.edges, transparent: false })
            .collect(),
    };
    let px = r.render_rgba(&scene, [w, h]).map_err(|e| e.to_string())?;
    crate::encode_png(&px, w, h)
}
