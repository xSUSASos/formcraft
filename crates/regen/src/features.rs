//! Regeneration of sketch-based and dress-up features.

use formcraft_doc::{
    BodyId, BodyOp, Expr, ExtrudeDirection, FaceSel, Feature, FeatureId, ProfileSel, RevolveAxis, SketchPlane, Vec3Expr,
};
use formcraft_kernel::{BoolOp, DVec2, DVec3, Plane, Shape};
use formcraft_sketch::{self as sk, Entity, Sketch};

use crate::select;
use crate::{Ctx, SketchResult};

fn plane_by_name(name: &str) -> Result<Plane, String> {
    match name.to_ascii_uppercase().as_str() {
        "XY" => Ok(Plane::XY),
        "XZ" => Ok(Plane::XZ),
        "YZ" => Ok(Plane::YZ),
        other => Err(format!("unknown origin plane `{other}` (XY, XZ, YZ)")),
    }
}

/// In-plane x direction for a face plane: world X projected, or world Y if X is normal to it.
fn face_x(normal: DVec3) -> DVec3 {
    let x = DVec3::X - normal * normal.x;
    if x.length() > 1e-6 { x.normalize() } else { (DVec3::Y - normal * normal.y).normalize() }
}

pub(crate) fn sketch_plane(ctx: &Ctx<'_>, p: &SketchPlane) -> Result<Plane, String> {
    match p {
        SketchPlane::Origin { name, offset } => Ok(plane_by_name(name)?.offset(ctx.num(offset, "offset")?)),
        SketchPlane::Face { body, face, offset } => {
            let shape = ctx.get(*body)?;
            let f = select::resolve_planar_face(ctx.kernel, shape, face)?;
            let n = f.normal.normalize();
            let plane = Plane::new(f.center, face_x(n), n).map_err(|e| e.to_string())?;
            Ok(plane.offset(ctx.num(offset, "offset")?))
        }
    }
}

pub(crate) fn run_sketch(ctx: &mut Ctx<'_>, f: &Feature, plane: &SketchPlane, sketch: &Sketch) -> Result<(), String> {
    let plane = sketch_plane(ctx, plane)?;
    let mut s = sketch.clone();
    let mut values = Vec::with_capacity(s.dimensions.len());
    for d in &s.dimensions {
        values.push(ctx.num(&Expr(d.value.clone()), "dimension")?);
    }
    let report = sk::solve(&mut s, &values);
    if !report.converged {
        return Err(format!(
            "sketch cannot be solved: conflicting constraints or dimensions (residual {:.3})",
            report.residual
        ));
    }
    let regions = sk::regions(&s);
    ctx.sketches.push(SketchResult { id: f.id, plane, sketch: s, regions, dof: report.dof });
    Ok(())
}

fn sketch_of<'a>(ctx: &'a Ctx<'_>, id: FeatureId) -> Result<&'a SketchResult, String> {
    ctx.sketches.iter().find(|s| s.id == id).ok_or_else(|| format!("sketch #{} is missing or failed", id.0))
}

fn pick_regions<'a>(s: &'a SketchResult, sel: &ProfileSel) -> Result<Vec<&'a sk::Region>, String> {
    let idx: Vec<usize> = match sel {
        ProfileSel::All => sk::default_regions(&s.regions),
        ProfileSel::Indices { indices } => indices.clone(),
        ProfileSel::Points { points } => points
            .iter()
            .map(|p| {
                sk::region_at(&s.regions, DVec2::new(p[0], p[1]))
                    .ok_or(format!("no closed profile at ({}, {})", p[0], p[1]))
            })
            .collect::<Result<_, _>>()?,
    };
    let out: Vec<&sk::Region> = idx.iter().filter_map(|i| s.regions.get(*i)).collect();
    if out.is_empty() || out.len() != idx.len() {
        return Err(if s.regions.is_empty() {
            "the sketch has no closed profile".into()
        } else {
            "selected profile does not exist".into()
        });
    }
    Ok(out)
}

/// Size of the model so far (for "through all").
fn model_extent(ctx: &Ctx<'_>) -> f64 {
    let mut bb = formcraft_kernel::Aabb::empty();
    for (_, s) in &ctx.bodies {
        if let Ok(m) = ctx.kernel.mass_props(s) {
            bb = bb.union(&m.bbox);
        }
    }
    if bb.is_empty() { 1000.0 } else { (bb.size().length() + bb.min.abs().max(bb.max.abs()).length()) * 2.0 + 10.0 }
}

pub(crate) fn run_extrude(
    ctx: &mut Ctx<'_>,
    f: &Feature,
    sketch: FeatureId,
    profiles: &ProfileSel,
    distance: &Expr,
    direction: ExtrudeDirection,
    through_all: bool,
    op: BodyOp,
) -> Result<(), String> {
    let s = sketch_of(ctx, sketch)?;
    let profile = sk::to_profile(s.plane, &pick_regions(s, profiles)?);
    let n = s.plane.normal;
    let (profile, vector) = if through_all {
        if op == BodyOp::NewBody {
            return Err("through all needs a target body (cut, join or intersect)".into());
        }
        let d = model_extent(ctx);
        (formcraft_kernel::Profile { plane: profile.plane.offset(-d), ..profile }, n * 2.0 * d)
    } else {
        let d = ctx.num(distance, "distance")?;
        if d.abs() < 1e-9 {
            return Err("extrude distance must not be zero".into());
        }
        match direction {
            ExtrudeDirection::OneSide => (profile, n * d),
            ExtrudeDirection::Symmetric => {
                (formcraft_kernel::Profile { plane: profile.plane.offset(-d.abs() / 2.0), ..profile }, n * d.abs())
            }
        }
    };
    let solid = ctx.kernel.extrude(&profile, vector).map_err(|e| e.to_string())?;
    ctx.apply(f, op, solid)
}

pub(crate) fn run_revolve(
    ctx: &mut Ctx<'_>,
    f: &Feature,
    sketch: FeatureId,
    profiles: &ProfileSel,
    axis: &RevolveAxis,
    angle: &Expr,
    op: BodyOp,
) -> Result<(), String> {
    let deg = ctx.num(angle, "angle")?;
    let s = sketch_of(ctx, sketch)?;
    let profile = sk::to_profile(s.plane, &pick_regions(s, profiles)?);
    let (o, d) = match axis {
        RevolveAxis::Name(n) => match n.as_str() {
            "X" | "x" => (DVec3::ZERO, DVec3::X),
            "Y" | "y" => (DVec3::ZERO, DVec3::Y),
            "Z" | "z" => (DVec3::ZERO, DVec3::Z),
            "sketch_x" => (s.plane.origin, s.plane.x_dir),
            "sketch_y" => (s.plane.origin, s.plane.y_dir()),
            other => return Err(format!("unknown axis `{other}` (X, Y, Z, sketch_x, sketch_y or a sketch line)")),
        },
        RevolveAxis::Line { line } => match s.sketch.get(*line) {
            Some(Entity::Line { a, b, .. }) => {
                let (pa, pb) =
                    (s.sketch.point(*a).map_err(|e| e.to_string())?, s.sketch.point(*b).map_err(|e| e.to_string())?);
                let wa = s.plane.to_world(DVec2::new(pa.0, pa.1));
                let wb = s.plane.to_world(DVec2::new(pb.0, pb.1));
                (wa, wb - wa)
            }
            _ => return Err("revolve axis must be a sketch line".into()),
        },
    };
    let solid = ctx.kernel.revolve(&profile, o, d, deg.to_radians()).map_err(|e| e.to_string())?;
    ctx.apply(f, op, solid)
}

pub(crate) fn run_fillet(
    ctx: &mut Ctx<'_>,
    body: BodyId,
    edges: &formcraft_doc::EdgeSel,
    radius: &Expr,
    chamfer: bool,
) -> Result<(), String> {
    let r = ctx.num(radius, if chamfer { "distance" } else { "radius" })?;
    let shape = ctx.get(body)?.clone();
    let idx = select::resolve_edges(ctx.kernel, &shape, edges)?;
    let out = if chamfer { ctx.kernel.chamfer(&shape, &idx, r) } else { ctx.kernel.fillet(&shape, &idx, r) };
    ctx.set(body, out.map_err(|e| e.to_string())?);
    Ok(())
}

pub(crate) fn run_shell(ctx: &mut Ctx<'_>, body: BodyId, faces: &FaceSel, thickness: &Expr) -> Result<(), String> {
    let t = ctx.num(thickness, "thickness")?;
    let shape = ctx.get(body)?.clone();
    let (idx, _) = select::resolve_faces(ctx.kernel, &shape, faces)?;
    let out = ctx.kernel.shell(&shape, &idx, t).map_err(|e| e.to_string())?;
    ctx.set(body, out);
    Ok(())
}

/// Union of tool solids (holes) or the single one.
fn union_all(ctx: &Ctx<'_>, tools: Vec<Shape>) -> Result<Shape, String> {
    let mut it = tools.into_iter();
    let mut acc = it.next().ok_or("no tools")?;
    for t in it {
        acc = ctx.kernel.boolean(BoolOp::Union, &acc, &t).map_err(|e| e.to_string())?;
    }
    Ok(acc)
}

pub(crate) struct HoleSpec<'a> {
    pub body: BodyId,
    pub face: &'a FaceSel,
    pub points: &'a [Vec3Expr],
    pub diameter: &'a Expr,
    pub depth: Option<&'a Expr>,
    pub counterbore: Option<&'a (Expr, Expr)>,
    pub countersink: Option<&'a (Expr, Expr)>,
}

pub(crate) fn run_hole(ctx: &mut Ctx<'_>, h: HoleSpec<'_>) -> Result<(), String> {
    if h.points.is_empty() || h.points.len() > 1000 {
        return Err("give 1 to 1000 hole positions".into());
    }
    let shape = ctx.get(h.body)?.clone();
    let face = select::resolve_planar_face(ctx.kernel, &shape, h.face)?;
    let n = face.normal.normalize();
    let d = ctx.num(h.diameter, "diameter")?;
    let depth = match h.depth {
        Some(e) => ctx.num(e, "depth")?,
        None => model_extent(ctx),
    };
    let eps = 0.01 * d.max(1.0);
    let mut tools = Vec::new();
    for p in h.points {
        let p = ctx.vec(p, "position")?;
        // Project onto the face plane.
        let on = p - n * (p - face.center).dot(n);
        let k = ctx.kernel;
        tools.push(k.make_cylinder(on + n * eps, -n, d / 2.0, depth + eps).map_err(|e| e.to_string())?);
        if let Some((cd, cdepth)) = h.counterbore {
            let (cd, cdepth) = (ctx.num(cd, "counterbore diameter")?, ctx.num(cdepth, "counterbore depth")?);
            if cd <= d {
                return Err("counterbore diameter must be larger than the hole".into());
            }
            tools.push(k.make_cylinder(on + n * eps, -n, cd / 2.0, cdepth + eps).map_err(|e| e.to_string())?);
        }
        if let Some((sd, angle)) = h.countersink {
            let (sd, angle) = (ctx.num(sd, "countersink diameter")?, ctx.num(angle, "countersink angle")?);
            if sd <= d || !(1.0..179.0).contains(&angle) {
                return Err("countersink diameter must exceed the hole and the angle be 1..179°".into());
            }
            let height = (sd - d) / 2.0 / (angle.to_radians() / 2.0).tan();
            // Slightly taller cone starting above the surface keeps the rim clean.
            let r_top = sd / 2.0 + eps * (angle.to_radians() / 2.0).tan();
            tools.push(k.make_cone(on + n * eps, -n, r_top, d / 2.0, height + eps).map_err(|e| e.to_string())?);
        }
    }
    let tool = union_all(ctx, tools)?;
    let out = ctx.kernel.boolean(BoolOp::Cut, &shape, &tool).map_err(|e| e.to_string())?;
    ctx.set(h.body, out);
    Ok(())
}

pub(crate) fn run_press_pull(ctx: &mut Ctx<'_>, body: BodyId, face: &FaceSel, distance: &Expr) -> Result<(), String> {
    let dist = ctx.num(distance, "distance")?;
    if dist.abs() < 1e-9 {
        return Err("press pull distance must not be zero".into());
    }
    let shape = ctx.get(body)?.clone();
    let f = select::resolve_planar_face(ctx.kernel, &shape, face)?;
    let prism = ctx.kernel.extrude_face(&shape, f.index, f.normal.normalize() * dist).map_err(|e| e.to_string())?;
    let op = if dist > 0.0 { BoolOp::Union } else { BoolOp::Cut };
    let out = ctx.kernel.boolean(op, &shape, &prism).map_err(|e| e.to_string())?;
    ctx.set(body, out);
    Ok(())
}
