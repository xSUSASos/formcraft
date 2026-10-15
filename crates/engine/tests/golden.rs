//! Golden geometry: reference parts built from `examples/golden/*.fcs`, checked against the
//! hand-computed values in `examples/golden/expected.json`.

use formcraft_engine::{Session, script};
use serde_json::{Value, json};

fn check(name: &str) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/golden");
    let expected: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("expected.json")).unwrap()).unwrap();
    let want = &expected[name];
    assert!(want.is_object(), "no expected entry for {name}");
    let text = std::fs::read_to_string(dir.join(format!("{name}.fcs"))).unwrap();
    let steps = script::parse(&text).unwrap();
    let mut s = Session::new();
    if let Err((i, e)) = script::run(&mut s, &steps) {
        panic!("{name}: step {i} failed: {e}");
    }
    let r = s.execute("model.inspect", &json!({})).unwrap();
    assert_eq!(r["errors"].as_u64().unwrap_or(0), 0, "{name}: {r}");
    let bodies = r["bodies"].as_array().unwrap();
    assert_eq!(bodies.len() as u64, want["bodies"].as_u64().unwrap(), "{name}: body count, {r}");
    let b = &bodies[0];
    assert_eq!(b["valid"], true, "{name}: invalid body {b}");
    let vol = b["volume"].as_f64().unwrap();
    let wv = want["volume"].as_f64().unwrap();
    assert!((vol - wv).abs() <= 1e-3 * wv, "{name}: volume {vol} != {wv}");
    for (i, w) in want["bbox_size"].as_array().unwrap().iter().enumerate() {
        let got = b["bbox"]["size"][i].as_f64().unwrap();
        let w = w.as_f64().unwrap();
        assert!((got - w).abs() < 1e-6, "{name}: bbox[{i}] {got} != {w}");
    }
}

#[test]
fn golden_plate() {
    check("plate");
}

#[test]
fn golden_bushing() {
    check("bushing");
}

#[test]
fn golden_flange() {
    check("flange");
}

#[test]
fn golden_l_bracket() {
    check("l_bracket");
}

#[test]
fn golden_washer() {
    check("washer");
}

#[test]
fn golden_hex_nut() {
    check("hex_nut");
}

#[test]
fn golden_shaft_keyway() {
    check("shaft_keyway");
}

#[test]
fn golden_enclosure() {
    check("enclosure");
}

#[test]
fn golden_pulley() {
    check("pulley");
}

#[test]
fn golden_rounded_plate() {
    check("rounded_plate");
}
