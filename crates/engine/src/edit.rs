//! Editing existing features and previewing commands.
//!
//! `feature.edit` runs the feature's creating command in a scratch session whose timeline marker
//! sits at the feature, takes the feature definition that produced and swaps it into the real
//! document (same id, name and body). The feature dialog and the AI both go through this.

use std::sync::Arc;

use formcraft_doc::{BodyOp, CombineOp, FeatureId, FeatureKind, ProfileSel, RevolveAxis, SketchPlane};
use formcraft_regen::{Model, Status};
use serde_json::{Value, json};

use crate::{CommandSpec, EngineError, Handler, Registry, Result, Session, params};

fn op_params(op: &BodyOp, v: &mut Value) {
    let (name, target) = match op {
        BodyOp::NewBody => ("new_body", None),
        BodyOp::Join(b) => ("join", Some(b)),
        BodyOp::Cut(b) => ("cut", Some(b)),
        BodyOp::Intersect(b) => ("intersect", Some(b)),
    };
    v["operation"] = json!(name);
    if let Some(b) = target {
        v["target"] = json!(b.0);
    }
}

fn profiles_param(p: &ProfileSel) -> Value {
    match p {
        ProfileSel::All => json!("all"),
        ProfileSel::Points { points } => json!(points),
        ProfileSel::Indices { indices } => json!(indices),
    }
}

/// The command that creates a feature of this kind and the parameters that recreate it.
/// Sketches are edited in sketch mode, not through a command dialog (None).
pub fn command_for(kind: &FeatureKind) -> Option<(String, Value)> {
    let x = |e: &formcraft_doc::Expr| json!(e.as_str());
    let v3 = |v: &formcraft_doc::Vec3Expr| json!([v.0[0].as_str(), v.0[1].as_str(), v.0[2].as_str()]);
    Some(match kind {
        FeatureKind::Box { corner, size, op } => {
            let mut v = json!({"corner": v3(corner), "size": v3(size)});
            op_params(op, &mut v);
            ("solid.box", v)
        }
        FeatureKind::Cylinder { base, axis, radius, height, op } => {
            let mut v = json!({"base": v3(base), "axis": v3(axis), "radius": x(radius), "height": x(height)});
            op_params(op, &mut v);
            ("solid.cylinder", v)
        }
        FeatureKind::Sphere { center, radius, op } => {
            let mut v = json!({"center": v3(center), "radius": x(radius)});
            op_params(op, &mut v);
            ("solid.sphere", v)
        }
        FeatureKind::Extrude { sketch, profiles, distance, direction, through_all, op } => {
            let dir = match direction {
                formcraft_doc::ExtrudeDirection::OneSide => "one_side",
                formcraft_doc::ExtrudeDirection::Symmetric => "symmetric",
            };
            let mut v = json!({"sketch": sketch.0, "profiles": profiles_param(profiles), "distance": x(distance), "direction": dir, "through_all": through_all});
            op_params(op, &mut v);
            ("solid.extrude", v)
        }
        FeatureKind::Revolve { sketch, profiles, axis, angle, op } => {
            let axis = match axis {
                RevolveAxis::Name(n) => json!(n),
                RevolveAxis::Line { line } => json!(line.0),
            };
            let mut v =
                json!({"sketch": sketch.0, "profiles": profiles_param(profiles), "axis": axis, "angle": x(angle)});
            op_params(op, &mut v);
            ("solid.revolve", v)
        }
        FeatureKind::Fillet { body, edges, radius } => {
            ("solid.fillet", json!({"body": body.0, "edges": edges, "radius": x(radius)}))
        }
        FeatureKind::Chamfer { body, edges, distance } => {
            ("solid.chamfer", json!({"body": body.0, "edges": edges, "distance": x(distance)}))
        }
        FeatureKind::Shell { body, faces, thickness } => {
            ("solid.shell", json!({"body": body.0, "faces": faces, "thickness": x(thickness)}))
        }
        FeatureKind::Hole { body, face, points, diameter, depth, counterbore, countersink } => {
            let mut v = json!({"body": body.0, "face": face, "points": points.iter().map(v3).collect::<Vec<_>>(), "diameter": x(diameter)});
            if let Some(d) = depth {
                v["depth"] = x(d);
            }
            if let Some((d, h)) = counterbore {
                v["counterbore"] = json!({"diameter": d.as_str(), "depth": h.as_str()});
            }
            if let Some((d, a)) = countersink {
                v["countersink"] = json!({"diameter": d.as_str(), "angle": a.as_str()});
            }
            ("solid.hole", v)
        }
        FeatureKind::PressPull { body, face, distance } => {
            ("solid.press_pull", json!({"body": body.0, "face": face, "distance": x(distance)}))
        }
        FeatureKind::Combine { target, tool, operation, keep_tool } => {
            let op = match operation {
                CombineOp::Join => "join",
                CombineOp::Cut => "cut",
                CombineOp::Intersect => "intersect",
            };
            ("solid.combine", json!({"target": target.0, "tool": tool.0, "operation": op, "keep_tools": keep_tool}))
        }
        FeatureKind::Sketch { plane: SketchPlane::Origin { .. } | SketchPlane::Face { .. }, .. } => return None,
        // An imported body has nothing to edit.
        FeatureKind::Import { .. } => return None,
        // A plugin feature is edited through the plugin's command `<plugin>.<kind>`.
        FeatureKind::Custom { plugin, kind, params, op, .. } => {
            let mut v = if params.is_object() { params.clone() } else { json!({}) };
            op_params(op, &mut v);
            return Some((format!("{plugin}.{kind}"), v));
        }
    })
    .map(|(c, v)| (c.to_string(), v))
}

impl Session {
    /// A throw-away session with the same kernel, commands and document (previews, edits).
    pub fn scratch(&self) -> Session {
        let mut s = Session::with_kernel(self.kernel.clone());
        s.registry = self.registry.clone();
        s.custom = self.custom.clone();
        s.doc = self.doc.clone();
        s.model = self.model.clone();
        s
    }

    /// The model a command would produce, without changing this session.
    pub fn preview(&self, id: &str, params: &Value) -> Result<Model> {
        let mut s = self.scratch();
        s.execute(id, params)?;
        Ok(s.model)
    }
}

/// Feature definition produced by running `cmd` with `params` at the feature's place.
fn rebuilt_kind(s: &Session, id: FeatureId, cmd: &str, params: &Value) -> Result<FeatureKind> {
    let idx = s.document().index_of(id).ok_or_else(|| EngineError::InvalidParams(format!("no feature {}", id.0)))?;
    let mut scratch = s.scratch();
    let mut d = (*scratch.doc).clone();
    d.set_marker(Some(idx));
    scratch.load(d);
    let out = scratch.execute(cmd, params)?;
    let new_id = out
        .get("feature")
        .and_then(Value::as_u64)
        .map(|v| FeatureId(v as u32))
        .ok_or_else(|| EngineError::Failed("the command did not create a feature".into()))?;
    scratch
        .document()
        .feature(new_id)
        .map(|f| f.kind.clone())
        .ok_or_else(|| EngineError::Failed("edited feature vanished".into()))
}

fn feature_edit(s: &mut Session, p: &Value) -> Result<Value> {
    let id = params::feature(s.document(), p, "feature")?;
    let (cmd, current) = s
        .document()
        .feature(id)
        .and_then(|f| command_for(&f.kind))
        .ok_or_else(|| EngineError::InvalidParams("this feature is edited in sketch mode".into()))?;
    // Missing keys keep their current values.
    let mut merged = current;
    if let (Some(m), Some(new)) = (merged.as_object_mut(), p.get("params").and_then(Value::as_object)) {
        for (k, v) in new {
            m.insert(k.clone(), v.clone());
        }
    }
    let kind = rebuilt_kind(s, id, &cmd, &merged)?;
    if s.document().feature(id).map(|f| std::mem::discriminant(&f.kind)) != Some(std::mem::discriminant(&kind)) {
        return Err(EngineError::Failed("edit changed the feature type".into()));
    }
    let name = s.document().feature(id).map(|f| f.name.clone()).unwrap_or_default();
    s.edit(
        &format!("Edit {name}"),
        |d| {
            let f = d.feature_mut(id).ok_or_else(|| EngineError::InvalidParams("feature vanished".into()))?;
            f.kind = kind;
            Ok(())
        },
        |_, m| match m.status.iter().find(|st| st.id == id).map(|st| &st.status) {
            Some(Status::Error(e)) => Err(EngineError::Failed(e.clone())),
            _ => Ok(()),
        },
    )?;
    Ok(json!({"feature": id.0, "errors": s.model().error_count(), "message": format!("{name} edited")}))
}

fn feature_params(s: &mut Session, p: &Value) -> Result<Value> {
    let id = params::feature(s.document(), p, "feature")?;
    let f = s.document().feature(id).ok_or_else(|| EngineError::InvalidParams("no such feature".into()))?;
    match command_for(&f.kind) {
        Some((cmd, v)) => Ok(json!({"feature": id.0, "name": f.name, "command": cmd, "params": v})),
        None => Ok(json!({"feature": id.0, "name": f.name, "command": null, "definition": f.kind})),
    }
}

pub(crate) fn register(r: &mut Registry) {
    let cmds: Vec<(CommandSpec, Handler)> = vec![
        (
            CommandSpec::new("feature.edit", "Edit Feature", "Change parameters of a feature that already exists on the timeline, by its name (e.g. Extrude1); missing params keep their values. Not needed while building a new part.")
                .params(json!({"type": "object", "required": ["feature", "params"], "properties": {
                    "feature": {"type": ["integer", "string"]}, "params": {"type": "object"}}}))
                .ai()
                .undoable(),
            Arc::new(feature_edit),
        ),
        (
            CommandSpec::new("feature.params", "Feature Parameters", "The command and parameters that define a feature (for editing).")
                .params(json!({"type": "object", "required": ["feature"], "properties": {"feature": {"type": ["integer", "string"]}}})),
            Arc::new(feature_params),
        ),
    ];
    for (spec, h) in cmds {
        if let Err(e) = r.register(spec, h) {
            log::error!("command failed to register: {e}");
        }
    }
}
