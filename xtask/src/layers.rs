//! Crate dependency layering. A crate may depend only on workspace crates of a strictly lower
//! layer; nothing below the UI layer may depend on UI toolkits.

use serde_json::Value;

/// Layer of each workspace crate (without the `formcraft-` prefix). Apps sit on top.
const LAYERS: &[(&str, u8)] = &[
    ("kernel", 0),
    ("plugin-api", 0),
    ("occt", 1),
    ("sketch", 1),
    ("doc", 2),
    ("regen", 3),
    ("tess", 3),
    ("io", 4),
    ("engine", 5),
    ("ai", 6),
    ("plugin", 6),
    ("render", 6),
    ("ui-egui", 7),
    ("mcp", 7),
];
const APP_LAYER: u8 = 8;
const UI_LAYER: u8 = 7;
/// Crates that only the UI layer and apps may use.
const UI_ONLY: &[&str] = &["egui", "eframe", "egui-wgpu", "egui-winit", "winit", "rfd", "egui_kittest"];

fn layer_of(name: &str) -> Option<u8> {
    if matches!(name, "formcraft" | "formcraft-cli") {
        return Some(APP_LAYER);
    }
    let short = name.strip_prefix("formcraft-")?;
    LAYERS.iter().find(|(n, _)| *n == short).map(|(_, l)| *l)
}

pub fn check(meta: &Value) -> Result<Vec<String>, String> {
    let packages = meta["packages"].as_array().ok_or("cargo metadata: no packages")?;
    let mut violations = Vec::new();
    for p in packages {
        let name = p["name"].as_str().unwrap_or_default();
        if name == "xtask" {
            continue;
        }
        let Some(layer) = layer_of(name) else {
            violations.push(format!("{name}: not assigned to a layer (xtask/src/layers.rs)"));
            continue;
        };
        for d in p["dependencies"].as_array().into_iter().flatten() {
            let dep = d["name"].as_str().unwrap_or_default();
            if d["kind"].as_str() == Some("dev") {
                continue;
            }
            if let Some(dl) = layer_of(dep)
                && dl >= layer
            {
                violations.push(format!("{name} (L{layer}) depends on {dep} (L{dl})"));
            }
            if layer < UI_LAYER && UI_ONLY.contains(&dep) {
                violations.push(format!("{name} (L{layer}) depends on UI crate {dep}"));
            }
        }
    }
    Ok(violations)
}
