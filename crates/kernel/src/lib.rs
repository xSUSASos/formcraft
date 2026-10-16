//! Geometry kernel abstraction: the [`Kernel`] trait, opaque [`Shape`]s, profiles, topology
//! queries and meshes. Nothing above this crate knows which B-rep library does the work
//! (docs/adr/0001-geometry-kernel.md).
//!
//! Units are millimetres, angles radians. Faces and edges of a shape are addressed by their
//! 0-based index in the kernel's topology order for *that* shape; indices are not stable across
//! regenerations — the regen layer resolves semantic selectors to indices each time.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod b64;
mod mesh;
mod profile;

use std::any::Any;
use std::path::Path;
use std::sync::Arc;

pub use glam::{DMat4, DVec2, DVec3};
pub use mesh::{EdgePolyline, Mesh};
pub use profile::{Loop2, Plane, Profile, Region, Segment2};
use serde::{Deserialize, Serialize};

pub type Result<T> = std::result::Result<T, KernelError>;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum KernelError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("{op} failed: {msg}")]
    Failed { op: &'static str, msg: String },
    #[error("not supported by this kernel: {0}")]
    Unsupported(&'static str),
    #[error("file error: {0}")]
    Io(String),
    #[error("shape belongs to another kernel")]
    ForeignShape,
}

impl KernelError {
    pub fn failed(op: &'static str, msg: impl Into<String>) -> Self {
        KernelError::Failed { op, msg: msg.into() }
    }
}

/// Opaque B-rep shape. Cheap to clone (shared, immutable); only its kernel can look inside.
#[derive(Clone)]
pub struct Shape {
    kernel: &'static str,
    inner: Arc<dyn Any + Send + Sync>,
}

impl Shape {
    pub fn new<T: Any + Send + Sync>(kernel: &'static str, value: T) -> Self {
        Shape { kernel, inner: Arc::new(value) }
    }
    pub fn kernel(&self) -> &'static str {
        self.kernel
    }
    /// The kernel's own representation, or [`KernelError::ForeignShape`].
    pub fn get<T: Any>(&self) -> Result<&T> {
        self.inner.downcast_ref::<T>().ok_or(KernelError::ForeignShape)
    }
    /// Whether two handles share the same underlying shape.
    pub fn same(&self, other: &Shape) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl std::fmt::Debug for Shape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Shape({})", self.kernel)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoolOp {
    Union,
    Cut,
    Intersect,
}

/// Axis-aligned bounding box.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Aabb {
    pub min: DVec3,
    pub max: DVec3,
}

impl Aabb {
    pub fn size(&self) -> DVec3 {
        self.max - self.min
    }
    pub fn center(&self) -> DVec3 {
        (self.min + self.max) * 0.5
    }
    pub fn empty() -> Self {
        Aabb { min: DVec3::splat(f64::INFINITY), max: DVec3::splat(f64::NEG_INFINITY) }
    }
    pub fn is_empty(&self) -> bool {
        self.min.x > self.max.x
    }
    pub fn grow(&mut self, p: DVec3) {
        self.min = self.min.min(p);
        self.max = self.max.max(p);
    }
    pub fn union(&self, o: &Aabb) -> Aabb {
        Aabb { min: self.min.min(o.min), max: self.max.max(o.max) }
    }
}

/// Geometry type of a face.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SurfaceKind {
    Plane { normal: DVec3 },
    Cylinder { origin: DVec3, axis: DVec3, radius: f64 },
    Cone { origin: DVec3, axis: DVec3 },
    Sphere { center: DVec3, radius: f64 },
    Torus { center: DVec3, axis: DVec3 },
    Other,
}

/// Geometry type of an edge.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CurveKind {
    Line { dir: DVec3 },
    Circle { center: DVec3, axis: DVec3, radius: f64 },
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FaceInfo {
    pub index: u32,
    pub kind: SurfaceKind,
    /// Centroid of the face.
    pub center: DVec3,
    /// Outward normal at the point of the face nearest to its centroid.
    pub normal: DVec3,
    pub area: f64,
    pub bbox: Aabb,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeInfo {
    pub index: u32,
    pub kind: CurveKind,
    pub start: DVec3,
    pub end: DVec3,
    pub mid: DVec3,
    pub length: f64,
    /// Indices of the faces that share this edge (usually two).
    pub faces: Vec<u32>,
}

impl EdgeInfo {
    pub fn is_closed(&self) -> bool {
        self.start.distance(self.end) < 1e-9
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MassProps {
    pub volume: f64,
    pub area: f64,
    pub centroid: DVec3,
    pub bbox: Aabb,
}

/// A B-rep modelling kernel. Every operation validates its input and returns an error instead
/// of panicking or aborting; shapes are immutable (operations return new shapes).
pub trait Kernel: Send + Sync {
    fn name(&self) -> &'static str;

    // --- primitives ---
    fn make_box(&self, min: DVec3, size: DVec3) -> Result<Shape>;
    fn make_cylinder(&self, base: DVec3, axis: DVec3, radius: f64, height: f64) -> Result<Shape>;
    fn make_cone(&self, base: DVec3, axis: DVec3, r1: f64, r2: f64, height: f64) -> Result<Shape>;
    fn make_sphere(&self, center: DVec3, radius: f64) -> Result<Shape>;
    fn make_torus(&self, center: DVec3, axis: DVec3, major: f64, minor: f64) -> Result<Shape>;

    // --- sweeps ---
    /// Planar face(s) of a profile (one face per region).
    fn make_face(&self, profile: &Profile) -> Result<Shape>;
    /// Extrude a profile along `vector` (its length is the distance).
    fn extrude(&self, profile: &Profile, vector: DVec3) -> Result<Shape>;
    /// Revolve a profile around an axis by `angle` radians (2π = full).
    fn revolve(&self, profile: &Profile, axis_origin: DVec3, axis_dir: DVec3, angle: f64) -> Result<Shape>;

    /// Prism of one face of a shape along `vector` (press pull).
    fn extrude_face(&self, shape: &Shape, face: u32, vector: DVec3) -> Result<Shape>;

    // --- modifications ---
    fn boolean(&self, op: BoolOp, a: &Shape, b: &Shape) -> Result<Shape>;
    fn fillet(&self, shape: &Shape, edges: &[u32], radius: f64) -> Result<Shape>;
    fn chamfer(&self, shape: &Shape, edges: &[u32], distance: f64) -> Result<Shape>;
    /// Hollow a solid: remove `open_faces` and keep walls of `thickness` (> 0, inward).
    fn shell(&self, shape: &Shape, open_faces: &[u32], thickness: f64) -> Result<Shape>;
    fn transform(&self, shape: &Shape, m: &DMat4) -> Result<Shape>;
    /// Several shapes as one (a compound); used for multi-body results and export.
    fn compound(&self, shapes: &[Shape]) -> Result<Shape>;
    /// The solids of a shape (a compound splits into its solids).
    fn solids(&self, shape: &Shape) -> Result<Vec<Shape>>;

    // --- queries ---
    fn faces(&self, shape: &Shape) -> Result<Vec<FaceInfo>>;
    fn edges(&self, shape: &Shape) -> Result<Vec<EdgeInfo>>;
    fn mass_props(&self, shape: &Shape) -> Result<MassProps>;
    fn is_valid(&self, shape: &Shape) -> bool;
    /// Triangulate with the given chordal deflection (mm).
    fn tessellate(&self, shape: &Shape, deflection: f64) -> Result<Mesh>;

    // --- exchange ---
    fn write_step(&self, shape: &Shape, path: &Path) -> Result<()>;
    fn read_step(&self, path: &Path) -> Result<Shape>;
    /// Kernel-native serialisation (frozen bodies, caches).
    fn serialize(&self, shape: &Shape) -> Result<Vec<u8>>;
    fn deserialize(&self, bytes: &[u8]) -> Result<Shape>;
}

/// Reject NaN/inf and non-positive values for lengths and radii.
pub fn check_positive(what: &str, v: f64) -> Result<f64> {
    if v.is_finite() && v > 0.0 {
        Ok(v)
    } else {
        Err(KernelError::InvalidInput(format!("{what} must be > 0, got {v}")))
    }
}

pub fn check_finite(what: &str, v: DVec3) -> Result<DVec3> {
    if v.is_finite() { Ok(v) } else { Err(KernelError::InvalidInput(format!("{what} is not finite"))) }
}

/// A non-zero finite direction, normalised.
pub fn check_dir(what: &str, v: DVec3) -> Result<DVec3> {
    let n = check_finite(what, v)?.length();
    if n > 1e-12 { Ok(v / n) } else { Err(KernelError::InvalidInput(format!("{what} has zero length"))) }
}

#[cfg(test)]
mod tests;
