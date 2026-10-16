use std::path::PathBuf;
use std::sync::Arc;

use formcraft_engine::Session;
use serde_json::json;

use super::*;

fn gear_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/plugins/gear")
}

fn setup() -> (Session, Arc<Plugins>) {
    let s = Session::new();
    let p = Plugins::new(s.kernel().clone()).expect("engine");
    (s, p)
}

fn volume(s: &mut Session) -> f64 {
    s.execute("model.inspect", &json!({})).unwrap()["bodies"][0]["volume"].as_f64().unwrap_or(0.0)
}

#[test]
fn gear_plugin_adds_a_parametric_feature() {
    let (mut s, plugins) = setup();
    let l = plugins.load(&gear_dir(), &mut s).expect("gear loads");
    assert_eq!(l.registered().features, vec!["spur"]);
    let spec = s.registry().get("gear.spur").expect("feature command registered").spec.clone();
    assert!(spec.ai, "offered to the AI");
    assert_eq!(spec.placement.as_ref().map(|p| p.group.as_str()), Some("Create"));
    s.execute("doc.parameter", &json!({"name": "z", "expression": 20})).unwrap();
    let r = s.execute("gear.spur", &json!({"teeth": "z", "module": 2, "thickness": 10, "bore": 8})).unwrap();
    assert!(r["message"].as_str().unwrap_or("").contains("Spur Gear1"), "{r}");
    let v20 = volume(&mut s);
    // outer r = 22, root r = 17.5, bore r = 4: the volume lies between the root and tip discs
    let pi = std::f64::consts::PI;
    assert!(v20 > pi * (17.5f64.powi(2) - 16.0) * 10.0 && v20 < pi * (22f64.powi(2) - 16.0) * 10.0, "{v20}");
    let inspect = s.execute("model.inspect", &json!({})).unwrap();
    assert_eq!(inspect["bodies"][0]["valid"], true);
    // a parameter change rebuilds through the plugin
    s.execute("doc.parameter", &json!({"name": "z", "expression": 30})).unwrap();
    let v30 = volume(&mut s);
    assert!(v30 > v20 * 1.8, "{v20} → {v30}");
    // editing goes through the plugin command
    s.execute("feature.edit", &json!({"feature": "Spur Gear1", "params": {"thickness": 5}})).unwrap();
    assert!((volume(&mut s) - v30 / 2.0).abs() < v30 * 1e-6);
    // hostile parameters are errors, not crashes
    assert!(s.execute("gear.spur", &json!({"teeth": 3, "module": 2})).is_err());
    assert!(s.execute("gear.spur", &json!({"teeth": 20, "module": "nope"})).is_err());
    assert!(plugins.list()[0].log().iter().any(|l| l.contains("spur gear")));
    // unloading: commands go, the feature keeps its frozen body with a warning
    assert!(plugins.unload("gear", &mut s));
    assert!(s.registry().get("gear.spur").is_none());
    assert!(matches!(s.model().status.last().map(|x| &x.status), Some(formcraft_engine::regen::Status::Warning(_))));
    assert!(volume(&mut s) > 0.0);
}

#[test]
fn bad_plugins_are_refused_cleanly() {
    let (mut s, plugins) = setup();
    let dir = std::env::temp_dir().join(format!("fc-bad-plugin-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // no manifest
    assert!(plugins.load(&dir, &mut s).is_err());
    // manifest for a future API
    std::fs::write(
        dir.join("plugin.toml"),
        "id = \"x\"\nname = \"X\"\nversion = \"1\"\napi = \"2.0\"\nwasm = \"x.wasm\"\n",
    )
    .unwrap();
    let e = plugins.load(&dir, &mut s).err().unwrap().to_string();
    assert!(e.contains("API"), "{e}");
    // garbage instead of a component
    std::fs::write(
        dir.join("plugin.toml"),
        "id = \"x\"\nname = \"X\"\nversion = \"1\"\napi = \"1.0\"\nwasm = \"x.wasm\"\n",
    )
    .unwrap();
    std::fs::write(dir.join("x.wasm"), b"\0asm garbage").unwrap();
    assert!(plugins.load(&dir, &mut s).is_err());
    let n = s.registry().len();
    assert_eq!(n, Session::new().registry().len(), "nothing registered");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn install_from_zip_checks_paths() {
    use std::io::Write;
    let tmp = std::env::temp_dir().join(format!("fc-zip-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let make = |name: &str, files: &[(&str, &[u8])]| {
        let path = tmp.join(name);
        let mut z = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        for (n, data) in files {
            z.start_file(*n, zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(data).unwrap();
        }
        z.finish().unwrap();
        path
    };
    let manifest = std::fs::read(gear_dir().join("plugin.toml")).unwrap();
    let wasm = std::fs::read(gear_dir().join("gear.wasm")).unwrap();
    let good = make("gear.zip", &[("gear/plugin.toml", &manifest), ("gear/gear.wasm", &wasm)]);
    let plugins_dir = tmp.join("plugins");
    std::fs::create_dir_all(&plugins_dir).unwrap();
    let (m, dir) = install::from_zip(&good, &plugins_dir).expect("installs");
    assert_eq!(m.id, "gear");
    assert!(dir.join("gear.wasm").is_file());
    let (mut s, plugins) = setup();
    plugins.load(&dir, &mut s).expect("installed plugin loads");
    let evil = make("evil.zip", &[("plugin.toml", &manifest), ("../evil.wasm", &wasm), ("gear.wasm", &wasm)]);
    assert!(install::from_zip(&evil, &plugins_dir).is_err());
    let missing = make("missing.zip", &[("plugin.toml", &manifest)]);
    assert!(install::from_zip(&missing, &plugins_dir).is_err());
    let _ = std::fs::remove_dir_all(&tmp);
}

fn spin_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/plugins/spin")
}

/// PLAN Phase 6 acceptance: an infinite loop is cut off by the time limit and the plugin is
/// unloaded; memory hogs and panics also only unload the plugin; the session survives.
#[test]
fn misbehaving_plugins_are_unloaded_not_fatal() {
    let (mut s, plugins) = setup();
    plugins.load(&spin_dir(), &mut s).expect("spin loads");
    s.execute("solid.box", &json!({"size": [1, 2, 3]})).unwrap();
    // host.execute works while the session is lent to the plugin
    s.execute("spin.box", &json!({})).unwrap();
    assert_eq!(s.model().bodies.len(), 2);
    // infinite loop → timeout → unloaded
    let t = std::time::Instant::now();
    let e = s.execute("spin.forever", &json!({})).unwrap_err().to_string();
    assert!(e.contains("timed out"), "{e}");
    assert!(t.elapsed() < std::time::Duration::from_secs(10), "{:?}", t.elapsed());
    assert_eq!(s.model().bodies.len(), 2, "the session came back intact");
    let reaped = plugins.reap(&mut s);
    assert_eq!(reaped.first().map(|r| r.0.as_str()), Some("spin"));
    assert!(s.registry().get("spin.forever").is_none(), "commands of a dead plugin are removed");
    assert!(s.execute("solid.box", &json!({"size": [1, 1, 1]})).is_ok(), "the app keeps working");
    // memory hog and panic: fresh loads, each ends in an unload
    for cmd in ["spin.hog", "spin.panic"] {
        let (mut s, plugins) = setup();
        plugins.load(&spin_dir(), &mut s).expect("spin loads");
        assert!(s.execute(cmd, &json!({})).is_err(), "{cmd}");
        assert!(plugins.list()[0].is_dead(), "{cmd}");
    }
    let d = plugins.describe();
    assert_eq!(d[0]["loaded"], false);
    assert!(d[0]["failure"].as_str().unwrap_or("").contains("timed out"));
}
