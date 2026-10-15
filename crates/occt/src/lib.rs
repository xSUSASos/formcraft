//! OpenCascade implementation of the FormCraft [`Kernel`] trait.
//!
//! The C++ facade (`cpp/bridge.cxx`) catches every OCCT exception, so a failing operation is an
//! `Err`, never an abort (docs/adr/0001-geometry-kernel.md). All OCCT calls are serialised by a
//! process-wide lock: OCCT is not safe for concurrent use of shared topology.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod ffi;

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use cxx::UniquePtr;
use ffi::bridge as sys;
use formcraft_kernel::{
    Aabb, BoolOp, CurveKind, DMat4, DVec3, EdgeInfo, EdgePolyline, FaceInfo, Kernel, KernelError, MassProps, Mesh,
    Profile, Result, Segment2, Shape, SurfaceKind, check_dir, check_finite, check_positive,
};

pub const NAME: &str = "occt";

static KERNEL_LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    KERNEL_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The OCCT shape behind a [`Shape`].
struct OcctShape(UniquePtr<sys::Shape>);

fn raw(s: &Shape) -> Result<&sys::Shape> {
    s.get::<OcctShape>()?.0.as_ref().ok_or_else(|| KernelError::failed("shape", "null shape"))
}

fn wrap(op: &'static str, r: std::result::Result<UniquePtr<sys::Shape>, cxx::Exception>) -> Result<Shape> {
    let p = r.map_err(|e| KernelError::failed(op, e.what()))?;
    if p.is_null() {
        return Err(KernelError::failed(op, "empty result"));
    }
    Ok(Shape::new(NAME, OcctShape(p)))
}

fn cx<T>(op: &'static str, r: std::result::Result<T, cxx::Exception>) -> Result<T> {
    r.map_err(|e| KernelError::failed(op, e.what()))
}

fn v3(v: DVec3) -> [f64; 3] {
    v.to_array()
}

fn at(v: &[f64], i: usize) -> f64 {
    v.get(i).copied().unwrap_or(0.0)
}

fn p3(v: &[f64], i: usize) -> DVec3 {
    DVec3::new(at(v, i), at(v, i + 1), at(v, i + 2))
}

fn bbox6(v: &[f64], i: usize) -> Aabb {
    Aabb { min: p3(v, i), max: p3(v, i + 3) }
}

/// Encode a profile for `make_faces`: stride-10 segments in world coordinates.
fn encode_profile(p: &Profile) -> Result<(Vec<f64>, Vec<u32>, Vec<u32>)> {
    p.validate()?;
    let pl = &p.plane;
    let (mut segs, mut loops, mut regions) = (Vec::new(), Vec::new(), Vec::new());
    for r in &p.regions {
        regions.push(u32::try_from(1 + r.holes.len()).map_err(|_| KernelError::InvalidInput("too many holes".into()))?);
        for l in std::iter::once(&r.outer).chain(&r.holes) {
            loops.push(u32::try_from(l.segments.len()).map_err(|_| KernelError::InvalidInput("loop too long".into()))?);
            for s in &l.segments {
                let mut rec = [0.0f64; 10];
                match *s {
                    Segment2::Line { a, b } => {
                        rec[1..4].copy_from_slice(&v3(pl.to_world(a)));
                        rec[4..7].copy_from_slice(&v3(pl.to_world(b)));
                    }
                    Segment2::Arc { start, mid, end } => {
                        rec[0] = 1.0;
                        rec[1..4].copy_from_slice(&v3(pl.to_world(start)));
                        rec[4..7].copy_from_slice(&v3(pl.to_world(mid)));
                        rec[7..10].copy_from_slice(&v3(pl.to_world(end)));
                    }
                    Segment2::Circle { center, radius } => {
                        rec[0] = 2.0;
                        rec[1..4].copy_from_slice(&v3(pl.to_world(center)));
                        rec[4..7].copy_from_slice(&v3(pl.normal));
                        rec[7] = radius;
                    }
                }
                segs.extend_from_slice(&rec);
            }
        }
    }
    Ok((segs, loops, regions))
}

/// Reject results OCCT reports as done but that are broken (invalid topology, no volume).
/// Called with the lock held.
fn checked(op: &'static str, shape: Shape, max_volume: Option<f64>) -> Result<Shape> {
    let r = raw(&shape)?;
    if !sys::is_valid(r) {
        return Err(KernelError::failed(op, "result is not a valid solid (try smaller values)"));
    }
    let v = cx(op, sys::mass_props(r))?;
    let vol = at(&v, 0);
    if !(vol.is_finite() && vol > 1e-9) {
        return Err(KernelError::failed(op, "result has no volume"));
    }
    if let Some(max) = max_volume
        && vol > max * (1.0 + 1e-6)
    {
        return Err(KernelError::failed(op, "result is larger than the input (self-intersecting)"));
    }
    Ok(shape)
}

/// OpenCascade kernel. Stateless; all instances share the process-wide OCCT lock.
#[derive(Debug, Default, Clone, Copy)]
pub struct Occt;

impl Occt {
    fn faces_of(&self, profile: &Profile) -> Result<UniquePtr<sys::Shape>> {
        let (segs, loops, regions) = encode_profile(profile)?;
        cx("profile", sys::make_faces(&segs, &loops, &regions, &v3(profile.plane.normal)))
    }
}

impl Kernel for Occt {
    fn name(&self) -> &'static str {
        NAME
    }

    fn make_box(&self, min: DVec3, size: DVec3) -> Result<Shape> {
        let min = check_finite("box corner", min)?;
        for (n, v) in [("box width", size.x), ("box depth", size.y), ("box height", size.z)] {
            check_positive(n, v)?;
        }
        let _g = lock();
        wrap("box", sys::make_box(min.x, min.y, min.z, size.x, size.y, size.z))
    }

    fn make_cylinder(&self, base: DVec3, axis: DVec3, radius: f64, height: f64) -> Result<Shape> {
        let (b, a) = (check_finite("cylinder base", base)?, check_dir("cylinder axis", axis)?);
        let (r, h) = (check_positive("cylinder radius", radius)?, check_positive("cylinder height", height)?);
        let _g = lock();
        wrap("cylinder", sys::make_cylinder(&v3(b), &v3(a), r, h))
    }

    fn make_cone(&self, base: DVec3, axis: DVec3, r1: f64, r2: f64, height: f64) -> Result<Shape> {
        let (b, a) = (check_finite("cone base", base)?, check_dir("cone axis", axis)?);
        if !(r1.is_finite() && r2.is_finite() && r1 >= 0.0 && r2 >= 0.0 && (r1 - r2).abs() > 1e-9) {
            return Err(KernelError::InvalidInput("cone radii must be >= 0 and differ".into()));
        }
        let h = check_positive("cone height", height)?;
        let _g = lock();
        wrap("cone", sys::make_cone(&v3(b), &v3(a), r1, r2, h))
    }

    fn make_sphere(&self, center: DVec3, radius: f64) -> Result<Shape> {
        let (c, r) = (check_finite("sphere center", center)?, check_positive("sphere radius", radius)?);
        let _g = lock();
        wrap("sphere", sys::make_sphere(&v3(c), r))
    }

    fn make_torus(&self, center: DVec3, axis: DVec3, major: f64, minor: f64) -> Result<Shape> {
        let (c, a) = (check_finite("torus center", center)?, check_dir("torus axis", axis)?);
        let (r1, r2) = (check_positive("torus major radius", major)?, check_positive("torus minor radius", minor)?);
        let _g = lock();
        wrap("torus", sys::make_torus(&v3(c), &v3(a), r1, r2))
    }

    fn make_face(&self, profile: &Profile) -> Result<Shape> {
        let _g = lock();
        wrap("profile", Ok(self.faces_of(profile)?))
    }

    fn extrude(&self, profile: &Profile, vector: DVec3) -> Result<Shape> {
        let v = check_finite("extrude direction", vector)?;
        if v.length() < 1e-9 {
            return Err(KernelError::InvalidInput("extrude distance must not be zero".into()));
        }
        let _g = lock();
        let f = self.faces_of(profile)?;
        let f = f.as_ref().ok_or_else(|| KernelError::failed("extrude", "no face"))?;
        wrap("extrude", sys::prism(f, v.x, v.y, v.z))
    }

    fn revolve(&self, profile: &Profile, axis_origin: DVec3, axis_dir: DVec3, angle: f64) -> Result<Shape> {
        let (o, d) = (check_finite("revolve axis origin", axis_origin)?, check_dir("revolve axis", axis_dir)?);
        if !angle.is_finite() || angle.abs() < 1e-9 {
            return Err(KernelError::InvalidInput("revolve angle must not be zero".into()));
        }
        let angle = angle.clamp(-std::f64::consts::TAU, std::f64::consts::TAU);
        let _g = lock();
        let f = self.faces_of(profile)?;
        let f = f.as_ref().ok_or_else(|| KernelError::failed("revolve", "no face"))?;
        wrap("revolve", sys::revolve(f, &v3(o), &v3(d), angle))
    }

    fn extrude_face(&self, shape: &Shape, face: u32, vector: DVec3) -> Result<Shape> {
        let v = check_finite("press pull direction", vector)?;
        if v.length() < 1e-9 {
            return Err(KernelError::InvalidInput("press pull distance must not be zero".into()));
        }
        let _g = lock();
        wrap("press pull", sys::face_prism(raw(shape)?, face, v.x, v.y, v.z))
    }

    fn boolean(&self, op: BoolOp, a: &Shape, b: &Shape) -> Result<Shape> {
        let code = match op {
            BoolOp::Union => 0,
            BoolOp::Cut => 1,
            BoolOp::Intersect => 2,
        };
        let _g = lock();
        wrap("boolean", sys::boolean_op(raw(a)?, raw(b)?, code))
    }

    fn fillet(&self, shape: &Shape, edges: &[u32], radius: f64) -> Result<Shape> {
        let r = check_positive("fillet radius", radius)?;
        let _g = lock();
        let s = wrap("fillet", sys::fillet(raw(shape)?, edges, r))?;
        checked("fillet", s, None)
    }

    fn chamfer(&self, shape: &Shape, edges: &[u32], distance: f64) -> Result<Shape> {
        let d = check_positive("chamfer distance", distance)?;
        let _g = lock();
        let s = wrap("chamfer", sys::chamfer(raw(shape)?, edges, d))?;
        checked("chamfer", s, None)
    }

    fn shell(&self, shape: &Shape, open_faces: &[u32], thickness: f64) -> Result<Shape> {
        let t = check_positive("shell thickness", thickness)?;
        let _g = lock();
        let before = cx("shell", sys::mass_props(raw(shape)?))?;
        let s = wrap("shell", sys::thick_solid(raw(shape)?, open_faces, -t))?;
        let s = checked("shell", s, Some(at(&before, 0)))?;
        // An inward shell keeps the outer boundary: same bounding box, less volume.
        let after = cx("shell", sys::mass_props(raw(&s)?))?;
        let (b0, b1) = (bbox6(&before, 5), bbox6(&after, 5));
        let tol = 1e-3 * b0.size().length().max(1.0);
        if b0.min.distance(b1.min) > tol || b0.max.distance(b1.max) > tol || at(&after, 0) >= at(&before, 0) {
            log::debug!("shell rejected: before {before:?} after {after:?}");
            return Err(KernelError::failed("shell", "wall thickness too large for this body"));
        }
        Ok(s)
    }

    fn transform(&self, shape: &Shape, m: &DMat4) -> Result<Shape> {
        let c = m.to_cols_array_2d();
        if !c.iter().flatten().all(|v| v.is_finite()) {
            return Err(KernelError::InvalidInput("transform matrix is not finite".into()));
        }
        // gp_Trsf takes the 3x4 affine part row by row.
        let rows: Vec<f64> = (0..3).flat_map(|r| (0..4).map(move |col| c[col][r])).collect();
        let _g = lock();
        wrap("transform", sys::transform(raw(shape)?, &rows))
    }

    fn compound(&self, shapes: &[Shape]) -> Result<Shape> {
        let _g = lock();
        let mut c = cx("compound", sys::compound_new())?;
        for s in shapes {
            let pin = c.as_mut().ok_or_else(|| KernelError::failed("compound", "null compound"))?;
            cx("compound", sys::compound_add(pin, raw(s)?))?;
        }
        wrap("compound", Ok(c))
    }

    fn solids(&self, shape: &Shape) -> Result<Vec<Shape>> {
        let _g = lock();
        let s = raw(shape)?;
        let n = cx("solids", sys::solid_count(s))?;
        (0..n).map(|i| wrap("solids", sys::solid_at(s, i))).collect()
    }

    fn faces(&self, shape: &Shape) -> Result<Vec<FaceInfo>> {
        let v = {
            let _g = lock();
            cx("faces", sys::face_info(raw(shape)?))?
        };
        const STRIDE: usize = 21;
        Ok(v.as_chunks::<STRIDE>()
            .0
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let (o, d, r) = (p3(f, 1), p3(f, 4), at(f, 7));
                let kind = match at(f, 0) as u32 {
                    0 => SurfaceKind::Plane { normal: d },
                    1 => SurfaceKind::Cylinder { origin: o, axis: d, radius: r },
                    2 => SurfaceKind::Cone { origin: o, axis: d },
                    3 => SurfaceKind::Sphere { center: o, radius: r },
                    4 => SurfaceKind::Torus { center: o, axis: d },
                    _ => SurfaceKind::Other,
                };
                FaceInfo {
                    index: i as u32,
                    kind,
                    center: p3(f, 8),
                    normal: p3(f, 11),
                    area: at(f, 14),
                    bbox: bbox6(f, 15),
                }
            })
            .collect())
    }

    fn edges(&self, shape: &Shape) -> Result<Vec<EdgeInfo>> {
        let v = {
            let _g = lock();
            cx("edges", sys::edge_info(raw(shape)?))?
        };
        const FIXED: usize = 18;
        let mut out = Vec::new();
        let mut i = 0usize;
        let mut index = 0u32;
        while let Some(e) = v.get(i..).filter(|e| e.len() > FIXED) {
            let n = at(e, FIXED) as usize;
            let faces: Vec<u32> = (0..n).map(|k| at(e, FIXED + 1 + k) as u32).collect();
            let kind = match at(e, 0) as u32 {
                0 => Some(CurveKind::Line { dir: p3(e, 14) }),
                1 => Some(CurveKind::Circle { center: p3(e, 11), axis: p3(e, 14), radius: at(e, 17) }),
                2 => Some(CurveKind::Other),
                _ => None, // degenerated edge
            };
            if let Some(kind) = kind {
                out.push(EdgeInfo {
                    index,
                    kind,
                    start: p3(e, 1),
                    end: p3(e, 4),
                    mid: p3(e, 7),
                    length: at(e, 10),
                    faces,
                });
            }
            i += FIXED + 1 + n;
            index += 1;
        }
        Ok(out)
    }

    fn mass_props(&self, shape: &Shape) -> Result<MassProps> {
        let v = {
            let _g = lock();
            cx("mass properties", sys::mass_props(raw(shape)?))?
        };
        Ok(MassProps { volume: at(&v, 0), area: at(&v, 1), centroid: p3(&v, 2), bbox: bbox6(&v, 5) })
    }

    fn is_valid(&self, shape: &Shape) -> bool {
        let _g = lock();
        raw(shape).map(sys::is_valid).unwrap_or(false)
    }

    fn tessellate(&self, shape: &Shape, deflection: f64) -> Result<Mesh> {
        let d = check_positive("tessellation deflection", deflection)?;
        let mut m = sys::MeshData::default();
        {
            let _g = lock();
            cx("tessellate", sys::tessellate(raw(shape)?, d, &mut m))?;
        }
        let tri3 = |v: &[f32]| v.as_chunks::<3>().0.to_vec();
        let points = tri3(&m.edge_points);
        let mut edges = Vec::with_capacity(m.edge_ids.len());
        let mut start = 0usize;
        for (id, end) in m.edge_ids.iter().zip(&m.edge_offsets) {
            let end = *end as usize;
            edges.push(EdgePolyline { edge: *id, points: points.get(start..end).unwrap_or_default().to_vec() });
            start = end;
        }
        Ok(Mesh {
            positions: tri3(&m.positions),
            normals: tri3(&m.normals),
            indices: m.indices,
            triangle_face: m.triangle_face,
            edges,
        })
    }

    fn write_step(&self, shape: &Shape, path: &Path) -> Result<()> {
        let p = path.to_str().ok_or_else(|| KernelError::Io("path is not valid UTF-8".into()))?;
        let _g = lock();
        cx("STEP export", sys::write_step(raw(shape)?, p)).map_err(|e| KernelError::Io(e.to_string()))
    }

    fn read_step(&self, path: &Path) -> Result<Shape> {
        let p = path.to_str().ok_or_else(|| KernelError::Io("path is not valid UTF-8".into()))?;
        if !path.is_file() {
            return Err(KernelError::Io(format!("{} does not exist", path.display())));
        }
        let _g = lock();
        wrap("STEP import", sys::read_step(p)).map_err(|e| KernelError::Io(e.to_string()))
    }

    fn serialize(&self, shape: &Shape) -> Result<Vec<u8>> {
        let _g = lock();
        cx("serialize", sys::to_brep(raw(shape)?))
    }

    fn deserialize(&self, bytes: &[u8]) -> Result<Shape> {
        let _g = lock();
        wrap("deserialize", sys::from_brep(bytes))
    }
}

#[cfg(test)]
mod tests;
