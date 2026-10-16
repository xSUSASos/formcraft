//! «Шестерня» — FormCraft example plugin (Plugin API v1): an involute spur gear as a custom
//! feature. Build: `cargo build --release --target wasm32-wasip2` (or `cargo xtask plugins`).

wit_bindgen::generate!({
    path: "../../../crates/plugin-api/wit",
    world: "plugin",
});

use std::f64::consts::PI;

use formcraft::plugin::geometry::extrude;
use formcraft::plugin::host::log;
use formcraft::plugin::types::{Placement, Plane, Point2, Vec3};
use serde_json::{Value, json};

struct Gear;

const SCHEMA: &str = r#"{"type": "object", "required": ["teeth", "module"], "properties": {
  "teeth": {"type": ["number", "string"], "description": "Number of teeth z (6..200)."},
  "module": {"type": ["number", "string"], "description": "Module m, mm (pitch diameter = m·z)."},
  "thickness": {"type": ["number", "string"], "description": "Face width, mm (default 10)."},
  "bore": {"type": ["number", "string"], "description": "Bore diameter, mm (0 = none)."},
  "pressure_angle": {"type": ["number", "string"], "description": "Pressure angle, degrees (default 20)."},
  "center": {"type": "array", "items": {"type": ["number", "string"]}, "minItems": 3, "maxItems": 3,
             "description": "Centre of the bottom face [x, y, z] (default origin)."}
}}"#;

fn num(p: &Value, key: &str, default: Option<f64>) -> Result<f64, String> {
    match p.get(key) {
        Some(v) => v.as_f64().filter(|x| x.is_finite()).ok_or(format!("`{key}` must be a number")),
        None => default.ok_or(format!("missing `{key}`")),
    }
}

/// Involute function inv(a) = tan a − a.
fn inv(a: f64) -> f64 {
    a.tan() - a
}

/// The gear outline as one polygon (counter-clockwise).
pub fn outline(z: usize, m: f64, alpha_deg: f64) -> Vec<(f64, f64)> {
    let alpha = alpha_deg.to_radians();
    let r = m * z as f64 / 2.0; // pitch radius
    let rb = r * alpha.cos(); // base circle
    let ra = r + m; // addendum
    let rf = (r - 1.25 * m).max(m * 0.5); // dedendum (kept positive for tiny gears)
    // half tooth angle at radius rho (tooth thickness at the pitch circle = π m / 2)
    let half = |rho: f64| {
        let rho = rho.max(rb);
        PI / (2.0 * z as f64) + inv(alpha) - inv((rb / rho).clamp(-1.0, 1.0).acos())
    };
    let r0 = rb.max(rf);
    let steps = 8;
    let at = |rho: f64, a: f64| (rho * a.cos(), rho * a.sin());
    let mut pts = Vec::new();
    for k in 0..z {
        let c = 2.0 * PI * k as f64 / z as f64;
        if rf < rb {
            pts.push(at(rf, c - half(rb)));
        }
        for i in 0..=steps {
            let rho = r0 + (ra - r0) * i as f64 / steps as f64;
            pts.push(at(rho, c - half(rho)));
        }
        for i in 1..4 {
            let a = c - half(ra) + 2.0 * half(ra) * i as f64 / 4.0;
            pts.push(at(ra, a));
        }
        for i in (0..=steps).rev() {
            let rho = r0 + (ra - r0) * i as f64 / steps as f64;
            pts.push(at(rho, c + half(rho)));
        }
        if rf < rb {
            pts.push(at(rf, c + half(rb)));
        }
        // root arc to the next tooth
        let next = 2.0 * PI * (k + 1) as f64 / z as f64;
        let (a0, a1) = (c + half(rb), next - half(rb));
        for i in 1..4 {
            pts.push(at(rf, a0 + (a1 - a0) * i as f64 / 4.0));
        }
    }
    pts
}

fn circle(r: f64, n: usize) -> Vec<(f64, f64)> {
    (0..n).map(|i| 2.0 * PI * i as f64 / n as f64).map(|a| (r * a.cos(), r * a.sin())).collect()
}

fn polygon(p: Vec<(f64, f64)>) -> Vec<Point2> {
    p.into_iter().map(|(x, y)| Point2 { x, y }).collect()
}

fn spur(p: &Value) -> Result<Solid, String> {
    let z = num(p, "teeth", None)?;
    if !(6.0..=200.0).contains(&z) || z.fract() != 0.0 {
        return Err("`teeth` must be a whole number 6..200".into());
    }
    let m = num(p, "module", None)?;
    if !(0.1..=50.0).contains(&m) {
        return Err("`module` must be 0.1..50 mm".into());
    }
    let t = num(p, "thickness", Some(10.0))?;
    if !(0.01..=10_000.0).contains(&t) {
        return Err("`thickness` must be positive".into());
    }
    let alpha = num(p, "pressure_angle", Some(20.0))?;
    if !(10.0..=35.0).contains(&alpha) {
        return Err("`pressure_angle` must be 10..35°".into());
    }
    let bore = num(p, "bore", Some(0.0))?;
    let rf = m * z / 2.0 - 1.25 * m;
    if bore < 0.0 || bore >= 2.0 * rf * 0.9 {
        return Err(format!("`bore` must be 0..{:.2} mm for this gear", 2.0 * rf * 0.9));
    }
    let c = match p.get("center").and_then(Value::as_array) {
        Some(a) if a.len() == 3 => {
            let v: Vec<f64> = a.iter().filter_map(Value::as_f64).collect();
            if v.len() != 3 {
                return Err("`center` must be three numbers".into());
            }
            Vec3 { x: v[0], y: v[1], z: v[2] }
        }
        _ => Vec3 { x: 0.0, y: 0.0, z: 0.0 },
    };
    let plane = Plane { origin: c, x_dir: Vec3 { x: 1.0, y: 0.0, z: 0.0 }, normal: Vec3 { x: 0.0, y: 0.0, z: 1.0 } };
    let holes = if bore > 0.0 {
        // holes run clockwise
        let mut h = circle(bore / 2.0, 48);
        h.reverse();
        vec![polygon(h)]
    } else {
        Vec::new()
    };
    log(2, &format!("spur gear z={z} m={m} t={t}"));
    extrude(plane, &polygon(outline(z as usize, m, alpha)), &holes, t)
}

impl Guest for Gear {
    fn init() -> Result<(Vec<CommandSpec>, Vec<FeatureSpec>), String> {
        Ok((
            Vec::new(),
            vec![FeatureSpec {
                kind: "spur".into(),
                label: "Spur Gear".into(),
                description: "Involute spur gear: teeth z, module m (pitch diameter m·z), face width, bore. Example: teeth 20, module 2, thickness 10, bore 8."
                    .into(),
                params_schema: SCHEMA.into(),
                placement: Some(Placement { tab: "Solid".into(), group: "Create".into(), pinned: true }),
                ai: true,
            }],
        ))
    }

    fn run_command(id: String, _params: String) -> Result<String, String> {
        Err(format!("gear has no command `{id}`"))
    }

    fn regenerate(kind: String, params: String) -> Result<Solid, String> {
        let p: Value = serde_json::from_str(&params).map_err(|e| format!("params: {e}"))?;
        match kind.as_str() {
            "spur" => spur(&p),
            other => Err(format!("unknown feature `{other}`")),
        }
    }
}

export!(Gear);

#[allow(dead_code)]
fn _schema_is_json() -> Value {
    serde_json::from_str(SCHEMA).unwrap_or(json!({}))
}
