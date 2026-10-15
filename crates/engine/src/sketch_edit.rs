//! Interactive sketch editing: one undoable `sketch.edit` applies a list of operations to a
//! sketch feature and re-solves it (refusing edits that make the sketch unsolvable).

use std::sync::Arc;

use formcraft_doc::{Expr, FeatureId, FeatureKind};
use formcraft_sketch::{self as sk, Constraint, DimKind, Eid, Entity, Sketch};
use serde_json::{Value, json};

use crate::{CommandSpec, EngineError, Registry, Result, Session, params};

fn bad(m: impl Into<String>) -> EngineError {
    EngineError::InvalidParams(m.into())
}

const MAX_OPS: usize = 5_000;

fn xy(v: Option<&Value>, key: &str) -> Result<(f64, f64)> {
    let a = v.and_then(Value::as_array).ok_or_else(|| bad(format!("`{key}` must be [x, y]")))?;
    match a.as_slice() {
        [x, y] => {
            let (x, y) = (x.as_f64().unwrap_or(f64::NAN), y.as_f64().unwrap_or(f64::NAN));
            if x.is_finite() && y.is_finite() {
                Ok((x, y))
            } else {
                Err(bad(format!("`{key}` must be finite numbers")))
            }
        }
        _ => Err(bad(format!("`{key}` must be [x, y]"))),
    }
}

fn eid(v: &Value, key: &str) -> Result<Eid> {
    v.get(key)
        .and_then(Value::as_u64)
        .and_then(|i| u32::try_from(i).ok())
        .map(Eid)
        .ok_or_else(|| bad(format!("`{key}` must be an entity id")))
}

/// Reuse an existing point (`<key>_point`) or create one at `<key>`.
fn point(s: &mut Sketch, op: &Value, key: &str) -> Result<Eid> {
    if let Some(id) = op.get(format!("{key}_point")).and_then(Value::as_u64) {
        let id = Eid(u32::try_from(id).map_err(|_| bad("bad point id"))?);
        s.point(id).map_err(|e| bad(e.to_string()))?;
        return Ok(id);
    }
    let (x, y) = xy(op.get(key), key)?;
    s.add_point(x, y).map_err(|e| bad(e.to_string()))
}

/// Apply one operation; returns ids it created.
fn apply(s: &mut Sketch, op: &Value) -> Result<Vec<u32>> {
    let kind = params::str(op, "op")?;
    let e = |r: std::result::Result<Eid, sk::SketchError>| r.map_err(|e| bad(e.to_string()));
    Ok(match kind {
        "point" => vec![point(s, op, "at")?.0],
        "line" => {
            let a = point(s, op, "a")?;
            let b = point(s, op, "b")?;
            let l = e(s.add_line(a, b))?;
            // Inferred constraints from interactive drawing.
            if op.get("horizontal").and_then(Value::as_bool) == Some(true) {
                s.add_constraint(Constraint::Horizontal { line: l }).map_err(|e| bad(e.to_string()))?;
            }
            if op.get("vertical").and_then(Value::as_bool) == Some(true) {
                s.add_constraint(Constraint::Vertical { line: l }).map_err(|e| bad(e.to_string()))?;
            }
            vec![a.0, b.0, l.0]
        }
        "rectangle" => {
            let (x0, y0) = xy(op.get("a"), "a")?;
            let (x1, y1) = xy(op.get("b"), "b")?;
            if (x1 - x0).abs() < 1e-9 || (y1 - y0).abs() < 1e-9 {
                return Err(bad("rectangle is degenerate"));
            }
            s.add_rectangle(x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1))
                .map_err(|e| bad(e.to_string()))?
                .iter()
                .map(|i| i.0)
                .collect()
        }
        "circle" => {
            let c = point(s, op, "center")?;
            let r = op.get("radius").and_then(Value::as_f64).ok_or_else(|| bad("circle needs `radius`"))?;
            let id = e(s.add_circle(c, r))?;
            vec![c.0, id.0]
        }
        "arc" => {
            let c = point(s, op, "center")?;
            let a = point(s, op, "start")?;
            let b = point(s, op, "end")?;
            let id = e(s.add_arc(c, a, b))?;
            vec![c.0, a.0, b.0, id.0]
        }
        "constraint" => {
            let c: Constraint = serde_json::from_value(op.get("constraint").cloned().unwrap_or(Value::Null))
                .map_err(|e| bad(format!("constraint: {e}")))?;
            s.add_constraint(c).map_err(|e| bad(e.to_string()))?;
            vec![]
        }
        "dimension" => {
            let d: DimKind = serde_json::from_value(op.get("dimension").cloned().unwrap_or(Value::Null))
                .map_err(|e| bad(format!("dimension: {e}")))?;
            let value = params::expr_of(op.get("value").unwrap_or(&Value::Null), "value")?;
            s.add_dimension(d, value.as_str()).map_err(|e| bad(e.to_string()))?;
            vec![]
        }
        "set_dimension" => {
            let i = op.get("index").and_then(Value::as_u64).ok_or_else(|| bad("`index` of the dimension"))? as usize;
            let value = params::expr_of(op.get("value").unwrap_or(&Value::Null), "value")?;
            let d = s.dimensions.get_mut(i).ok_or_else(|| bad("no such dimension"))?;
            d.value = value.0;
            vec![]
        }
        "delete" => {
            let id = eid(op, "id")?;
            if s.get(id).is_none() {
                return Err(bad(format!("no entity {}", id.0)));
            }
            s.delete(id);
            vec![]
        }
        "delete_constraint" => {
            let i = op.get("index").and_then(Value::as_u64).ok_or_else(|| bad("`index`"))? as usize;
            if i >= s.constraints.len() {
                return Err(bad("no such constraint"));
            }
            s.constraints.remove(i);
            vec![]
        }
        "move" => {
            let id = eid(op, "point")?;
            let (x, y) = xy(op.get("to"), "to")?;
            match s.get_mut(id) {
                Some(Entity::Point { x: px, y: py }) => {
                    *px = x;
                    *py = y;
                }
                _ => return Err(bad("`point` is not a point")),
            }
            vec![]
        }
        "construction" => {
            let id = eid(op, "id")?;
            let on = op.get("on").and_then(Value::as_bool).unwrap_or(true);
            match s.get_mut(id) {
                Some(
                    Entity::Line { construction, .. }
                    | Entity::Circle { construction, .. }
                    | Entity::Arc { construction, .. },
                ) => *construction = on,
                _ => return Err(bad("only curves can be construction geometry")),
            }
            vec![]
        }
        other => return Err(bad(format!("unknown sketch op `{other}`"))),
    })
}

fn sketch_edit(s: &mut Session, p: &Value) -> Result<Value> {
    let id = params::feature(s.document(), p, "sketch")?;
    let ops = p.get("ops").and_then(Value::as_array).ok_or_else(|| bad("`ops` must be an array"))?.clone();
    if ops.len() > MAX_OPS {
        return Err(bad("too many ops"));
    }
    let values = s.document().parameter_values().unwrap_or_default();
    let (created, report) = s.edit(
        "Edit sketch",
        |d| {
            let f = d.feature_mut(id).ok_or_else(|| bad("no such sketch"))?;
            let FeatureKind::Sketch { sketch, .. } = &mut f.kind else { return Err(bad("not a sketch")) };
            let mut created = Vec::new();
            for op in &ops {
                created.push(apply(sketch, op)?);
            }
            // Solve now so the stored geometry is what the user sees.
            let dims: Vec<f64> = sketch
                .dimensions
                .iter()
                .map(|dm| {
                    Expr(dm.value.clone())
                        .eval(&|n| values.get(n).copied())
                        .map_err(|e| bad(format!("dimension `{}`: {e}", dm.value)))
                })
                .collect::<Result<_>>()?;
            let report = sk::solve(sketch, &dims);
            if !report.converged {
                return Err(EngineError::Failed("this makes the sketch over-constrained or unsolvable".into()));
            }
            Ok((created, report))
        },
        |_, _| Ok(()),
    )?;
    let regions = s.model().sketch(id).map(|r| r.regions.len()).unwrap_or(0);
    Ok(
        json!({"created": created, "dof": report.dof, "profiles": regions, "message": format!("Sketch: {} DOF", report.dof)}),
    )
}

/// The solved sketch geometry (for clients that draw sketches).
fn sketch_get(s: &mut Session, p: &Value) -> Result<Value> {
    let id: FeatureId = params::feature(s.document(), p, "sketch")?;
    let r = s.model().sketch(id).ok_or_else(|| bad("sketch is missing or failed"))?;
    Ok(json!({
        "plane": {"origin": r.plane.origin.to_array(), "x_dir": r.plane.x_dir.to_array(), "normal": r.plane.normal.to_array()},
        "sketch": r.sketch,
        "dof": r.dof,
        "regions": r.regions.iter().map(|g| json!({"area": g.area, "depth": g.depth, "sample": [g.sample.x, g.sample.y]})).collect::<Vec<_>>(),
    }))
}

pub(crate) fn register(r: &mut Registry) {
    let cmds: Vec<(CommandSpec, crate::Handler)> = vec![
        (
            CommandSpec::new(
                "sketch.edit",
                "Edit Sketch",
                "Apply operations to a sketch as one step: point, line, rectangle, circle, arc, constraint, dimension, set_dimension, delete, delete_constraint, move, construction. Reuse points with `a_point`/`b_point`/`center_point` ids.",
            )
            .params(json!({"type": "object", "required": ["sketch", "ops"], "properties": {
                "sketch": {"type": ["integer", "string"]}, "ops": {"type": "array", "items": {"type": "object"}}}}))
            .undoable(),
            Arc::new(sketch_edit),
        ),
        (
            CommandSpec::new("sketch.get", "Sketch Geometry", "Solved geometry, constraints, dimensions and profiles of a sketch.")
                .params(json!({"type": "object", "required": ["sketch"], "properties": {"sketch": {"type": ["integer", "string"]}}})),
            Arc::new(sketch_get),
        ),
    ];
    for (spec, h) in cmds {
        if let Err(e) = r.register(spec, h) {
            log::error!("command failed to register: {e}");
        }
    }
}
