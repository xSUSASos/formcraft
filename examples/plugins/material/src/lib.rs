//! «Масса по таблице материалов» — FormCraft example plugin (Plugin API v1): a command add-in
//! without custom features. Build: `cargo build --release --target wasm32-wasip2`.

wit_bindgen::generate!({
    path: "../../../crates/plugin-api/wit",
    world: "plugin",
});

use formcraft::plugin::host::{inspect, log};
use formcraft::plugin::types::Placement;
use serde_json::{Value, json};

struct Material;

const SCHEMA: &str = r#"{"type": "object", "required": ["material"], "properties": {
  "material": {"type": "string",
    "enum": ["steel", "stainless", "aluminium", "brass", "copper", "titanium", "abs", "pla", "petg", "nylon", "wood_oak"],
    "description": "Material from the density table."},
  "density": {"type": "number", "description": "Density in g/cm3 overriding the table (optional)."}
}}"#;

/// Densities, g/cm³.
const TABLE: [(&str, f64); 11] = [
    ("steel", 7.85),
    ("stainless", 8.0),
    ("aluminium", 2.70),
    ("brass", 8.50),
    ("copper", 8.96),
    ("titanium", 4.43),
    ("abs", 1.04),
    ("pla", 1.24),
    ("petg", 1.27),
    ("nylon", 1.14),
    ("wood_oak", 0.75),
];

fn mass(params: &str) -> Result<String, String> {
    let p: Value = serde_json::from_str(params).map_err(|e| format!("params: {e}"))?;
    let name = p.get("material").and_then(Value::as_str).ok_or("missing `material`")?;
    let table = TABLE.iter().find(|(n, _)| *n == name).map(|(_, d)| *d);
    let density = match p.get("density") {
        None | Some(Value::Null) => table.ok_or(format!("unknown material `{name}`"))?,
        Some(v) => {
            let d = v.as_f64().filter(|d| d.is_finite() && *d > 0.0);
            d.ok_or("`density` must be a positive number (g/cm3)")?
        }
    };
    if table.is_none() {
        return Err(format!("unknown material `{name}`"));
    }

    let model: Value = serde_json::from_str(&inspect()).map_err(|e| format!("model.inspect: {e}"))?;
    let empty = Vec::new();
    let list = model.get("bodies").and_then(Value::as_array).unwrap_or(&empty);
    let mut bodies = Vec::new();
    let mut total = 0.0;
    for b in list {
        if b.get("visible").and_then(Value::as_bool) == Some(false) {
            continue;
        }
        let volume = b.get("volume").and_then(Value::as_f64).filter(|v| v.is_finite() && *v >= 0.0);
        let Some(volume) = volume else { continue };
        let grams = volume / 1000.0 * density; // mm³ → cm³
        total += grams;
        let bname = b.get("name").and_then(Value::as_str).unwrap_or("?");
        bodies.push(json!({"name": bname, "volume_mm3": volume, "mass_g": grams}));
    }
    let message = if bodies.is_empty() {
        "No visible bodies".to_string()
    } else {
        format!("{name}, {density} g/cm3: total {total:.2} g ({:.4} kg), bodies: {}", total / 1000.0, bodies.len())
    };
    log(2, &message);
    Ok(json!({
        "message": message,
        "material": name,
        "density_g_cm3": density,
        "bodies": bodies,
        "total_g": total,
        "total_kg": total / 1000.0,
    })
    .to_string())
}

impl Guest for Material {
    fn init() -> Result<(Vec<CommandSpec>, Vec<FeatureSpec>), String> {
        Ok((
            vec![CommandSpec {
                id: "material.mass".into(),
                label: "Mass by material".into(),
                description: "Mass of the visible bodies from a material density table (steel, stainless, aluminium, brass, copper, titanium, abs, pla, petg, nylon, wood_oak); optional density in g/cm3 overrides the table."
                    .into(),
                params_schema: SCHEMA.into(),
                placement: Some(Placement { tab: "Inspect".into(), group: "Inspect".into(), pinned: true }),
                shortcut: None,
                ai: true,
            }],
            Vec::new(),
        ))
    }

    fn run_command(id: String, params: String) -> Result<String, String> {
        match id.as_str() {
            "material.mass" => mass(&params),
            other => Err(format!("material has no command `{other}`")),
        }
    }

    fn regenerate(kind: String, _params: String) -> Result<Solid, String> {
        Err(format!("material has no feature `{kind}`"))
    }
}

export!(Material);
