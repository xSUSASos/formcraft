//! Regeneration: evaluate the active timeline into B-rep bodies.
//!
//! Features run in order. A failing feature gets an error status and is skipped; the rest of
//! the timeline still regenerates (Fusion marks the feature red and keeps going). Suppressed
//! features and features after the marker are not computed.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod features;
pub mod select;

use std::collections::HashMap;

use formcraft_doc::{BodyId, BodyOp, CombineOp, Document, Expr, Feature, FeatureId, FeatureKind, Vec3Expr};
use formcraft_kernel::{BoolOp, DVec3, Kernel, Plane, Shape};
use serde::Serialize;

/// A body after regeneration.
#[derive(Debug, Clone)]
pub struct Body {
    pub id: BodyId,
    pub name: String,
    pub visible: bool,
    pub shape: Shape,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Ok,
    Suppressed,
    RolledBack,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FeatureStatus {
    pub id: FeatureId,
    pub name: String,
    pub status: Status,
}

/// A solved sketch (for drawing and for the features that use it).
#[derive(Debug, Clone)]
pub struct SketchResult {
    pub id: FeatureId,
    pub plane: Plane,
    /// Geometry after solving constraints and dimensions.
    pub sketch: formcraft_sketch::Sketch,
    pub regions: Vec<formcraft_sketch::Region>,
    /// Remaining degrees of freedom (0 = fully constrained).
    pub dof: usize,
}

/// Result of regenerating a document.
#[derive(Debug, Clone, Default)]
pub struct Model {
    /// Bodies in creation order.
    pub bodies: Vec<Body>,
    pub status: Vec<FeatureStatus>,
    pub sketches: Vec<SketchResult>,
}

impl Model {
    pub fn body(&self, id: BodyId) -> Option<&Body> {
        self.bodies.iter().find(|b| b.id == id)
    }
    pub fn errors(&self) -> impl Iterator<Item = &FeatureStatus> {
        self.status.iter().filter(|s| matches!(s.status, Status::Error(_)))
    }
    pub fn error_count(&self) -> usize {
        self.errors().count()
    }
    pub fn sketch(&self, id: FeatureId) -> Option<&SketchResult> {
        self.sketches.iter().find(|s| s.id == id)
    }
}

pub(crate) struct Ctx<'a> {
    pub(crate) kernel: &'a dyn Kernel,
    params: HashMap<String, f64>,
    pub(crate) bodies: Vec<(BodyId, Shape)>,
    pub(crate) sketches: Vec<SketchResult>,
}

impl Ctx<'_> {
    pub(crate) fn num(&self, e: &Expr, what: &str) -> Result<f64, String> {
        e.eval(&|n| self.params.get(n).copied()).map_err(|err| format!("{what}: {err}"))
    }
    pub(crate) fn vec(&self, v: &Vec3Expr, what: &str) -> Result<DVec3, String> {
        let [x, y, z] = &v.0;
        Ok(DVec3::new(self.num(x, what)?, self.num(y, what)?, self.num(z, what)?))
    }
    pub(crate) fn get(&self, b: BodyId) -> Result<&Shape, String> {
        self.bodies
            .iter()
            .find(|(id, _)| *id == b)
            .map(|(_, s)| s)
            .ok_or_else(|| format!("body #{} does not exist", b.0))
    }
    pub(crate) fn set(&mut self, b: BodyId, s: Shape) {
        match self.bodies.iter_mut().find(|(id, _)| *id == b) {
            Some(slot) => slot.1 = s,
            None => self.bodies.push((b, s)),
        }
    }
    /// Put a new solid into the model according to the feature's operation.
    pub(crate) fn apply(&mut self, f: &Feature, op: BodyOp, solid: Shape) -> Result<(), String> {
        let bool_op = match op {
            BodyOp::NewBody => {
                self.set(BodyId(f.id.0), solid);
                return Ok(());
            }
            BodyOp::Join(_) => BoolOp::Union,
            BodyOp::Cut(_) => BoolOp::Cut,
            BodyOp::Intersect(_) => BoolOp::Intersect,
        };
        let target = op.target().ok_or("missing target body")?;
        let base = self.get(target)?.clone();
        let r = self.kernel.boolean(bool_op, &base, &solid).map_err(|e| e.to_string())?;
        self.set(target, r);
        Ok(())
    }
}

fn run(ctx: &mut Ctx<'_>, f: &Feature) -> Result<(), String> {
    let k = ctx.kernel;
    match &f.kind {
        FeatureKind::Box { corner, size, op } => {
            let c = ctx.vec(corner, "corner")?;
            let s = ctx.vec(size, "size")?;
            // Negative sizes extend the other way (dragging a box from any corner).
            let min = c.min(c + s);
            let solid = k.make_box(min, s.abs()).map_err(|e| e.to_string())?;
            ctx.apply(f, *op, solid)
        }
        FeatureKind::Cylinder { base, axis, radius, height, op } => {
            let solid = k
                .make_cylinder(
                    ctx.vec(base, "base")?,
                    ctx.vec(axis, "axis")?,
                    ctx.num(radius, "radius")?,
                    ctx.num(height, "height")?,
                )
                .map_err(|e| e.to_string())?;
            ctx.apply(f, *op, solid)
        }
        FeatureKind::Sphere { center, radius, op } => {
            let solid =
                k.make_sphere(ctx.vec(center, "center")?, ctx.num(radius, "radius")?).map_err(|e| e.to_string())?;
            ctx.apply(f, *op, solid)
        }
        FeatureKind::Combine { target, tool, operation, keep_tool } => {
            if target == tool {
                return Err("target and tool must be different bodies".into());
            }
            let a = ctx.get(*target)?.clone();
            let b = ctx.get(*tool)?.clone();
            let op = match operation {
                CombineOp::Join => BoolOp::Union,
                CombineOp::Cut => BoolOp::Cut,
                CombineOp::Intersect => BoolOp::Intersect,
            };
            let r = k.boolean(op, &a, &b).map_err(|e| e.to_string())?;
            ctx.set(*target, r);
            if !keep_tool {
                ctx.bodies.retain(|(id, _)| id != tool);
            }
            Ok(())
        }
        FeatureKind::Sketch { plane, sketch } => features::run_sketch(ctx, f, plane, sketch),
        FeatureKind::Extrude { sketch, profiles, distance, direction, through_all, op } => {
            features::run_extrude(ctx, f, *sketch, profiles, distance, *direction, *through_all, *op)
        }
        FeatureKind::Revolve { sketch, profiles, axis, angle, op } => {
            features::run_revolve(ctx, f, *sketch, profiles, axis, angle, *op)
        }
        FeatureKind::Fillet { body, edges, radius } => features::run_fillet(ctx, *body, edges, radius, false),
        FeatureKind::Chamfer { body, edges, distance } => features::run_fillet(ctx, *body, edges, distance, true),
        FeatureKind::Shell { body, faces, thickness } => features::run_shell(ctx, *body, faces, thickness),
        FeatureKind::Hole { body, face, points, diameter, depth, counterbore, countersink } => features::run_hole(
            ctx,
            features::HoleSpec {
                body: *body,
                face,
                points,
                diameter,
                depth: depth.as_ref(),
                counterbore: counterbore.as_ref(),
                countersink: countersink.as_ref(),
            },
        ),
        FeatureKind::PressPull { body, face, distance } => features::run_press_pull(ctx, *body, face, distance),
    }
}

/// Regenerate the document on `kernel`. Never panics; per-feature errors are in `status`.
pub fn regen(doc: &Document, kernel: &dyn Kernel) -> Model {
    let mut model = Model::default();
    let params = match doc.parameter_values() {
        Ok(p) => p,
        Err(e) => {
            // A broken parameter table: every feature that needs it will fail with this message.
            log::warn!("parameters: {e}");
            HashMap::new()
        }
    };
    let mut ctx = Ctx { kernel, params, bodies: Vec::new(), sketches: Vec::new() };
    let active = doc.active_features().len();
    for (i, f) in doc.features.iter().enumerate() {
        let status = if i >= active {
            Status::RolledBack
        } else if f.suppressed {
            Status::Suppressed
        } else {
            match run(&mut ctx, f) {
                Ok(()) => Status::Ok,
                Err(e) => Status::Error(e),
            }
        };
        model.status.push(FeatureStatus { id: f.id, name: f.name.clone(), status });
    }
    model.sketches = std::mem::take(&mut ctx.sketches);
    for (id, shape) in ctx.bodies {
        let state = doc.bodies.get(&id);
        model.bodies.push(Body {
            id,
            name: state.map(|s| s.name.clone()).unwrap_or_else(|| doc.body_name(id)),
            visible: state.is_none_or(|s| s.visible),
            shape,
        });
    }
    model
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tests3;
