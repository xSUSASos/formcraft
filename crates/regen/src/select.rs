//! Resolving persistent face/edge selectors to the indices of the current body.

use formcraft_doc::select::{EdgeSignature, FaceSignature};
use formcraft_doc::{EdgeSel, FaceSel};
use formcraft_kernel::{Aabb, CurveKind, DVec3, EdgeInfo, FaceInfo, Kernel, Shape, SurfaceKind};

const AXIS: f64 = 0.999;

fn kind_name(k: &SurfaceKind) -> &'static str {
    match k {
        SurfaceKind::Plane { .. } => "plane",
        SurfaceKind::Cylinder { .. } => "cylinder",
        SurfaceKind::Cone { .. } => "cone",
        SurfaceKind::Sphere { .. } => "sphere",
        SurfaceKind::Torus { .. } => "torus",
        SurfaceKind::Other => "other",
    }
}

fn curve_name(k: &CurveKind) -> &'static str {
    match k {
        CurveKind::Line { .. } => "line",
        CurveKind::Circle { .. } => "circle",
        CurveKind::Other => "other",
    }
}

fn planar(f: &FaceInfo) -> bool {
    matches!(f.kind, SurfaceKind::Plane { .. })
}

fn v3(a: [f64; 3]) -> DVec3 {
    DVec3::from_array(a)
}

/// Signature of a face (recorded when the user picks it).
pub fn face_signature(f: &FaceInfo) -> FaceSignature {
    FaceSignature {
        kind: kind_name(&f.kind).into(),
        center: f.center.to_array(),
        normal: f.normal.to_array(),
        area: f.area,
    }
}

pub fn edge_signature(e: &EdgeInfo) -> EdgeSignature {
    EdgeSignature { kind: curve_name(&e.kind).into(), mid: e.mid.to_array(), length: e.length }
}

/// Distance from a point to a box (0 inside).
fn bbox_dist(p: DVec3, b: &Aabb) -> f64 {
    (b.min - p).max(p - b.max).max(DVec3::ZERO).length()
}

/// Extreme planar face facing `dir` (largest extent along it; ties → bigger area).
fn extreme(faces: &[FaceInfo], dir: DVec3) -> Option<u32> {
    faces
        .iter()
        .filter(|f| planar(f) && f.normal.dot(dir) > AXIS)
        .max_by(|a, b| a.center.dot(dir).total_cmp(&b.center.dot(dir)).then(a.area.total_cmp(&b.area)))
        .map(|f| f.index)
}

fn face_names(faces: &[FaceInfo], name: &str) -> Result<Vec<u32>, String> {
    let one = |o: Option<u32>| o.map(|i| vec![i]).ok_or_else(|| format!("no face matches `{name}`"));
    match name {
        "top" | "+Z" => one(extreme(faces, DVec3::Z)),
        "bottom" | "-Z" => one(extreme(faces, DVec3::NEG_Z)),
        "+X" => one(extreme(faces, DVec3::X)),
        "-X" => one(extreme(faces, DVec3::NEG_X)),
        "+Y" => one(extreme(faces, DVec3::Y)),
        "-Y" => one(extreme(faces, DVec3::NEG_Y)),
        "largest_planar" => {
            one(faces.iter().filter(|f| planar(f)).max_by(|a, b| a.area.total_cmp(&b.area)).map(|f| f.index))
        }
        "smallest_planar" => {
            one(faces.iter().filter(|f| planar(f)).min_by(|a, b| a.area.total_cmp(&b.area)).map(|f| f.index))
        }
        "cylindrical" => {
            Ok(faces.iter().filter(|f| matches!(f.kind, SurfaceKind::Cylinder { .. })).map(|f| f.index).collect())
        }
        "planar" => Ok(faces.iter().filter(|f| planar(f)).map(|f| f.index).collect()),
        "all" => Ok(faces.iter().map(|f| f.index).collect()),
        other => Err(format!(
            "unknown face selector `{other}` (use one of {})",
            formcraft_doc::select::FACE_NAMES.join(", ")
        )),
    }
}

/// How far `p` is from face `f` (approximate, good enough to tell faces apart).
fn face_distance(p: DVec3, f: &FaceInfo) -> f64 {
    let off = bbox_dist(p, &f.bbox);
    match f.kind {
        SurfaceKind::Plane { normal } => (p - f.center).dot(normal).abs() + off,
        SurfaceKind::Cylinder { origin, axis, radius } => {
            let d = p - origin;
            let radial = (d - axis * d.dot(axis)).length();
            (radial - radius).abs() + off
        }
        SurfaceKind::Sphere { center, radius } => ((p - center).length() - radius).abs() + off,
        _ => off + (p - f.center).length() * 0.1,
    }
}

fn signature_score(s: &FaceSignature, f: &FaceInfo) -> f64 {
    let kind = if kind_name(&f.kind) == s.kind { 0.0 } else { 1e6 };
    let angle = 1.0 - f.normal.dot(v3(s.normal)).clamp(-1.0, 1.0);
    let scale = s.area.max(1e-6).sqrt();
    kind + (f.center - v3(s.center)).length() / scale + angle * 10.0 + (f.area - s.area).abs() / s.area.max(1e-6)
}

fn faces_of(faces: &[FaceInfo], sel: &FaceSel, depth: usize) -> Result<Vec<u32>, String> {
    if depth > 16 {
        return Err("face selector nested too deeply".into());
    }
    match sel {
        FaceSel::Name(n) => face_names(faces, n),
        FaceSel::Near { near } => {
            let p = v3(*near);
            if !p.is_finite() {
                return Err("face selector point is not finite".into());
            }
            faces
                .iter()
                .min_by(|a, b| face_distance(p, a).total_cmp(&face_distance(p, b)))
                .map(|f| vec![f.index])
                .ok_or_else(|| "body has no faces".into())
        }
        FaceSel::Signature { face } => faces
            .iter()
            .min_by(|a, b| signature_score(face, a).total_cmp(&signature_score(face, b)))
            .filter(|f| signature_score(face, f) < 1e5)
            .map(|f| vec![f.index])
            .ok_or_else(|| format!("the selected {} face no longer exists", face.kind)),
        FaceSel::Many(list) => {
            let mut out = Vec::new();
            for s in list {
                for i in faces_of(faces, s, depth + 1)? {
                    if !out.contains(&i) {
                        out.push(i);
                    }
                }
            }
            Ok(out)
        }
    }
}

pub fn resolve_faces(k: &dyn Kernel, shape: &Shape, sel: &FaceSel) -> Result<(Vec<u32>, Vec<FaceInfo>), String> {
    let faces = k.faces(shape).map_err(|e| e.to_string())?;
    let idx = faces_of(&faces, sel, 0)?;
    Ok((idx, faces))
}

/// Exactly one planar face.
pub fn resolve_planar_face(k: &dyn Kernel, shape: &Shape, sel: &FaceSel) -> Result<FaceInfo, String> {
    let (idx, faces) = resolve_faces(k, shape, sel)?;
    let [i] = idx.as_slice() else { return Err(format!("expected one face, the selector matched {}", idx.len())) };
    let f = faces.iter().find(|f| f.index == *i).ok_or("face vanished")?;
    if !planar(f) {
        return Err("the face is not planar".into());
    }
    Ok(f.clone())
}

fn edge_point_distance(p: DVec3, e: &EdgeInfo) -> f64 {
    match e.kind {
        CurveKind::Line { .. } => {
            let ab = e.end - e.start;
            let t = if ab.length_squared() > 0.0 {
                ((p - e.start).dot(ab) / ab.length_squared()).clamp(0.0, 1.0)
            } else {
                0.0
            };
            (e.start + ab * t - p).length()
        }
        CurveKind::Circle { center, axis, radius } => {
            let d = p - center;
            let planar = d - axis * d.dot(axis);
            let radial = planar.length() - radius;
            (radial * radial + d.dot(axis).powi(2)).sqrt()
        }
        CurveKind::Other => [e.start, e.mid, e.end].iter().map(|q| (*q - p).length()).fold(f64::INFINITY, f64::min),
    }
}

fn edges_on_faces(edges: &[EdgeInfo], faces: &[u32]) -> Vec<u32> {
    edges.iter().filter(|e| e.faces.iter().any(|f| faces.contains(f))).map(|e| e.index).collect()
}

fn edge_names(edges: &[EdgeInfo], faces: &[FaceInfo], name: &str) -> Result<Vec<u32>, String> {
    let line_dir = |e: &EdgeInfo| match e.kind {
        CurveKind::Line { dir } => Some(dir),
        _ => None,
    };
    let out: Vec<u32> = match name {
        "all" => edges.iter().map(|e| e.index).collect(),
        "vertical" => edges.iter().filter(|e| line_dir(e).is_some_and(|d| d.z.abs() > AXIS)).map(|e| e.index).collect(),
        "horizontal" => edges
            .iter()
            .filter(|e| match e.kind {
                CurveKind::Line { dir } => dir.z.abs() < 1e-3,
                CurveKind::Circle { axis, .. } => axis.z.abs() > AXIS,
                CurveKind::Other => false,
            })
            .map(|e| e.index)
            .collect(),
        "circular" => edges.iter().filter(|e| matches!(e.kind, CurveKind::Circle { .. })).map(|e| e.index).collect(),
        "linear" => edges.iter().filter(|e| matches!(e.kind, CurveKind::Line { .. })).map(|e| e.index).collect(),
        "top" | "bottom" | "top_outer" | "bottom_outer" => {
            let dir = if name.starts_with("top") { DVec3::Z } else { DVec3::NEG_Z };
            let Some(fi) = extreme(faces, dir) else { return Err(format!("no face for `{name}` edges")) };
            let mut list = edges_on_faces(edges, &[fi]);
            if name.ends_with("outer")
                && let Some(f) = faces.iter().find(|f| f.index == fi)
            {
                // Outer boundary: edges touching the face's bounding rectangle.
                let tol = 1e-6 * f.bbox.size().length().max(1.0);
                list.retain(|i| {
                    edges.iter().find(|e| e.index == *i).is_some_and(|e| {
                        let m = e.mid;
                        (m.x - f.bbox.min.x).abs() < tol
                            || (m.x - f.bbox.max.x).abs() < tol
                            || (m.y - f.bbox.min.y).abs() < tol
                            || (m.y - f.bbox.max.y).abs() < tol
                    })
                });
            }
            list
        }
        other => {
            return Err(format!(
                "unknown edge selector `{other}` (use one of {})",
                formcraft_doc::select::EDGE_NAMES.join(", ")
            ));
        }
    };
    if out.is_empty() { Err(format!("no edge matches `{name}`")) } else { Ok(out) }
}

fn edges_of(edges: &[EdgeInfo], faces: &[FaceInfo], sel: &EdgeSel, depth: usize) -> Result<Vec<u32>, String> {
    if depth > 16 {
        return Err("edge selector nested too deeply".into());
    }
    match sel {
        EdgeSel::Name(n) => edge_names(edges, faces, n),
        EdgeSel::Of { edges_of: fsel } => {
            let fi = faces_of(faces, fsel, depth + 1)?;
            let out = edges_on_faces(edges, &fi);
            if out.is_empty() { Err("the selected faces have no edges".into()) } else { Ok(out) }
        }
        EdgeSel::Near { near } => {
            let p = v3(*near);
            if !p.is_finite() {
                return Err("edge selector point is not finite".into());
            }
            edges
                .iter()
                .min_by(|a, b| edge_point_distance(p, a).total_cmp(&edge_point_distance(p, b)))
                .map(|e| vec![e.index])
                .ok_or_else(|| "body has no edges".into())
        }
        EdgeSel::Signature { edge } => {
            let score = |e: &EdgeInfo| {
                (if curve_name(&e.kind) == edge.kind { 0.0 } else { 1e6 })
                    + (e.mid - v3(edge.mid)).length() / edge.length.max(1e-6)
                    + (e.length - edge.length).abs() / edge.length.max(1e-6)
            };
            edges
                .iter()
                .min_by(|a, b| score(a).total_cmp(&score(b)))
                .filter(|e| score(e) < 1e5)
                .map(|e| vec![e.index])
                .ok_or_else(|| "the selected edge no longer exists".into())
        }
        EdgeSel::Many(list) => {
            let mut out = Vec::new();
            for s in list {
                for i in edges_of(edges, faces, s, depth + 1)? {
                    if !out.contains(&i) {
                        out.push(i);
                    }
                }
            }
            Ok(out)
        }
    }
}

pub fn resolve_edges(k: &dyn Kernel, shape: &Shape, sel: &EdgeSel) -> Result<Vec<u32>, String> {
    let faces = k.faces(shape).map_err(|e| e.to_string())?;
    let edges = k.edges(shape).map_err(|e| e.to_string())?;
    let out = edges_of(&edges, &faces, sel, 0)?;
    if out.is_empty() { Err("no edges selected".into()) } else { Ok(out) }
}
