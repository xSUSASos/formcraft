//! What to draw: camera, background, lines (grid, axes) and body meshes by key; vertex
//! builders that bake face/edge highlight colours into the vertex data.

use bytemuck::{Pod, Zeroable};
use formcraft_kernel::Mesh;

use crate::camera::Camera;
use crate::grid::{self, LineKind};

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct MeshVertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct LineVertex {
    pub pos: [f32; 3],
    pub color: [f32; 4],
}

/// One body in the scene.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Item {
    pub key: u64,
    pub visible: bool,
    pub shaded: bool,
    pub edges: bool,
    pub transparent: bool,
}

#[derive(Debug, Clone)]
pub struct Scene {
    pub camera: Camera,
    /// Linear RGBA clear colour.
    pub background: [f32; 4],
    pub lines: Vec<LineVertex>,
    pub items: Vec<Item>,
}

/// sRGB 0..255 to linear 0..1 (the render target is sRGB).
pub fn linear(c: [u8; 4]) -> [f32; 4] {
    let f = |v: u8| {
        let s = v as f32 / 255.0;
        if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
    };
    [f(c[0]), f(c[1]), f(c[2]), c[3] as f32 / 255.0]
}

/// Colours for the grid and axes.
#[derive(Debug, Clone, Copy)]
pub struct GridColors {
    pub minor: [f32; 4],
    pub major: [f32; 4],
    pub x: [f32; 4],
    pub y: [f32; 4],
    pub z: [f32; 4],
}

/// Grid on XY plus a short Z axis.
pub fn grid_lines(cam: &Camera, c: &GridColors, with_grid: bool) -> Vec<LineVertex> {
    let mut out = Vec::new();
    let mut push = |a: glam::DVec3, b: glam::DVec3, color: [f32; 4]| {
        out.push(LineVertex { pos: a.as_vec3().to_array(), color });
        out.push(LineVertex { pos: b.as_vec3().to_array(), color });
    };
    if with_grid {
        for (a, b, kind) in grid::lines(cam) {
            let color = match kind {
                LineKind::Minor => c.minor,
                LineKind::Major => c.major,
                LineKind::AxisX => c.x,
                LineKind::AxisY => c.y,
            };
            push(a, b, color);
        }
    }
    let len = grid::spacing(cam) * 10.0;
    push(glam::DVec3::ZERO, glam::DVec3::Z * len, c.z);
    out
}

/// Mesh vertices with a colour per face: `face_color(face)` overrides `base` (hover/selection).
pub fn mesh_vertices(
    m: &Mesh,
    base: [f32; 4],
    face_color: &dyn Fn(u32) -> Option<[f32; 4]>,
) -> (Vec<MeshVertex>, Vec<u32>) {
    // Faces own disjoint vertex ranges (the kernel emits vertices per face), so the colour of
    // a vertex is the colour of the face of any triangle that uses it.
    let mut color = vec![base; m.positions.len()];
    for (t, face) in m.indices.as_chunks::<3>().0.iter().zip(&m.triangle_face) {
        if let Some(c) = face_color(*face) {
            for i in t {
                if let Some(slot) = color.get_mut(*i as usize) {
                    *slot = c;
                }
            }
        }
    }
    let verts = m
        .positions
        .iter()
        .enumerate()
        .map(|(i, p)| MeshVertex {
            pos: *p,
            normal: m.normals.get(i).copied().unwrap_or([0.0, 0.0, 1.0]),
            color: color.get(i).copied().unwrap_or(base),
        })
        .collect();
    (verts, m.indices.clone())
}

/// Edge polylines as a line list, coloured per edge.
pub fn edge_vertices(m: &Mesh, base: [f32; 4], edge_color: &dyn Fn(u32) -> Option<[f32; 4]>) -> Vec<LineVertex> {
    let mut out = Vec::new();
    for e in &m.edges {
        let c = edge_color(e.edge).unwrap_or(base);
        for w in e.points.windows(2) {
            if let [a, b] = w {
                out.push(LineVertex { pos: *a, color: c });
                out.push(LineVertex { pos: *b, color: c });
            }
        }
    }
    out
}
