//! `formcraft-cli plugin new <name> --lang rust|python-script`: starter projects (PLAN §3А).

use std::path::Path;

const RUST_LIB: &str = r##"//! {name} — a FormCraft add-in (Plugin API v1).
//! Build: `cargo build --release --target wasm32-wasip2`, then copy
//! `target/wasm32-wasip2/release/{crate_file}.wasm` next to plugin.toml as `{id}.wasm`.

wit_bindgen::generate!({ path: "wit", world: "plugin" });

use formcraft::plugin::geometry::make_box;
use formcraft::plugin::host::{execute, log};
use formcraft::plugin::types::{Placement, Vec3};

struct Addin;

impl Guest for Addin {
    fn init() -> Result<(Vec<CommandSpec>, Vec<FeatureSpec>), String> {
        Ok((
            vec![CommandSpec {
                id: "{id}.hello".into(),
                label: "Hello".into(),
                description: "Adds a 10 mm cube through a built-in command.".into(),
                params_schema: r#"{"type": "object", "properties": {}}"#.into(),
                placement: Some(Placement { tab: "Tools".into(), group: "Add-Ins".into(), pinned: true }),
                shortcut: None,
                ai: false,
            }],
            vec![FeatureSpec {
                kind: "block".into(),
                label: "Block".into(),
                description: "A parametric block: size [x, y, z].".into(),
                params_schema: r#"{"type": "object", "required": ["size"], "properties": {
                    "size": {"type": "array", "items": {"type": ["number", "string"]}, "minItems": 3, "maxItems": 3}}}"#
                    .into(),
                placement: Some(Placement { tab: "Solid".into(), group: "Create".into(), pinned: false }),
                ai: true,
            }],
        ))
    }

    fn run_command(id: String, _params: String) -> Result<String, String> {
        log(2, &format!("running {id}"));
        execute("solid.box", r#"{"size": [10, 10, 10]}"#)
    }

    fn regenerate(_kind: String, params: String) -> Result<Solid, String> {
        // params arrive as JSON with parameter expressions already evaluated
        let nums: Vec<f64> = params
            .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
            .filter_map(|t| t.parse().ok())
            .collect();
        let [x, y, z] = [0, 1, 2].map(|i| nums.get(i).copied().unwrap_or(10.0));
        make_box(Vec3 { x: 0.0, y: 0.0, z: 0.0 }, Vec3 { x, y, z })
    }
}

export!(Addin);
"##;

const CARGO: &str = r#"[package]
name = "{crate}"
version = "0.1.0"
edition = "2024"
publish = false

[lib]
crate-type = ["cdylib"]

[dependencies]
wit-bindgen = "0.63"

[profile.release]
opt-level = "s"
strip = true

[workspace]
"#;

const MANIFEST: &str = r#"id = "{id}"
name = "{name}"
version = "0.1.0"
author = ""
license = "MIT"
description = ""
api = "1.0"
wasm = "{id}.wasm"

[permissions]
files = []
network = false
"#;

const PYTHON: &str = r#""""{name}: a FormCraft script. Run from Tools > Scripts and Add-Ins, or against
`formcraft --control 7878` with FORMCRAFT_PORT=7878."""

import formcraft

with formcraft.connect() as fc:
    fc.parameter("plate_length", 80)
    fc.execute("solid.box", size=["plate_length", 50, 6])
    for b in fc.bodies():
        print(b["name"], round(b["volume"], 2))
"#;

fn fill(t: &str, id: &str, name: &str) -> String {
    let krate = format!("formcraft-addin-{id}");
    t.replace("{id}", id)
        .replace("{name}", name)
        .replace("{crate}", &krate)
        .replace("{crate_file}", &krate.replace('-', "_"))
}

pub fn create(dir: &Path, lang: &str) -> Result<(), String> {
    let name = dir.file_name().and_then(|n| n.to_str()).ok_or("bad name")?.to_string();
    let id: String = name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    if !formcraft_plugin_api::valid_id(&id) {
        return Err(format!("`{name}` does not give a valid add-in id (start with a letter)"));
    }
    if dir.exists() {
        return Err(format!("{} already exists", dir.display()));
    }
    let write = |rel: &str, text: &str| -> Result<(), String> {
        let p = dir.join(rel);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(&p, text).map_err(|e| format!("{}: {e}", p.display()))
    };
    match lang {
        "rust" => {
            write("Cargo.toml", &fill(CARGO, &id, &name))?;
            write("plugin.toml", &fill(MANIFEST, &id, &name))?;
            write("src/lib.rs", &fill(RUST_LIB, &id, &name))?;
            write("wit/formcraft.wit", formcraft_plugin_api::WIT)?;
            write(".gitignore", "target/\n")?;
            eprintln!("created {} (Rust add-in). Build with:", dir.display());
            eprintln!("  cargo build --release --target wasm32-wasip2");
        }
        "python-script" => {
            write(&format!("{id}.py"), &fill(PYTHON, &id, &name))?;
            eprintln!("created {} (Python script; needs the `formcraft` package, see python/)", dir.display());
        }
        other => return Err(format!("unknown --lang `{other}` (rust, python-script)")),
    }
    Ok(())
}
