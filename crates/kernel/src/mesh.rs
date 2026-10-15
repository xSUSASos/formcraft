//! Triangle meshes produced by tessellation.

use glam::DVec3;
use serde::{Deserialize, Serialize};

use crate::Aabb;

/// Edge polyline for drawing and picking.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EdgePolyline {
    pub edge: u32,
    pub points: Vec<[f32; 3]>,
}

/// Triangulated shape. Triangles keep the index of the B-rep face they came from, so picking a
/// triangle picks a face.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Counter-clockwise (outward) triangles.
    pub indices: Vec<u32>,
    /// Face index for each triangle (`indices.len() / 3` entries).
    pub triangle_face: Vec<u32>,
    pub edges: Vec<EdgePolyline>,
}

impl Mesh {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    fn triangles(&self) -> impl Iterator<Item = [DVec3; 3]> + '_ {
        self.indices.as_chunks::<3>().0.iter().filter_map(|t| {
            let p = |i: u32| self.positions.get(i as usize).map(|v| DVec3::new(v[0] as f64, v[1] as f64, v[2] as f64));
            Some([p(t[0])?, p(t[1])?, p(t[2])?])
        })
    }

    /// Enclosed volume by the divergence theorem (positive for outward triangles).
    pub fn volume(&self) -> f64 {
        self.triangles().map(|[a, b, c]| a.dot(b.cross(c))).sum::<f64>() / 6.0
    }

    pub fn area(&self) -> f64 {
        self.triangles().map(|[a, b, c]| (b - a).cross(c - a).length() * 0.5).sum()
    }

    pub fn bbox(&self) -> Aabb {
        let mut b = Aabb::empty();
        for p in &self.positions {
            b.grow(DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64));
        }
        b
    }

    /// Append another mesh (indices and faces are offset; face ids are kept as given).
    pub fn append(&mut self, other: &Mesh) {
        let base = self.positions.len() as u32;
        self.positions.extend_from_slice(&other.positions);
        self.normals.extend_from_slice(&other.normals);
        self.indices.extend(other.indices.iter().map(|i| i + base));
        self.triangle_face.extend_from_slice(&other.triangle_face);
        self.edges.extend(other.edges.iter().cloned());
    }
}
