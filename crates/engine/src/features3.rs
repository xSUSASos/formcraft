//! Phase 3 commands: sketches and sketch-based / dress-up features.

use std::sync::Arc;

use formcraft_doc::{
    BodyId, Document, EdgeSel, Expr, ExtrudeDirection, FaceSel, FeatureId, FeatureKind, ProfileSel, RevolveAxis,
    SketchPlane,
};
use formcraft_sketch::{Constraint, DimKind, Eid, Entity, Sketch};
use serde_json::{Value, json};

use crate::modeling::add_feature;
use crate::params::{self, schema};
use crate::{CommandSpec, EngineError, Handler, Registry, Result, Session};

fn bad(m: impl Into<String>) -> EngineError {
    EngineError::InvalidParams(m.into())
}

const MAX_ITEMS: usize = 2_000;

/// Evaluate a coordinate for the initial geometry; strings stay as driving dimensions.
fn coord(d: &Document, v: &Value, key: &str) -> Result<(f64, Option<Expr>)> {
    let e = params::expr_of(v, key)?;
    let n = d.eval(&e).map_err(|err| bad(format!("`{key}`: {err}")))?;
    Ok((n, v.is_string().then_some(e)))
}

fn pair(d: &Document, v: Option<&Value>, key: &str) -> Result<[(f64, Option<Expr>); 2]> {
    match v {
        Some(Value::Array(a)) if a.len() == 2 => Ok([coord(d, &a[0], key)?, coord(d, &a[1], key)?]),
        _ => Err(bad(format!("`{key}` must be [x, y]"))),
    }
}

/// Builds a sketch from declarative JSON; keeps a fixed origin point for parametric positions.
struct Builder<'a> {
    d: &'a Document,
    s: Sketch,
    origin: Option<Eid>,
}

impl Builder<'_> {
    fn origin(&mut self) -> Result<Eid> {
        if let Some(o) = self.origin {
            return Ok(o);
        }
        let o = self.s.add_point(0.0, 0.0).map_err(|e| bad(e.to_string()))?;
        self.s.add_constraint(Constraint::Fix { point: o }).map_err(|e| bad(e.to_string()))?;
        self.origin = Some(o);
        Ok(o)
    }

    /// A free point at the given position (polyline vertices).
    fn point(&mut self, c: &[(f64, Option<Expr>); 2]) -> Result<Eid> {
        if c.iter().any(|v| v.1.is_some()) {
            return self.anchored(c);
        }
        self.s.add_point(c[0].0, c[1].0).map_err(|e| bad(e.to_string()))
    }

    /// A point that stays where it was given when parameters change: fixed if numeric,
    /// otherwise dimensioned from the sketch origin on both axes.
    fn anchored(&mut self, c: &[(f64, Option<Expr>); 2]) -> Result<Eid> {
        let p = self.s.add_point(c[0].0, c[1].0).map_err(|e| bad(e.to_string()))?;
        if c.iter().all(|v| v.1.is_none()) {
            self.s.add_constraint(Constraint::Fix { point: p }).map_err(|e| bad(e.to_string()))?;
            return Ok(p);
        }
        let o = self.origin()?;
        let ex = c[0].1.clone().unwrap_or_else(|| Expr::num(c[0].0));
        let ey = c[1].1.clone().unwrap_or_else(|| Expr::num(c[1].0));
        self.dim(DimKind::HorizontalDistance { a: o, b: p }, &ex, c[0].0)?;
        self.dim(DimKind::VerticalDistance { a: o, b: p }, &ey, c[1].0)?;
        Ok(p)
    }

    /// Signed H/V distances store the magnitude; the solver keeps the side from the geometry.
    fn dim(&mut self, k: DimKind, e: &Expr, value: f64) -> Result<()> {
        let text = if value < 0.0 && matches!(k, DimKind::HorizontalDistance { .. } | DimKind::VerticalDistance { .. })
        {
            format!("abs({})", e.0)
        } else {
            e.0.clone()
        };
        self.s.add_dimension(k, &text).map_err(|err| bad(err.to_string()))
    }

    fn rect(&mut self, v: &Value) -> Result<()> {
        let size = pair(self.d, v.get("size"), "size")?;
        let centered = v.get("centered").and_then(Value::as_bool).unwrap_or(false);
        let at = match (v.get("corner"), v.get("center")) {
            (Some(c), _) => pair(self.d, Some(c), "corner")?,
            (None, Some(c)) => pair(self.d, Some(c), "center")?,
            _ => [(0.0, None), (0.0, None)],
        };
        let is_center = centered || v.get("center").is_some();
        let (w, h) = (size[0].0, size[1].0);
        if !(w.abs() > 1e-9 && h.abs() > 1e-9) {
            return Err(bad("rectangle size must not be zero"));
        }
        let (x0, y0) = if is_center { (at[0].0 - w / 2.0, at[1].0 - h / 2.0) } else { (at[0].0, at[1].0) };
        let lines = self.s.add_rectangle(x0, y0, x0 + w, y0 + h).map_err(|e| bad(e.to_string()))?;
        let [bottom, right, ..] = lines;
        let corner = match self.s.get(bottom) {
            Some(Entity::Line { a, .. }) => *a,
            _ => return Err(EngineError::Failed("rectangle".into())),
        };
        if let Some(e) = &size[0].1 {
            self.dim(DimKind::Length { line: bottom }, e, w)?;
        }
        if let Some(e) = &size[1].1 {
            self.dim(DimKind::Length { line: right }, e, h)?;
        }
        // Position: pin the corner (or the centre) where it was given, so the rectangle grows
        // away from it when its size parameters change.
        {
            let anchor = if is_center {
                let c = self.anchored(&at)?;
                let diag_end = match self.s.get(right) {
                    Some(Entity::Line { b, .. }) => *b,
                    _ => return Err(EngineError::Failed("rectangle".into())),
                };
                let diag = self.s.add_line(corner, diag_end).map_err(|e| bad(e.to_string()))?;
                if let Some(Entity::Line { construction, .. }) = self.s.get_mut(diag) {
                    *construction = true;
                }
                self.s.add_constraint(Constraint::Midpoint { point: c, line: diag }).map_err(|e| bad(e.to_string()))?;
                c
            } else {
                let p = self.anchored(&at)?;
                self.s.add_constraint(Constraint::Coincident { a: p, b: corner }).map_err(|e| bad(e.to_string()))?;
                p
            };
            let _ = anchor;
        }
        Ok(())
    }

    fn circle(&mut self, v: &Value) -> Result<()> {
        let c = pair(self.d, v.get("center"), "center")?;
        let (r, dim) = match (v.get("diameter"), v.get("radius")) {
            (Some(dv), None) => {
                let (n, e) = coord(self.d, dv, "diameter")?;
                (n / 2.0, e.map(|e| (DimKind::Diameter { curve: Eid(0) }, e)))
            }
            (None, Some(rv)) => {
                let (n, e) = coord(self.d, rv, "radius")?;
                (n, e.map(|e| (DimKind::Radius { curve: Eid(0) }, e)))
            }
            _ => return Err(bad("circle needs `diameter` or `radius`")),
        };
        let center = self.anchored(&c)?;
        let circ = self.s.add_circle(center, r).map_err(|e| bad(e.to_string()))?;
        if let Some((k, e)) = dim {
            let k = match k {
                DimKind::Diameter { .. } => DimKind::Diameter { curve: circ },
                _ => DimKind::Radius { curve: circ },
            };
            self.dim(k, &e, r)?;
        }
        Ok(())
    }

    fn polyline(&mut self, pts: &[Value], closed: bool) -> Result<()> {
        if pts.len() < 2 || pts.len() > MAX_ITEMS {
            return Err(bad("a polyline needs 2..2000 points"));
        }
        let mut ids = Vec::new();
        for p in pts {
            let c = pair(self.d, Some(p), "points")?;
            ids.push(self.point(&c)?);
        }
        let n = ids.len();
        let segs = if closed { n } else { n - 1 };
        for i in 0..segs {
            self.s.add_line(ids[i], ids[(i + 1) % n]).map_err(|e| bad(e.to_string()))?;
        }
        Ok(())
    }

    fn arc(&mut self, v: &Value) -> Result<()> {
        let c = pair(self.d, v.get("center"), "center")?;
        let a = pair(self.d, v.get("start"), "start")?;
        let b = pair(self.d, v.get("end"), "end")?;
        let (c, a, b) = (self.anchored(&c)?, self.point(&a)?, self.point(&b)?);
        self.s.add_arc(c, a, b).map_err(|e| bad(e.to_string()))?;
        Ok(())
    }
}

fn list<'a>(p: &'a Value, key: &str) -> Result<&'a [Value]> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(&[]),
        Some(Value::Array(a)) if a.len() <= MAX_ITEMS => Ok(a),
        _ => Err(bad(format!("`{key}` must be an array (≤ {MAX_ITEMS})"))),
    }
}

pub(crate) fn plane_param(d: &Document, p: &Value) -> Result<SketchPlane> {
    let offset = params::opt_expr(p, "offset")?.unwrap_or(Expr::num(0.0));
    match p.get("plane") {
        None | Some(Value::Null) => Ok(SketchPlane::Origin { name: "XY".into(), offset }),
        Some(Value::String(n)) if ["XY", "XZ", "YZ"].contains(&n.to_ascii_uppercase().as_str()) => {
            Ok(SketchPlane::Origin { name: n.to_ascii_uppercase(), offset })
        }
        Some(v @ Value::Object(_)) => {
            let body = params::body(d, v, "body")?;
            let face: FaceSel = serde_json::from_value(v.get("face").cloned().unwrap_or(json!("top")))
                .map_err(|e| bad(format!("`plane.face`: {e}")))?;
            Ok(SketchPlane::Face { body, face, offset })
        }
        _ => Err(bad("`plane` must be XY, XZ, YZ or {body, face}")),
    }
}

fn sketch_create(s: &mut Session, p: &Value) -> Result<Value> {
    let d = s.document();
    let plane = plane_param(d, p)?;
    let mut b = Builder { d, s: Sketch::new(), origin: None };
    for v in list(p, "rectangles")? {
        b.rect(v)?;
    }
    for v in list(p, "circles")? {
        b.circle(v)?;
    }
    for v in list(p, "lines")? {
        b.polyline(&[v.get("a").cloned().unwrap_or(Value::Null), v.get("b").cloned().unwrap_or(Value::Null)], false)?;
    }
    for v in list(p, "polylines")? {
        let pts = v.get("points").and_then(Value::as_array).ok_or_else(|| bad("polyline needs `points`"))?;
        b.polyline(pts, v.get("closed").and_then(Value::as_bool).unwrap_or(true))?;
    }
    for v in list(p, "arcs")? {
        b.arc(v)?;
    }
    let sketch = b.s;
    let mut out = add_feature(s, FeatureKind::Sketch { plane, sketch })?;
    if let Some(id) = out.get("feature").and_then(Value::as_u64)
        && let Some(r) = s.model().sketch(FeatureId(id as u32))
    {
        out["profiles"] = json!(r.regions.len());
        out["dof"] = json!(r.dof);
        out["regions"] = r
            .regions
            .iter()
            .enumerate()
            .map(|(i, g)| json!({"index": i, "area": (g.area * 1e3).round() / 1e3, "depth": g.depth, "sample": [g.sample.x, g.sample.y]}))
            .collect();
    }
    Ok(out)
}

fn sketch_ref(d: &Document, p: &Value) -> Result<FeatureId> {
    match p.get("sketch") {
        None | Some(Value::Null) => d
            .active_features()
            .iter()
            .rev()
            .find(|f| matches!(f.kind, FeatureKind::Sketch { .. }))
            .map(|f| f.id)
            .ok_or_else(|| bad("no sketch: create one with sketch.create")),
        Some(_) => {
            let id = params::feature(d, p, "sketch")?;
            match d.feature(id).map(|f| &f.kind) {
                Some(FeatureKind::Sketch { .. }) => Ok(id),
                _ => Err(bad("`sketch` is not a sketch feature")),
            }
        }
    }
}

fn profiles(p: &Value) -> Result<ProfileSel> {
    match p.get("profiles") {
        None | Some(Value::Null) => Ok(ProfileSel::All),
        Some(Value::String(s)) if s == "all" => Ok(ProfileSel::All),
        Some(Value::Array(a)) if a.iter().all(Value::is_u64) => {
            Ok(ProfileSel::Indices { indices: a.iter().filter_map(|v| v.as_u64().map(|i| i as usize)).collect() })
        }
        Some(Value::Array(a)) => {
            let pts: Vec<[f64; 2]> =
                serde_json::from_value(Value::Array(a.clone())).map_err(|e| bad(format!("`profiles`: {e}")))?;
            Ok(ProfileSel::Points { points: pts })
        }
        _ => Err(bad("`profiles` must be \"all\", [indices] or [[x, y], …]")),
    }
}

fn solid_extrude(s: &mut Session, p: &Value) -> Result<Value> {
    let d = s.document();
    let sketch = sketch_ref(d, p)?;
    let through_all = params::opt_bool(p, "through_all")?.unwrap_or(false);
    let distance = if through_all {
        params::opt_expr(p, "distance")?.unwrap_or(Expr::num(1.0))
    } else {
        params::expr(p, "distance")?
    };
    let direction = match params::opt_str(p, "direction").unwrap_or("one_side") {
        "one_side" => ExtrudeDirection::OneSide,
        "symmetric" => ExtrudeDirection::Symmetric,
        o => return Err(bad(format!("unknown direction `{o}` (one_side, symmetric)"))),
    };
    let kind = FeatureKind::Extrude {
        sketch,
        profiles: profiles(p)?,
        distance,
        direction,
        through_all,
        op: params::body_op(d, p)?,
    };
    add_feature(s, kind)
}

fn solid_revolve(s: &mut Session, p: &Value) -> Result<Value> {
    let d = s.document();
    let sketch = sketch_ref(d, p)?;
    let axis = match p.get("axis") {
        Some(Value::String(n)) => RevolveAxis::Name(n.clone()),
        Some(Value::Number(n)) => RevolveAxis::Line {
            line: Eid(n.as_u64().and_then(|v| u32::try_from(v).ok()).ok_or_else(|| bad("bad axis line id"))?),
        },
        _ => return Err(bad("`axis` must be X, Y, Z, sketch_x, sketch_y or a sketch line id")),
    };
    let angle = params::opt_expr(p, "angle")?.unwrap_or(Expr::num(360.0));
    add_feature(s, FeatureKind::Revolve { sketch, profiles: profiles(p)?, axis, angle, op: params::body_op(d, p)? })
}

fn edges_param(p: &Value) -> Result<EdgeSel> {
    serde_json::from_value(p.get("edges").cloned().unwrap_or(json!("all"))).map_err(|e| bad(format!("`edges`: {e}")))
}

fn faces_param(p: &Value, key: &str, default: Value) -> Result<FaceSel> {
    serde_json::from_value(p.get(key).cloned().unwrap_or(default)).map_err(|e| bad(format!("`{key}`: {e}")))
}

/// The body to modify: `body`, else the only/last body.
fn target_body(s: &Session, p: &Value) -> Result<BodyId> {
    match p.get("body") {
        Some(_) => params::body(s.document(), p, "body"),
        None => s.model().bodies.last().map(|b| b.id).ok_or_else(|| bad("there is no body yet")),
    }
}

fn solid_fillet(s: &mut Session, p: &Value) -> Result<Value> {
    let body = target_body(s, p)?;
    add_feature(s, FeatureKind::Fillet { body, edges: edges_param(p)?, radius: params::expr(p, "radius")? })
}

fn solid_chamfer(s: &mut Session, p: &Value) -> Result<Value> {
    let body = target_body(s, p)?;
    add_feature(s, FeatureKind::Chamfer { body, edges: edges_param(p)?, distance: params::expr(p, "distance")? })
}

fn solid_shell(s: &mut Session, p: &Value) -> Result<Value> {
    let body = target_body(s, p)?;
    add_feature(
        s,
        FeatureKind::Shell {
            body,
            faces: faces_param(p, "faces", json!("top"))?,
            thickness: params::expr(p, "thickness")?,
        },
    )
}

fn solid_hole(s: &mut Session, p: &Value) -> Result<Value> {
    let body = target_body(s, p)?;
    let pts = match p.get("points").or(p.get("positions")) {
        Some(Value::Array(a)) if !a.is_empty() && a.len() <= 1000 => a
            .iter()
            .map(|v| {
                let arr = match v {
                    Value::Array(x) if x.len() == 2 => json!({"p": [x[0], x[1], 0]}),
                    other => json!({"p": other}),
                };
                params::vec3(&arr, "p", None)
            })
            .collect::<Result<Vec<_>>>()?,
        _ => return Err(bad("`points` must be a non-empty array of [x, y, z] (or [x, y])")),
    };
    let pair_of = |key: &str, a: &str, b: &str| -> Result<Option<(Expr, Expr)>> {
        match p.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => Ok(Some((params::expr(v, a)?, params::expr(v, b)?))),
        }
    };
    add_feature(
        s,
        FeatureKind::Hole {
            body,
            face: faces_param(p, "face", json!("top"))?,
            points: pts,
            diameter: params::expr(p, "diameter")?,
            depth: params::opt_expr(p, "depth")?,
            counterbore: pair_of("counterbore", "diameter", "depth")?,
            countersink: pair_of("countersink", "diameter", "angle")?,
        },
    )
}

fn solid_press_pull(s: &mut Session, p: &Value) -> Result<Value> {
    let body = target_body(s, p)?;
    add_feature(
        s,
        FeatureKind::PressPull {
            body,
            face: faces_param(p, "face", json!("top"))?,
            distance: params::expr(p, "distance")?,
        },
    )
}

fn h(f: fn(&mut Session, &Value) -> Result<Value>) -> Handler {
    Arc::new(f)
}

fn face_schema(desc: &str) -> Value {
    json!({"description": format!("{desc} A selector: \"top\", \"bottom\", \"+X\", \"-X\", \"+Y\", \"-Y\", \"largest_planar\", \"cylindrical\", {{\"near\": [x, y, z]}}, or an array of these.")})
}

fn edge_schema() -> Value {
    json!({"description": "Edges: \"all\", \"vertical\", \"horizontal\", \"circular\", \"linear\", \"top\", \"bottom\", \"top_outer\", \"bottom_outer\", {\"edges_of\": <face selector>}, {\"near\": [x, y, z]}, or an array of these.", "default": "all"})
}

pub(crate) fn register(r: &mut Registry) {
    let op = |mut props: Value| {
        if let Some(o) = props.as_object_mut() {
            o.insert("operation".into(), schema::operation());
            o.insert("target".into(), schema::body("Body to join/cut/intersect with (id or name)."));
        }
        props
    };
    let xy = json!({"type": "array", "items": {"type": ["number", "string"]}, "minItems": 2, "maxItems": 2});
    let cmds: Vec<(CommandSpec, Handler)> = vec![
        (
            CommandSpec::new(
                "sketch.create",
                "Create Sketch",
                "Create a sketch on a plane from shapes in sketch coordinates (mm). Numbers are drawn as given; expression strings (\"L\", \"W/2\") become driving dimensions, so the sketch follows parameters.",
            )
            .params(json!({"type": "object", "properties": {
                "plane": {"description": "\"XY\" (default), \"XZ\", \"YZ\", or {\"body\": id|name, \"face\": selector} to sketch on a planar face."},
                "offset": schema::num("Offset of the plane along its normal."),
                "rectangles": {"type": "array", "items": {"type": "object", "properties": {"corner": xy, "center": xy, "size": xy}, "required": ["size"]},
                    "description": "Axis-aligned rectangles by `corner` (lower-left) or `center`, and `size` [w, h]."},
                "circles": {"type": "array", "items": {"type": "object", "properties": {"center": xy, "diameter": schema::num(""), "radius": schema::num("")}, "required": ["center"]}},
                "lines": {"type": "array", "items": {"type": "object", "properties": {"a": xy, "b": xy}}},
                "polylines": {"type": "array", "items": {"type": "object", "properties": {"points": {"type": "array", "items": xy}, "closed": {"type": "boolean", "default": true}}}},
                "arcs": {"type": "array", "items": {"type": "object", "properties": {"center": xy, "start": xy, "end": xy}},
                    "description": "Counter-clockwise arcs from start to end."},
            }}))
            .place("Solid", "Create", true)
            .icon("sketch")
            .ai()
            .undoable(),
            h(sketch_create),
        ),
        (
            CommandSpec::new("solid.extrude", "Extrude", "Extrude closed profiles of a sketch (default: the latest sketch, outer profiles with their holes).")
                .params(json!({"type": "object", "required": ["distance"], "properties": op(json!({
                    "sketch": {"type": ["integer", "string"], "description": "Sketch feature id or name (default: latest)."},
                    "distance": schema::num("Distance along the sketch normal; negative goes the other way."),
                    "direction": {"type": "string", "enum": ["one_side", "symmetric"], "default": "one_side"},
                    "through_all": {"type": "boolean", "default": false, "description": "Cut/join through the whole model (needs a target)."},
                    "profiles": {"description": "\"all\" (default), [region indices] or [[x, y] points inside the wanted regions]."},
                }))}))
                .place("Solid", "Create", true)
                .shortcut("E")
                .icon("extrude")
                .ai()
                .undoable(),
            h(solid_extrude),
        ),
        (
            CommandSpec::new("solid.revolve", "Revolve", "Revolve sketch profiles around an axis.")
                .params(json!({"type": "object", "required": ["axis"], "properties": op(json!({
                    "sketch": {"type": ["integer", "string"]},
                    "axis": {"type": ["string", "integer"], "description": "\"X\", \"Y\", \"Z\", \"sketch_x\", \"sketch_y\" or a sketch line id."},
                    "angle": schema::num("Degrees, default 360."),
                    "profiles": {"description": "\"all\", [indices] or [[x, y]]."},
                }))}))
                .place("Solid", "Create", true)
                .icon("revolve")
                .ai()
                .undoable(),
            h(solid_revolve),
        ),
        (
            CommandSpec::new("solid.fillet", "Fillet", "Round edges of a body with a radius.")
                .params(json!({"type": "object", "required": ["radius"], "properties": {
                    "body": schema::body("Body (default: the last one)."), "edges": edge_schema(), "radius": schema::num("Radius.")}}))
                .place("Solid", "Modify", true)
                .shortcut("F")
                .icon("fillet")
                .ai()
                .undoable(),
            h(solid_fillet),
        ),
        (
            CommandSpec::new("solid.chamfer", "Chamfer", "Bevel edges of a body by a distance.")
                .params(json!({"type": "object", "required": ["distance"], "properties": {
                    "body": schema::body("Body (default: the last one)."), "edges": edge_schema(), "distance": schema::num("Distance.")}}))
                .place("Solid", "Modify", true)
                .icon("chamfer")
                .ai()
                .undoable(),
            h(solid_chamfer),
        ),
        (
            CommandSpec::new("solid.shell", "Shell", "Hollow a body: remove the given faces (default: top) and keep walls of `thickness`.")
                .params(json!({"type": "object", "required": ["thickness"], "properties": {
                    "body": schema::body("Body (default: the last one)."), "faces": face_schema("Faces to open (empty array = closed hollow)."), "thickness": schema::num("Wall thickness (inward).")}}))
                .place("Solid", "Modify", false)
                .icon("shell")
                .ai()
                .undoable(),
            h(solid_shell),
        ),
        (
            CommandSpec::new("solid.hole", "Hole", "Drill holes into a planar face at points (through all unless `depth`), optionally counterbored or countersunk. M3 clearance 3.4, M4 4.5, M5 5.5, M6 6.6, M8 9.")
                .params(json!({"type": "object", "required": ["points", "diameter"], "properties": {
                    "body": schema::body("Body (default: the last one)."),
                    "face": face_schema("Face to drill into (default \"top\")."),
                    "points": {"type": "array", "items": {"type": "array", "items": {"type": ["number", "string"]}}, "description": "Hole centres [x, y, z] (projected onto the face) or [x, y]."},
                    "diameter": schema::num("Hole diameter."),
                    "depth": schema::num("Depth (omit for through all)."),
                    "counterbore": {"type": "object", "properties": {"diameter": schema::num(""), "depth": schema::num("")}},
                    "countersink": {"type": "object", "properties": {"diameter": schema::num(""), "angle": schema::num("Included angle, e.g. 90.")}},
                }}))
                .place("Solid", "Create", true)
                .shortcut("H")
                .icon("hole")
                .ai()
                .undoable(),
            h(solid_hole),
        ),
        (
            CommandSpec::new("solid.press_pull", "Press Pull", "Push (negative) or pull (positive) a planar face along its normal.")
                .params(json!({"type": "object", "required": ["distance"], "properties": {
                    "body": schema::body("Body (default: the last one)."), "face": face_schema("Face (default \"top\")."), "distance": schema::num("Distance.")}}))
                .place("Solid", "Modify", true)
                .shortcut("Q")
                .icon("press_pull")
                .ai()
                .undoable(),
            h(solid_press_pull),
        ),
    ];
    for (spec, handler) in cmds {
        if let Err(e) = r.register(spec, handler) {
            log::error!("command failed to register: {e}");
        }
    }
}
