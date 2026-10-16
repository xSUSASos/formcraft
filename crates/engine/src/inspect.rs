//! Inspect (PLAN Phase 7): measure, section analysis, body properties with mass by density.
//!
//! Entities are given as `{"body": id|name, "face": <selector>}`, `{"body", "edge": <selector>}`,
//! `{"body": id|name}` or a point `[x, y, z]`; without them the current selection is used.

use std::sync::Arc;

use formcraft_doc::BodyId;
use formcraft_doc::select::{EdgeSel, FaceSel};
use formcraft_kernel::{Aabb, BoolOp, CurveKind, DVec3, EdgeInfo, FaceInfo, SurfaceKind};
use serde_json::{Value, json};

use crate::registry::{CommandSpec, Handler, Registry};
use crate::{EngineError, Pick, Result, Session, params};

/// Densities in g/cm³ (common engineering and 3D-printing materials).
pub const MATERIALS: &[(&str, f64)] = &[
    ("steel", 7.85),
    ("stainless", 8.0),
    ("aluminium", 2.70),
    ("brass", 8.50),
    ("copper", 8.96),
    ("titanium", 4.43),
    ("cast_iron", 7.20),
    ("abs", 1.04),
    ("pla", 1.24),
    ("petg", 1.27),
    ("nylon", 1.14),
    ("wood_oak", 0.75),
];

fn bad(m: impl Into<String>) -> EngineError {
    EngineError::InvalidParams(m.into())
}

#[derive(Debug, Clone)]
enum Entity {
    Point(DVec3),
    Face(BodyId, FaceInfo),
    Edge(BodyId, EdgeInfo),
    Body(BodyId),
}

fn faces(s: &Session, b: BodyId) -> Result<Vec<FaceInfo>> {
    let body = s.model().body(b).ok_or_else(|| bad(format!("no body {}", b.0)))?;
    s.kernel().faces(&body.shape).map_err(Into::into)
}

fn edges(s: &Session, b: BodyId) -> Result<Vec<EdgeInfo>> {
    let body = s.model().body(b).ok_or_else(|| bad(format!("no body {}", b.0)))?;
    s.kernel().edges(&body.shape).map_err(Into::into)
}

fn entity(s: &Session, v: &Value) -> Result<Entity> {
    if let Some(a) = v.as_array() {
        let p: Vec<f64> = a.iter().filter_map(Value::as_f64).collect();
        return match p.as_slice() {
            [x, y, z] if p.iter().all(|c| c.is_finite()) => Ok(Entity::Point(DVec3::new(*x, *y, *z))),
            _ => Err(bad("a point is [x, y, z]")),
        };
    }
    let body = params::body(s.document(), v, "body")?;
    let model_body = s.model().body(body).ok_or_else(|| bad(format!("body {} has no geometry", body.0)))?;
    let k = s.kernel().as_ref();
    if let Some(f) = v.get("face") {
        let sel: FaceSel = serde_json::from_value(f.clone()).map_err(|e| bad(format!("`face`: {e}")))?;
        let (idx, all) = formcraft_regen::select::resolve_faces(k, &model_body.shape, &sel).map_err(bad)?;
        let first = idx.first().copied().ok_or_else(|| bad("no face matches"))?;
        return all
            .into_iter()
            .find(|f| f.index == first)
            .map(|f| Entity::Face(body, f))
            .ok_or_else(|| bad("face vanished"));
    }
    if let Some(e) = v.get("edge") {
        let sel: EdgeSel = serde_json::from_value(e.clone()).map_err(|e| bad(format!("`edge`: {e}")))?;
        let idx = formcraft_regen::select::resolve_edges(k, &model_body.shape, &sel).map_err(bad)?;
        let first = idx.first().copied().ok_or_else(|| bad("no edge matches"))?;
        return edges(s, body)?
            .into_iter()
            .find(|x| x.index == first)
            .map(|x| Entity::Edge(body, x))
            .ok_or_else(|| bad("edge vanished"));
    }
    Ok(Entity::Body(body))
}

fn from_pick(s: &Session, p: &Pick) -> Result<Entity> {
    Ok(match *p {
        Pick::Body { body } => Entity::Body(body),
        Pick::Face { body, index } => Entity::Face(
            body,
            faces(s, body)?.into_iter().find(|f| f.index == index).ok_or_else(|| bad("face vanished"))?,
        ),
        Pick::Edge { body, index } => Entity::Edge(
            body,
            edges(s, body)?.into_iter().find(|e| e.index == index).ok_or_else(|| bad("edge vanished"))?,
        ),
    })
}

fn v3(v: DVec3) -> Value {
    let r = |x: f64| (x * 1e6).round() / 1e6;
    json!([r(v.x), r(v.y), r(v.z)])
}

fn describe(s: &Session, e: &Entity) -> Result<Value> {
    Ok(match e {
        Entity::Point(p) => json!({"type": "point", "point": v3(*p)}),
        Entity::Face(b, f) => {
            let mut v =
                json!({"type": "face", "body": b.0, "area": f.area, "center": v3(f.center), "normal": v3(f.normal)});
            match f.kind {
                SurfaceKind::Plane { .. } => v["surface"] = json!("plane"),
                SurfaceKind::Cylinder { radius, axis, .. } => {
                    v["surface"] = json!("cylinder");
                    v["radius"] = json!(radius);
                    v["diameter"] = json!(radius * 2.0);
                    v["axis"] = v3(axis);
                }
                SurfaceKind::Sphere { radius, .. } => {
                    v["surface"] = json!("sphere");
                    v["radius"] = json!(radius);
                }
                SurfaceKind::Cone { .. } => v["surface"] = json!("cone"),
                SurfaceKind::Torus { .. } => v["surface"] = json!("torus"),
                SurfaceKind::Other => v["surface"] = json!("other"),
            }
            v
        }
        Entity::Edge(b, ed) => {
            let mut v =
                json!({"type": "edge", "body": b.0, "length": ed.length, "start": v3(ed.start), "end": v3(ed.end)});
            match ed.kind {
                CurveKind::Line { dir } => {
                    v["curve"] = json!("line");
                    v["direction"] = v3(dir);
                }
                CurveKind::Circle { radius, center, .. } => {
                    v["curve"] = json!("circle");
                    v["radius"] = json!(radius);
                    v["diameter"] = json!(radius * 2.0);
                    v["center"] = v3(center);
                }
                CurveKind::Other => v["curve"] = json!("other"),
            }
            v
        }
        Entity::Body(b) => properties(s, *b, None)?,
    })
}

/// A representative point and an optional direction (plane normal, line direction).
fn anchor(s: &Session, e: &Entity) -> Result<(DVec3, Option<DVec3>, bool)> {
    Ok(match e {
        Entity::Point(p) => (*p, None, false),
        Entity::Face(_, f) => (f.center, Some(f.normal), matches!(f.kind, SurfaceKind::Plane { .. })),
        Entity::Edge(_, ed) => match ed.kind {
            CurveKind::Line { dir } => (ed.mid, Some(dir), false),
            CurveKind::Circle { center, .. } => (center, None, false),
            CurveKind::Other => (ed.mid, None, false),
        },
        Entity::Body(b) => {
            let body = s.model().body(*b).ok_or_else(|| bad("no body"))?;
            (s.kernel().mass_props(&body.shape)?.centroid, None, false)
        }
    })
}

fn measure_two(s: &Session, a: &Entity, b: &Entity) -> Result<Value> {
    let (pa, da, plane_a) = anchor(s, a)?;
    let (pb, db, plane_b) = anchor(s, b)?;
    let mut out = json!({"a": describe(s, a)?, "b": describe(s, b)?});
    let delta = pb - pa;
    out["center_distance"] = json!(delta.length());
    out["delta"] = v3(delta);
    if let (Some(x), Some(y)) = (da, db) {
        let cos = x.normalize_or_zero().dot(y.normalize_or_zero()).clamp(-1.0, 1.0);
        let angle = cos.acos().to_degrees();
        // planes measure the angle between normals; lines the angle between directions (0..90)
        out["angle_deg"] = json!(if plane_a && plane_b { angle } else { angle.min(180.0 - angle) });
        if plane_a && plane_b && cos.abs() > 1.0 - 1e-9 {
            // parallel planes: the exact gap along the normal
            out["distance"] = json!(delta.dot(x.normalize_or_zero()).abs());
            out["parallel"] = json!(true);
        }
    }
    if out.get("distance").is_none() {
        out["distance"] = out["center_distance"].clone();
    }
    Ok(out)
}

/// Volume, area, centroid, bbox and (with a material or density) the mass.
pub fn properties(s: &Session, b: BodyId, density: Option<(String, f64)>) -> Result<Value> {
    let body = s.model().body(b).ok_or_else(|| bad(format!("no body {}", b.0)))?;
    let m = s.kernel().mass_props(&body.shape)?;
    let mut v = json!({
        "type": "body", "body": b.0, "name": body.name, "volume_mm3": m.volume, "area_mm2": m.area,
        "centroid": v3(m.centroid), "bbox_min": v3(m.bbox.min), "bbox_max": v3(m.bbox.max),
        "size": v3(m.bbox.max - m.bbox.min),
    });
    if let Some((name, rho)) = density {
        // mm³ → cm³ is ×1e-3
        let grams = m.volume * 1e-3 * rho;
        v["material"] = json!(name);
        v["density_g_cm3"] = json!(rho);
        v["mass_g"] = json!(grams);
        v["mass_kg"] = json!(grams / 1000.0);
    }
    Ok(v)
}

fn density(p: &Value) -> Result<Option<(String, f64)>> {
    let custom = p
        .get("density")
        .map(|d| {
            d.as_f64()
                .filter(|x| x.is_finite() && *x > 0.0)
                .ok_or_else(|| bad("`density` must be a positive number (g/cm³)"))
        })
        .transpose()?;
    match (p.get("material").and_then(Value::as_str), custom) {
        (Some(m), d) => {
            let table = MATERIALS.iter().find(|(n, _)| *n == m).map(|(_, r)| *r);
            let rho = d.or(table).ok_or_else(|| {
                bad(format!(
                    "unknown material `{m}` ({})",
                    MATERIALS.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")
                ))
            })?;
            Ok(Some((m.to_string(), rho)))
        }
        (None, Some(d)) => Ok(Some(("custom".into(), d))),
        (None, None) => Ok(None),
    }
}

fn inspect_properties(s: &mut Session, p: &Value) -> Result<Value> {
    let rho = density(p)?;
    let ids: Vec<BodyId> = if p.get("body").is_some() {
        vec![params::body(s.document(), p, "body")?]
    } else {
        let sel = s.selection().bodies();
        if sel.is_empty() { s.model().bodies.iter().filter(|b| b.visible).map(|b| b.id).collect() } else { sel }
    };
    if ids.is_empty() {
        return Err(EngineError::Failed("there is no body".into()));
    }
    let bodies: Vec<Value> = ids.iter().map(|b| properties(s, *b, rho.clone())).collect::<Result<_>>()?;
    let total_v: f64 = bodies.iter().filter_map(|b| b["volume_mm3"].as_f64()).sum();
    let mut out = json!({"bodies": bodies, "volume_mm3": total_v});
    let msg = match &rho {
        Some((name, r)) => {
            let g = total_v * 1e-3 * r;
            out["mass_g"] = json!(g);
            format!("{} body(ies): {total_v:.2} mm³, {g:.1} g of {name}", ids.len())
        }
        None => format!("{} body(ies): {total_v:.2} mm³", ids.len()),
    };
    out["message"] = json!(msg);
    Ok(out)
}

fn inspect_measure(s: &mut Session, p: &Value) -> Result<Value> {
    let mut ents = Vec::new();
    for key in ["a", "b"] {
        if let Some(v) = p.get(key) {
            ents.push(entity(s, v)?);
        }
    }
    if ents.is_empty() {
        let picks = s.selection().items.clone();
        for pk in picks.iter().take(2) {
            ents.push(from_pick(s, pk)?);
        }
    }
    let mut out = match ents.as_slice() {
        [] => return Err(bad("select one or two faces, edges or bodies (or give `a`, `b`)")),
        [one] => describe(s, one)?,
        [a, b, ..] => measure_two(s, a, b)?,
    };
    let msg = if let Some(d) = out.get("distance").and_then(Value::as_f64) {
        let mut m = format!("Distance {d:.4} mm");
        if let Some(a) = out.get("angle_deg").and_then(Value::as_f64) {
            m.push_str(&format!(", angle {a:.3}°"));
        }
        m
    } else if let Some(a) = out.get("area").and_then(Value::as_f64) {
        format!("Area {a:.4} mm²")
    } else if let Some(l) = out.get("length").and_then(Value::as_f64) {
        format!("Length {l:.4} mm")
    } else if let Some(v) = out.get("volume_mm3").and_then(Value::as_f64) {
        format!("Volume {v:.4} mm³")
    } else {
        "Measured".into()
    };
    out["message"] = json!(msg);
    Ok(out)
}

/// The world axis normal to a section plane name.
fn section_axis(name: &str) -> Option<usize> {
    match name.to_ascii_uppercase().as_str() {
        "XY" => Some(2),
        "XZ" => Some(1),
        "YZ" => Some(0),
        _ => None,
    }
}

impl Session {
    /// The visible bodies cut by a plane (`XY`/`XZ`/`YZ` at `offset`), keeping the side below
    /// it — for Section Analysis display. The document is not changed.
    pub fn section_model(&self, plane: &str, offset: f64) -> Result<formcraft_regen::Model> {
        let axis = section_axis(plane).ok_or_else(|| bad(format!("unknown plane `{plane}` (XY, XZ, YZ)")))?;
        if !offset.is_finite() {
            return Err(bad("`offset` must be finite"));
        }
        let k = self.kernel().clone();
        let mut m = self.model().clone();
        let mut all = Aabb::empty();
        for b in &m.bodies {
            if let Ok(mp) = k.mass_props(&b.shape) {
                all = all.union(&mp.bbox);
            }
        }
        if all.is_empty() {
            return Ok(m);
        }
        let pad = (all.max - all.min).length() + 10.0;
        let mut min = all.min - DVec3::splat(pad);
        let mut max = all.max + DVec3::splat(pad);
        max[axis] = offset;
        if max[axis] <= min[axis] {
            min[axis] = offset - 1.0;
        }
        let half = k.make_box(min, max - min)?;
        for b in &mut m.bodies {
            if !b.visible {
                continue;
            }
            match k.boolean(BoolOp::Intersect, &b.shape, &half) {
                Ok(cut) => b.shape = cut,
                Err(e) => log::warn!("section of {}: {e}", b.name),
            }
        }
        Ok(m)
    }
}

fn inspect_section(s: &mut Session, p: &Value) -> Result<Value> {
    let plane = p.get("plane").and_then(Value::as_str).unwrap_or("XY").to_string();
    let offset = match p.get("offset") {
        None => 0.0,
        Some(v) => s.eval_params(v).as_f64().ok_or_else(|| bad("`offset` must be a number or an expression"))?,
    };
    let axis = section_axis(&plane).ok_or_else(|| bad(format!("unknown plane `{plane}` (XY, XZ, YZ)")))?;
    let cut = s.section_model(&plane, offset)?;
    let k = s.kernel().clone();
    let mut bodies = Vec::new();
    let mut total = 0.0;
    for b in cut.bodies.iter().filter(|b| b.visible) {
        let faces = k.faces(&b.shape)?;
        // the section faces lie on the cutting plane
        let area: f64 = faces
            .iter()
            .filter(|f| {
                matches!(f.kind, SurfaceKind::Plane { .. })
                    && (f.center[axis] - offset).abs() < 1e-6
                    && f.normal[axis] > 0.99
            })
            .map(|f| f.area)
            .sum();
        total += area;
        bodies.push(json!({"body": b.id.0, "name": b.name, "section_area_mm2": area}));
    }
    Ok(json!({
        "plane": plane, "offset": offset, "bodies": bodies, "section_area_mm2": total,
        "message": format!("Section {plane} at {offset}: {total:.3} mm²"),
    }))
}

pub(crate) fn register(r: &mut Registry) {
    let names: Vec<&str> = MATERIALS.iter().map(|(n, _)| *n).collect();
    let ent = json!({"description": "{body, face: selector} | {body, edge: selector} | {body} | [x, y, z]; default: the selection"});
    let cmds: Vec<(CommandSpec, Handler)> = vec![
        (
            CommandSpec::new("inspect.properties", "Properties", "Volume, area, centroid, bounding box and mass of bodies (the selection, a `body`, or all visible ones) for a material or density.")
                .params(json!({"type": "object", "properties": {
                    "body": {"type": ["integer", "string"]},
                    "material": {"type": "string", "enum": names},
                    "density": {"type": "number", "description": "g/cm³ (overrides the material table)"}}}))
                .place("Inspect", "Inspect", true)
                .icon("properties"),
            Arc::new(inspect_properties),
        ),
        (
            CommandSpec::new("inspect.measure", "Measure", "Measure one entity (area, length, radius, volume) or two (distance, angle). Parallel planar faces give the exact gap; otherwise centre to centre.")
                .params(json!({"type": "object", "properties": {"a": ent.clone(), "b": ent}}))
                .place("Inspect", "Inspect", true)
                .shortcut("I")
                .icon("measure"),
            Arc::new(inspect_measure),
        ),
        (
            CommandSpec::new("inspect.section", "Section Analysis", "Cut the view with a plane XY/XZ/YZ at an offset and report the section area (the model is not changed).")
                .params(json!({"type": "object", "properties": {
                    "plane": {"type": "string", "enum": ["XY", "XZ", "YZ"], "default": "XY"},
                    "offset": {"type": ["number", "string"], "description": "Position of the plane along its normal, mm."}}}))
                .place("Inspect", "Inspect", true)
                .icon("section"),
            Arc::new(inspect_section),
        ),
    ];
    for (spec, h) in cmds {
        if let Err(e) = r.register(spec, h) {
            log::error!("command failed to register: {e}");
        }
    }
}
