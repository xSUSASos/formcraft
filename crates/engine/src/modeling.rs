//! Modelling and document commands (Phase 2): primitives, combine, undo/redo, export,
//! parameters, timeline edits, bodies, inspection, selection.

use std::path::PathBuf;
use std::sync::Arc;

use formcraft_doc::{BodyOp, CombineOp, Document, Expr, FeatureId, FeatureKind};
use formcraft_regen::{Model, Status};
use serde_json::{Value, json};

use crate::params::{self, schema};
use crate::{CommandSpec, EngineError, Handler, Pick, Registry, Result, Session};

fn h(f: fn(&mut Session, &Value) -> Result<Value>) -> Handler {
    Arc::new(f)
}

/// Refuse an edit whose new feature failed to regenerate (so programmatic callers — the AI,
/// MCP, scripts — get an error instead of a red feature).
fn feature_ok(id: &FeatureId, m: &Model) -> Result<()> {
    match m.status.iter().find(|s| s.id == *id).map(|s| &s.status) {
        Some(Status::Error(e)) => Err(EngineError::Failed(e.clone())),
        _ => Ok(()),
    }
}

/// Add a feature as one undo step; report the feature and the body it created or changed.
pub(crate) fn add_feature(s: &mut Session, kind: FeatureKind) -> Result<Value> {
    let label = format!("Add {}", kind.base_name());
    let id = s.edit(&label, |d| Ok(d.add_feature(kind)?), feature_ok)?;
    let d = s.document();
    let f = d.feature(id);
    let name = f.map(|f| f.name.clone()).unwrap_or_default();
    let body = f.and_then(|f| match f.kind.op() {
        Some(BodyOp::NewBody) => Some(formcraft_doc::BodyId(f.id.0)),
        Some(op) => op.target(),
        None => None,
    });
    Ok(json!({
        "feature": id.0,
        "name": name,
        "body": body.map(|b| b.0),
        "body_name": body.map(|b| d.body_name(b)),
        "message": format!("{name} added"),
    }))
}

fn diameter_or_radius(p: &Value) -> Result<Expr> {
    match (params::opt_expr(p, "radius")?, params::opt_expr(p, "diameter")?) {
        (Some(r), None) => Ok(r),
        (None, Some(d)) => Ok(Expr(format!("({}) / 2", d.0))),
        (Some(_), Some(_)) => Err(EngineError::InvalidParams("give `radius` or `diameter`, not both".into())),
        (None, None) => Err(EngineError::InvalidParams("missing `radius` or `diameter`".into())),
    }
}

fn solid_box(s: &mut Session, p: &Value) -> Result<Value> {
    let kind = FeatureKind::Box {
        corner: params::vec3(p, "corner", Some([0.0; 3]))?,
        size: params::vec3(p, "size", None)?,
        op: params::body_op(s.document(), p)?,
    };
    add_feature(s, kind)
}

fn solid_cylinder(s: &mut Session, p: &Value) -> Result<Value> {
    let kind = FeatureKind::Cylinder {
        base: params::vec3(p, "base", Some([0.0; 3]))?,
        axis: params::vec3(p, "axis", Some([0.0, 0.0, 1.0]))?,
        radius: diameter_or_radius(p)?,
        height: params::expr(p, "height")?,
        op: params::body_op(s.document(), p)?,
    };
    add_feature(s, kind)
}

fn solid_sphere(s: &mut Session, p: &Value) -> Result<Value> {
    let kind = FeatureKind::Sphere {
        center: params::vec3(p, "center", Some([0.0; 3]))?,
        radius: diameter_or_radius(p)?,
        op: params::body_op(s.document(), p)?,
    };
    add_feature(s, kind)
}

fn solid_combine(s: &mut Session, p: &Value) -> Result<Value> {
    let d = s.document();
    let target = params::body(d, p, "target")?;
    let tools: Vec<_> = match (p.get("tools"), p.get("tool")) {
        (Some(Value::Array(a)), _) => a.iter().map(|v| params::body_ref(d, v, "tools")).collect::<Result<_>>()?,
        (_, Some(v)) => vec![params::body_ref(d, v, "tool")?],
        _ => return Err(EngineError::InvalidParams("missing `tool` or `tools`".into())),
    };
    if tools.is_empty() || tools.len() > 100 {
        return Err(EngineError::InvalidParams("give 1 to 100 tool bodies".into()));
    }
    let operation = match params::opt_str(p, "operation").unwrap_or("join") {
        "join" => CombineOp::Join,
        "cut" => CombineOp::Cut,
        "intersect" => CombineOp::Intersect,
        o => return Err(EngineError::InvalidParams(format!("unknown operation `{o}` (join, cut, intersect)"))),
    };
    let keep_tool = params::opt_bool(p, "keep_tools")?.unwrap_or(false);
    // Several tools = several combine features, added as one undo step.
    let ids = s.edit(
        "Combine",
        |d| {
            tools
                .iter()
                .map(|t| Ok(d.add_feature(FeatureKind::Combine { target, tool: *t, operation, keep_tool })?))
                .collect::<Result<Vec<FeatureId>>>()
        },
        |ids, m| ids.iter().try_for_each(|id| feature_ok(id, m)),
    )?;
    Ok(json!({"features": ids.iter().map(|i| i.0).collect::<Vec<_>>(), "body": target.0, "message": "Combine done"}))
}

fn doc_undo(s: &mut Session, _: &Value) -> Result<Value> {
    let l = s.undo()?;
    Ok(json!({"message": format!("Undo: {l}")}))
}

fn doc_redo(s: &mut Session, _: &Value) -> Result<Value> {
    let l = s.redo()?;
    Ok(json!({"message": format!("Redo: {l}")}))
}

fn doc_new(s: &mut Session, p: &Value) -> Result<Value> {
    let name = params::opt_str(p, "name").unwrap_or("Untitled");
    s.load(Document::new(name));
    Ok(json!({"message": format!("New design `{name}`")}))
}

/// Visible bodies (or the listed ones).
fn export_shapes(s: &Session, p: &Value) -> Result<Vec<formcraft_kernel::Shape>> {
    let d = s.document();
    let m = s.model();
    match p.get("bodies") {
        Some(Value::Array(a)) => a
            .iter()
            .map(|v| {
                let id = params::body_ref(d, v, "bodies")?;
                m.body(id)
                    .map(|b| b.shape.clone())
                    .ok_or_else(|| EngineError::Failed(format!("body {} failed to build", d.body_name(id))))
            })
            .collect(),
        _ => Ok(m.bodies.iter().filter(|b| b.visible).map(|b| b.shape.clone()).collect()),
    }
}

fn doc_export(s: &mut Session, p: &Value) -> Result<Value> {
    let path = PathBuf::from(params::str(p, "path")?);
    let shapes = export_shapes(s, p)?;
    formcraft_io::export(s.kernel().as_ref(), &shapes, &path)?;
    Ok(
        json!({"path": path.display().to_string(), "bodies": shapes.len(), "message": format!("Exported {}", path.display())}),
    )
}

fn doc_save(s: &mut Session, p: &Value) -> Result<Value> {
    let path = match params::opt_str(p, "path") {
        Some(x) => PathBuf::from(x),
        None => s
            .file()
            .map(PathBuf::from)
            .ok_or_else(|| EngineError::InvalidParams("missing `path` (the design was never saved)".into()))?,
    };
    let path = if path.extension().is_none() { path.with_extension("fcraft") } else { path };
    formcraft_io::fcraft::save(s.document(), None, &path)?;
    s.mark_saved(path.clone());
    Ok(json!({"path": path.display().to_string(), "message": format!("Saved {}", path.display())}))
}

fn doc_open(s: &mut Session, p: &Value) -> Result<Value> {
    let path = PathBuf::from(params::str(p, "path")?);
    let (doc, _) = formcraft_io::fcraft::open(&path)?;
    s.load(doc);
    s.mark_saved(path.clone());
    let errors = s.model().error_count();
    Ok(json!({"path": path.display().to_string(), "errors": errors, "message": format!("Opened {}", path.display())}))
}

fn doc_parameter(s: &mut Session, p: &Value) -> Result<Value> {
    let name = params::str(p, "name")?.to_string();
    let e = params::expr(p, "expression")?;
    let comment = params::opt_str(p, "comment").map(str::to_string);
    s.edit(&format!("Parameter {name}"), |d| Ok(d.set_parameter(&name, e, comment.as_deref())?), |_, _| Ok(()))?;
    let v = s.document().parameter_values().ok().and_then(|m| m.get(&name).copied());
    Ok(
        json!({"name": name, "value": v, "message": format!("{name} = {}", v.map(formcraft_doc::expr::fmt_num).unwrap_or_default())}),
    )
}

fn doc_parameters(s: &mut Session, _: &Value) -> Result<Value> {
    let d = s.document();
    let vals = d.parameter_values().unwrap_or_default();
    let list: Vec<Value> = d
        .parameters
        .iter()
        .map(|p| json!({"name": p.name, "expression": p.expr.as_str(), "value": vals.get(&p.name), "comment": p.comment}))
        .collect();
    Ok(json!({"parameters": list}))
}

fn doc_delete_parameter(s: &mut Session, p: &Value) -> Result<Value> {
    let name = params::str(p, "name")?.to_string();
    s.edit(&format!("Delete parameter {name}"), |d| Ok(d.remove_parameter(&name)?), |_, _| Ok(()))?;
    Ok(json!({"message": format!("Parameter {name} deleted")}))
}

fn feature_delete(s: &mut Session, p: &Value) -> Result<Value> {
    let id = params::feature(s.document(), p, "feature")?;
    s.edit("Delete feature", |d| Ok(d.delete_feature(id)?), |_, _| Ok(()))?;
    Ok(json!({"message": "Feature deleted"}))
}

fn feature_suppress(s: &mut Session, p: &Value) -> Result<Value> {
    let id = params::feature(s.document(), p, "feature")?;
    let on = params::opt_bool(p, "suppressed")?.unwrap_or(true);
    s.edit(
        if on { "Suppress feature" } else { "Unsuppress feature" },
        |d| {
            let f = d.feature_mut(id).ok_or_else(|| EngineError::InvalidParams("no such feature".into()))?;
            f.suppressed = on;
            Ok(())
        },
        |_, _| Ok(()),
    )?;
    Ok(json!({"message": if on { "Suppressed" } else { "Unsuppressed" }}))
}

fn feature_rename(s: &mut Session, p: &Value) -> Result<Value> {
    let id = params::feature(s.document(), p, "feature")?;
    let name = params::str(p, "name")?.trim().to_string();
    if name.is_empty() || name.len() > 100 {
        return Err(EngineError::InvalidParams("name must be 1..100 characters".into()));
    }
    s.edit(
        "Rename feature",
        |d| {
            if d.features.iter().any(|f| f.name == name && f.id != id) {
                return Err(EngineError::InvalidParams(format!("a feature named `{name}` exists")));
            }
            if let Some(f) = d.feature_mut(id) {
                f.name = name.clone();
            }
            Ok(())
        },
        |_, _| Ok(()),
    )?;
    Ok(json!({"message": format!("Renamed to {name}")}))
}

fn timeline_marker(s: &mut Session, p: &Value) -> Result<Value> {
    let at = match p.get("index") {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            v.as_u64()
                .and_then(|i| usize::try_from(i).ok())
                .ok_or_else(|| EngineError::InvalidParams("`index` must be >= 0".into()))?,
        ),
    };
    s.edit(
        "Move marker",
        |d| {
            d.set_marker(at);
            Ok(())
        },
        |_, _| Ok(()),
    )?;
    Ok(json!({"message": match at { Some(i) => format!("Marker before feature {i}"), None => "Marker at end".into() }}))
}

fn timeline_reorder(s: &mut Session, p: &Value) -> Result<Value> {
    let id = params::feature(s.document(), p, "feature")?;
    let to = p
        .get("index")
        .and_then(Value::as_u64)
        .and_then(|i| usize::try_from(i).ok())
        .ok_or_else(|| EngineError::InvalidParams("missing `index`".into()))?;
    s.edit("Reorder", |d| Ok(d.reorder(id, to)?), |_, _| Ok(()))?;
    Ok(json!({"message": "Reordered", "errors": s.model().error_count()}))
}

fn body_rename(s: &mut Session, p: &Value) -> Result<Value> {
    let b = params::body(s.document(), p, "body")?;
    let name = params::str(p, "name")?.trim().to_string();
    if name.is_empty() || name.len() > 100 {
        return Err(EngineError::InvalidParams("name must be 1..100 characters".into()));
    }
    s.edit(
        "Rename body",
        |d| {
            if let Some(st) = d.bodies.get_mut(&b) {
                st.name = name.clone();
            }
            Ok(())
        },
        |_, _| Ok(()),
    )?;
    Ok(json!({"message": format!("Body renamed to {name}")}))
}

fn body_visibility(s: &mut Session, p: &Value) -> Result<Value> {
    let b = params::body(s.document(), p, "body")?;
    let cur = s.document().bodies.get(&b).is_none_or(|st| st.visible);
    let visible = params::opt_bool(p, "visible")?.unwrap_or(!cur);
    s.edit(
        if visible { "Show body" } else { "Hide body" },
        |d| {
            if let Some(st) = d.bodies.get_mut(&b) {
                st.visible = visible;
            }
            Ok(())
        },
        |_, _| Ok(()),
    )?;
    Ok(json!({"visible": visible, "message": if visible { "Shown" } else { "Hidden" }}))
}

/// Everything an agent needs to verify a model: bodies with exact volume, area, bbox and
/// topology counts; features with their status; parameters.
pub fn inspect(s: &Session) -> Result<Value> {
    let k = s.kernel();
    let d = s.document();
    let mut bodies = Vec::new();
    for b in &s.model().bodies {
        let mp = k.mass_props(&b.shape)?;
        let faces = k.faces(&b.shape)?;
        let edges = k.edges(&b.shape)?;
        let holes = faces.iter().filter(|f| matches!(f.kind, formcraft_kernel::SurfaceKind::Cylinder { .. })).count();
        bodies.push(json!({
            "id": b.id.0,
            "name": b.name,
            "visible": b.visible,
            "volume": round(mp.volume),
            "area": round(mp.area),
            "centroid": [round(mp.centroid.x), round(mp.centroid.y), round(mp.centroid.z)],
            "bbox": {"min": [round(mp.bbox.min.x), round(mp.bbox.min.y), round(mp.bbox.min.z)],
                     "max": [round(mp.bbox.max.x), round(mp.bbox.max.y), round(mp.bbox.max.z)],
                     "size": [round(mp.bbox.size().x), round(mp.bbox.size().y), round(mp.bbox.size().z)]},
            "faces": faces.len(),
            "edges": edges.len(),
            "cylindrical_faces": holes,
            "valid": k.is_valid(&b.shape),
        }));
    }
    let features: Vec<Value> = s
        .model()
        .status
        .iter()
        .map(|st| {
            let kind = d.feature(st.id).map(|f| f.kind.base_name()).unwrap_or("");
            json!({"id": st.id.0, "name": st.name, "type": kind, "status": st.status})
        })
        .collect();
    let vals = d.parameter_values().unwrap_or_default();
    let parameters: Vec<Value> = d
        .parameters
        .iter()
        .map(|p| json!({"name": p.name, "expression": p.expr.as_str(), "value": vals.get(&p.name)}))
        .collect();
    Ok(json!({
        "document": d.name,
        "units": d.units,
        "bodies": bodies,
        "features": features,
        "parameters": parameters,
        "errors": s.model().error_count(),
        "marker": d.marker,
    }))
}

fn round(v: f64) -> f64 {
    (v * 1e4).round() / 1e4
}

fn model_inspect(s: &mut Session, _: &Value) -> Result<Value> {
    inspect(s)
}

fn select_set(s: &mut Session, p: &Value) -> Result<Value> {
    let picks: Vec<Pick> = match p.get("picks") {
        Some(v) => {
            serde_json::from_value(v.clone()).map_err(|e| EngineError::InvalidParams(format!("`picks`: {e}")))?
        }
        None => Vec::new(),
    };
    if picks.len() > crate::selection::MAX_SELECTION {
        return Err(EngineError::InvalidParams("too many picks".into()));
    }
    for pk in &picks {
        if s.model().body(pk.body()).is_none() {
            return Err(EngineError::InvalidParams(format!("no body {}", pk.body().0)));
        }
    }
    let sel = s.selection_mut();
    sel.items = picks;
    Ok(json!({"selected": sel.items.len()}))
}

fn select_clear(s: &mut Session, _: &Value) -> Result<Value> {
    s.selection_mut().clear();
    Ok(json!({"selected": 0}))
}

pub(crate) fn register(r: &mut Registry) {
    let op_props = |extra: Value| {
        let mut props = extra;
        if let Some(o) = props.as_object_mut() {
            o.insert("operation".into(), schema::operation());
            o.insert("target".into(), schema::body("Body to join/cut/intersect with (id or name)."));
        }
        props
    };
    let cmds: Vec<(CommandSpec, Handler)> = vec![
        (
            CommandSpec::new("solid.box", "Box", "Create an axis-aligned box from a corner and a size [x, y, z] (mm). Values may be expressions with parameters.")
                .params(json!({"type": "object", "required": ["size"], "properties": op_props(json!({
                    "corner": schema::vec3("Corner [x, y, z], default [0, 0, 0]."),
                    "size": schema::vec3("Size [dx, dy, dz]; negative values extend the other way."),
                }))}))
                .place("Solid", "Create", false)
                .icon("box")
                .ai()
                .undoable(),
            h(solid_box),
        ),
        (
            CommandSpec::new("solid.cylinder", "Cylinder", "Create a cylinder from its base centre, axis direction, radius (or diameter) and height.")
                .params(json!({"type": "object", "required": ["height"], "properties": op_props(json!({
                    "base": schema::vec3("Centre of the base [x, y, z], default origin."),
                    "axis": schema::vec3("Axis direction, default [0, 0, 1]."),
                    "radius": schema::num("Radius."),
                    "diameter": schema::num("Diameter (instead of radius)."),
                    "height": schema::num("Height along the axis."),
                }))}))
                .place("Solid", "Create", false)
                .icon("cylinder")
                .ai()
                .undoable(),
            h(solid_cylinder),
        ),
        (
            CommandSpec::new("solid.sphere", "Sphere", "Create a sphere from its centre and radius (or diameter).")
                .params(json!({"type": "object", "properties": op_props(json!({
                    "center": schema::vec3("Centre [x, y, z], default origin."),
                    "radius": schema::num("Radius."),
                    "diameter": schema::num("Diameter (instead of radius)."),
                }))}))
                .place("Solid", "Create", false)
                .icon("sphere")
                .ai()
                .undoable(),
            h(solid_sphere),
        ),
        (
            CommandSpec::new("solid.combine", "Combine", "Join, cut or intersect tool bodies with a target body.")
                .params(json!({"type": "object", "required": ["target"], "properties": {
                    "target": schema::body("Target body (id or name)."),
                    "tool": schema::body("Tool body."),
                    "tools": {"type": "array", "items": {"type": ["integer", "string"]}, "description": "Several tool bodies."},
                    "operation": {"type": "string", "enum": ["join", "cut", "intersect"], "default": "join"},
                    "keep_tools": {"type": "boolean", "default": false},
                }}))
                .place("Solid", "Modify", false)
                .icon("combine")
                .ai()
                .undoable(),
            h(solid_combine),
        ),
        (CommandSpec::new("doc.undo", "Undo", "Undo the last change.").shortcut("Ctrl+Z").icon("undo"), h(doc_undo)),
        (CommandSpec::new("doc.redo", "Redo", "Redo the last undone change.").shortcut("Ctrl+Y").icon("redo"), h(doc_redo)),
        (
            CommandSpec::new("doc.new", "New Design", "Start an empty design (the current one is discarded).")
                .params(json!({"type": "object", "properties": {"name": {"type": "string"}}})),
            h(doc_new),
        ),
        (
            CommandSpec::new("doc.save", "Save", "Save the design as .fcraft (to `path`, or to the file it came from).")
                .params(json!({"type": "object", "properties": {"path": {"type": "string"}}}))
                .shortcut("Ctrl+S"),
            h(doc_save),
        ),
        (
            CommandSpec::new("doc.open", "Open", "Open a .fcraft design (the current one is replaced).")
                .params(json!({"type": "object", "required": ["path"], "properties": {"path": {"type": "string"}}}))
                .shortcut("Ctrl+O"),
            h(doc_open),
        ),
        (
            CommandSpec::new("doc.export", "Export", "Export visible bodies (or `bodies`) to STEP (.step/.stp) or STL (.stl).")
                .params(json!({"type": "object", "required": ["path"], "properties": {
                    "path": {"type": "string"},
                    "bodies": {"type": "array", "items": {"type": ["integer", "string"]}},
                }})),
            h(doc_export),
        ),
        (
            CommandSpec::new("doc.parameter", "Set Parameter", "Create or change a user parameter, e.g. plate_length = 60 or hole_d = plate_length / 10.")
                .params(json!({"type": "object", "required": ["name", "expression"], "properties": {
                    "name": {"type": "string", "description": "Letters, digits and _, e.g. plate_length."},
                    "expression": schema::num("Value or expression (mm, deg)."),
                    "comment": {"type": "string"},
                }}))
                .place("Solid", "Modify", false)
                .icon("parameters")
                .ai()
                .undoable(),
            h(doc_parameter),
        ),
        (CommandSpec::new("doc.parameters", "Parameters", "List user parameters with their values."), h(doc_parameters)),
        (
            CommandSpec::new("doc.delete_parameter", "Delete Parameter", "Delete an unused user parameter.")
                .params(json!({"type": "object", "required": ["name"], "properties": {"name": {"type": "string"}}}))
                .undoable(),
            h(doc_delete_parameter),
        ),
        (
            CommandSpec::new("feature.delete", "Delete Feature", "Delete a feature from the timeline.")
                .params(json!({"type": "object", "required": ["feature"], "properties": {"feature": {"type": ["integer", "string"]}}}))
                .undoable(),
            h(feature_delete),
        ),
        (
            CommandSpec::new("feature.suppress", "Suppress Feature", "Suppress (skip) or unsuppress a feature.")
                .params(json!({"type": "object", "required": ["feature"], "properties": {"feature": {"type": ["integer", "string"]}, "suppressed": {"type": "boolean", "default": true}}}))
                .undoable(),
            h(feature_suppress),
        ),
        (
            CommandSpec::new("feature.rename", "Rename Feature", "Rename a timeline feature.")
                .params(json!({"type": "object", "required": ["feature", "name"], "properties": {"feature": {"type": ["integer", "string"]}, "name": {"type": "string"}}}))
                .undoable(),
            h(feature_rename),
        ),
        (
            CommandSpec::new("timeline.marker", "Move Timeline Marker", "Roll the timeline back to before feature `index` (null = end).")
                .params(json!({"type": "object", "properties": {"index": {"type": ["integer", "null"]}}}))
                .undoable(),
            h(timeline_marker),
        ),
        (
            CommandSpec::new("timeline.reorder", "Reorder Feature", "Move a feature to another position in the timeline.")
                .params(json!({"type": "object", "required": ["feature", "index"], "properties": {"feature": {"type": ["integer", "string"]}, "index": {"type": "integer", "minimum": 0}}}))
                .undoable(),
            h(timeline_reorder),
        ),
        (
            CommandSpec::new("body.rename", "Rename Body", "Rename a body.")
                .params(json!({"type": "object", "required": ["body", "name"], "properties": {"body": schema::body("Body"), "name": {"type": "string"}}}))
                .undoable(),
            h(body_rename),
        ),
        (
            CommandSpec::new("body.visibility", "Show/Hide Body", "Show or hide a body (toggles when `visible` is omitted).")
                .params(json!({"type": "object", "required": ["body"], "properties": {"body": schema::body("Body"), "visible": {"type": "boolean"}}}))
                .undoable(),
            h(body_visibility),
        ),
        (
            CommandSpec::new("model.inspect", "Inspect Model", "Bodies with exact volume, area, bounding box and topology counts; features with status; parameters.")
                .place("Inspect", "Inspect", false)
                .icon("properties"),
            h(model_inspect),
        ),
        (
            CommandSpec::new("select.set", "Select", "Replace the selection: picks like {type: body, body: 1} or {type: face, body: 1, index: 3}.")
                .params(json!({"type": "object", "properties": {"picks": {"type": "array"}}})),
            h(select_set),
        ),
        (CommandSpec::new("select.clear", "Clear Selection", "Deselect everything."), h(select_clear)),
    ];
    for (spec, handler) in cmds {
        if let Err(e) = r.register(spec, handler) {
            log::error!("modeling command failed to register: {e}");
        }
    }
}
