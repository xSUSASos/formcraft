use std::sync::Arc;

use serde_json::json;

use super::*;

fn session() -> Session {
    Session::new()
}

#[test]
fn builtins_are_registered() {
    let mut s = session();
    let v = s.execute("app.commands", &json!({})).unwrap();
    let ids: Vec<&str> = v["commands"].as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&"app.about"));
    assert_eq!(s.execute("app.about", &Value::Null).unwrap()["kernel"], "occt");
}

#[test]
fn runtime_registration_and_unload() {
    let mut s = session();
    let n = s.registry().len();
    let spec = CommandSpec::new("gear.make", "Gear", "Spur gear").source(Source::Plugin("gear".into()));
    s.registry_mut().register(spec, Arc::new(|_, p| Ok(json!({"z": p["z"]})))).unwrap();
    assert_eq!(s.execute("gear.make", &json!({"z": 20})).unwrap()["z"], 20);
    assert_eq!(s.registry().search("gea").first().map(|c| c.id.as_str()), Some("gear.make"));
    assert_eq!(s.registry_mut().unregister_source(&Source::Plugin("gear".into())), vec!["gear.make".to_string()]);
    assert_eq!(s.registry().len(), n);
    assert!(matches!(s.execute("gear.make", &json!({})), Err(EngineError::UnknownCommand(_))));
}

#[test]
fn registration_is_validated() {
    let mut r = Registry::default();
    let h: Handler = Arc::new(|_, _| Ok(Value::Null));
    assert!(r.register(CommandSpec::new("Bad Id", "x", ""), h.clone()).is_err());
    assert!(r.register(CommandSpec::new("a..b", "x", ""), h.clone()).is_err());
    assert!(r.register(CommandSpec::new("ok.id", "x", "").params(json!([])), h.clone()).is_err());
    r.register(CommandSpec::new("ok.id", "x", ""), h.clone()).unwrap();
    assert!(r.register(CommandSpec::new("ok.id", "dup", ""), h).is_err(), "duplicates are refused");
}

#[test]
fn panicking_command_is_contained() {
    let mut s = session();
    s.registry_mut()
        .register(CommandSpec::new("test.boom", "Boom", ""), Arc::new(|_, _| -> Result<Value> { panic!("boom") }))
        .unwrap();
    let e = s.execute("test.boom", &json!({})).unwrap_err();
    assert!(matches!(e, EngineError::Internal(_, ref m) if m == "boom"), "{e:?}");
    assert!(!s.log().last().unwrap().ok);
    // still usable
    assert!(s.execute("app.about", &json!({})).is_ok());
}

#[test]
fn params_must_be_an_object() {
    let mut s = session();
    assert!(matches!(s.execute("app.about", &json!([1, 2])), Err(EngineError::InvalidParams(_))));
}

#[test]
fn search_ranks_label_prefix_first() {
    let mut r = Registry::default();
    let h: Handler = Arc::new(|_, _| Ok(Value::Null));
    r.register(CommandSpec::new("solid.extrude", "Extrude", "Push a profile into a body"), h.clone()).unwrap();
    r.register(CommandSpec::new("solid.fillet", "Fillet", "Round edges, e.g. after extrude"), h).unwrap();
    let hits: Vec<&str> = r.search("ext").iter().map(|c| c.id.as_str()).collect();
    assert_eq!(hits, vec!["solid.extrude", "solid.fillet"]);
    assert!(r.search("   ").is_empty());
}

// ---------------------------------------------------------------- Phase 2 modelling

fn inspect_body(s: &mut Session, i: usize) -> Value {
    s.execute("model.inspect", &json!({})).unwrap()["bodies"][i].clone()
}

#[test]
fn bracket_plate_with_two_holes_and_parameters() {
    let mut s = session();
    s.execute("doc.parameter", &json!({"name": "L", "expression": 60})).unwrap();
    s.execute("doc.parameter", &json!({"name": "hole_d", "expression": "6"})).unwrap();
    let plate = s.execute("solid.box", &json!({"size": ["L", 40, 5]})).unwrap();
    assert_eq!(plate["body_name"], "Body1");
    for x in [10, 50] {
        s.execute(
            "solid.cylinder",
            &json!({"base": [x, 20, -1], "diameter": "hole_d", "height": 7, "operation": "cut", "target": "Body1"}),
        )
        .unwrap();
    }
    let b = inspect_body(&mut s, 0);
    let want = 60.0 * 40.0 * 5.0 - 2.0 * std::f64::consts::PI * 9.0 * 5.0;
    assert!((b["volume"].as_f64().unwrap() - want).abs() < 1e-3, "{b}");
    assert_eq!(b["bbox"]["size"], json!([60.0, 40.0, 5.0]));
    assert_eq!(b["cylindrical_faces"], 2);

    // Parameter change propagates.
    s.execute("doc.parameter", &json!({"name": "L", "expression": "80"})).unwrap();
    assert_eq!(inspect_body(&mut s, 0)["bbox"]["size"][0], 80.0);
}

#[test]
fn failing_feature_is_refused_atomically() {
    let mut s = session();
    s.execute("solid.box", &json!({"size": [10, 10, 10]})).unwrap();
    let before = s.document().features.len();
    let e = s.execute("solid.sphere", &json!({"radius": -5})).unwrap_err();
    assert!(e.to_string().contains("radius"), "{e}");
    assert_eq!(s.document().features.len(), before, "nothing added");
    assert!(s.execute("solid.box", &json!({"size": [1, 1, 1], "operation": "cut", "target": "Nope"})).is_err());
    assert!(s.execute("solid.box", &json!({"size": [1, 1]})).is_err());
    assert!(s.execute("solid.cylinder", &json!({"radius": 1, "diameter": 2, "height": 1})).is_err());
    assert!(s.execute("solid.box", &json!({"size": ["unknown_param", 1, 1]})).is_err());
}

#[test]
fn undo_redo_walks_history() {
    let mut s = session();
    s.execute("solid.box", &json!({"size": [10, 10, 10]})).unwrap();
    s.execute("solid.sphere", &json!({"center": [30, 0, 0], "radius": 3})).unwrap();
    assert_eq!(s.model().bodies.len(), 2);
    s.execute("doc.undo", &json!({})).unwrap();
    assert_eq!(s.model().bodies.len(), 1);
    s.execute("doc.redo", &json!({})).unwrap();
    assert_eq!(s.model().bodies.len(), 2);
    s.execute("doc.undo", &json!({})).unwrap();
    s.execute("doc.undo", &json!({})).unwrap();
    assert!(s.execute("doc.undo", &json!({})).is_err());
    assert_eq!(s.model().bodies.len(), 0);
}

#[test]
fn combine_and_body_commands() {
    let mut s = session();
    s.execute("solid.box", &json!({"size": [10, 10, 10]})).unwrap();
    s.execute("solid.box", &json!({"corner": [5, 5, 5], "size": [10, 10, 10]})).unwrap();
    s.execute("solid.combine", &json!({"target": "Body1", "tool": "Body2", "operation": "cut"})).unwrap();
    assert_eq!(s.model().bodies.len(), 1);
    assert!((inspect_body(&mut s, 0)["volume"].as_f64().unwrap() - 875.0).abs() < 1e-6);
    s.execute("body.rename", &json!({"body": "Body1", "name": "Bracket"})).unwrap();
    s.execute("body.visibility", &json!({"body": "Bracket"})).unwrap();
    assert!(!s.model().bodies[0].visible);
    assert!(
        s.execute("doc.export", &json!({"path": std::env::temp_dir().join("fc-none.stl").to_str().unwrap()})).is_err(),
        "no visible bodies"
    );
}

#[test]
fn export_stl_has_the_right_volume() {
    let mut s = session();
    s.execute("solid.box", &json!({"size": [20, 10, 5]})).unwrap();
    let path = std::env::temp_dir().join(format!("fc-export-{}.stl", std::process::id()));
    s.execute("doc.export", &json!({"path": path.to_str().unwrap()})).unwrap();
    let mesh = formcraft_io::stl::from_bytes(&std::fs::read(&path).unwrap()).unwrap();
    let _ = std::fs::remove_file(&path);
    assert!((mesh.volume() - 1000.0).abs() < 1e-3);
    assert!(s.execute("doc.export", &json!({"path": "x.unknown"})).is_err());
}

#[test]
fn timeline_commands() {
    let mut s = session();
    s.execute("solid.box", &json!({"size": [10, 10, 10]})).unwrap();
    s.execute("solid.box", &json!({"size": [5, 5, 5], "operation": "cut", "target": "Body1"})).unwrap();
    s.execute("timeline.marker", &json!({"index": 1})).unwrap();
    assert!((inspect_body(&mut s, 0)["volume"].as_f64().unwrap() - 1000.0).abs() < 1e-6);
    s.execute("timeline.marker", &json!({"index": null})).unwrap();
    s.execute("feature.suppress", &json!({"feature": "Box2"})).unwrap();
    assert!((inspect_body(&mut s, 0)["volume"].as_f64().unwrap() - 1000.0).abs() < 1e-6);
    s.execute("feature.rename", &json!({"feature": "Box2", "name": "Pocket"})).unwrap();
    s.execute("feature.delete", &json!({"feature": "Pocket"})).unwrap();
    assert_eq!(s.document().features.len(), 1);
    let insp = s.execute("model.inspect", &json!({})).unwrap();
    assert_eq!(insp["features"][0]["status"], "ok");
}

#[test]
fn selection_commands() {
    let mut s = session();
    s.execute("solid.box", &json!({"size": [1, 1, 1]})).unwrap();
    s.execute("select.set", &json!({"picks": [{"type": "face", "body": 1, "index": 0}]})).unwrap();
    assert_eq!(s.selection().items.len(), 1);
    assert!(s.execute("select.set", &json!({"picks": [{"type": "body", "body": 77}]})).is_err());
    assert!(s.execute("select.set", &json!({"picks": "garbage"})).is_err());
    s.execute("select.clear", &json!({})).unwrap();
    assert!(s.selection().items.is_empty());
}

/// Every command with garbage params returns an error or a value, never panics.
#[test]
fn hostile_params_never_panic() {
    let mut s = session();
    s.execute("solid.box", &json!({"size": [1, 1, 1]})).unwrap();
    let ids: Vec<String> = s.registry().specs().map(|c| c.id.clone()).collect();
    let junk = [
        json!({}),
        json!({"size": "x", "radius": [], "path": 5, "name": {}, "feature": -1, "index": -3, "body": 1e300}),
        json!({"size": [1e308, 1e308, 1e308], "radius": "1/0", "expression": "(((("}),
        json!({"picks": [{"type": "edge", "body": 1, "index": 4000000000u64}], "tools": [1, 1, 1]}),
    ];
    for id in &ids {
        if id == "doc.new" {
            continue;
        }
        for j in &junk {
            let r = s.execute(id, j);
            assert!(!matches!(r, Err(EngineError::Internal(..))), "{id} panicked with {j}");
        }
    }
}

// ---------------------------------------------------------------- Phase 3 commands

fn vol0(s: &mut Session) -> f64 {
    inspect_body(s, 0)["volume"].as_f64().unwrap()
}

#[test]
fn parametric_bracket_by_commands() {
    use std::f64::consts::PI;
    let mut s = session();
    for (n, v) in [("L", "60"), ("W", "40"), ("T", "5"), ("hole_d", "6.6"), ("edge", "8")] {
        s.execute("doc.parameter", &json!({"name": n, "expression": v})).unwrap();
    }
    let sk = s
        .execute(
            "sketch.create",
            &json!({"plane": "XY", "rectangles": [{"corner": [0, 0], "size": ["L", "W"]}],
                    "circles": [{"center": ["edge", "edge"], "diameter": "hole_d"}, {"center": ["L - edge", "W - edge"], "diameter": "hole_d"}]}),
        )
        .unwrap();
    assert_eq!(sk["profiles"], 3, "{sk}");
    s.execute("solid.extrude", &json!({"distance": "T"})).unwrap();
    s.execute("solid.fillet", &json!({"edges": "vertical", "radius": 3})).unwrap();
    let expect = |l: f64, w: f64, t: f64, d: f64| (l * w - (4.0 - PI) * 9.0 - 2.0 * PI * d * d / 4.0) * t;
    assert!((vol0(&mut s) - expect(60.0, 40.0, 5.0, 6.6)).abs() < 1e-3, "{}", vol0(&mut s));
    s.execute("doc.parameter", &json!({"name": "L", "expression": "90"})).unwrap();
    s.execute("doc.parameter", &json!({"name": "hole_d", "expression": "9"})).unwrap();
    assert_eq!(s.model().error_count(), 0, "{:?}", s.model().status);
    assert!((vol0(&mut s) - expect(90.0, 40.0, 5.0, 9.0)).abs() < 1e-3);
    let b = inspect_body(&mut s, 0);
    assert_eq!(b["bbox"]["size"], json!([90.0, 40.0, 5.0]));
}

#[test]
fn flange_with_bolt_circle() {
    use std::f64::consts::PI;
    let mut s = session();
    s.execute("sketch.create", &json!({"circles": [{"center": [0, 0], "diameter": 100}]})).unwrap();
    s.execute("solid.extrude", &json!({"distance": 10})).unwrap();
    let pts: Vec<Value> = (0..4)
        .map(|i| {
            let a = PI / 4.0 + i as f64 * PI / 2.0;
            json!([40.0 * a.cos(), 40.0 * a.sin(), 10])
        })
        .collect();
    s.execute("solid.hole", &json!({"points": pts, "diameter": 9})).unwrap();
    let b = inspect_body(&mut s, 0);
    let want = PI * 2500.0 * 10.0 - 4.0 * PI * 20.25 * 10.0;
    assert!((b["volume"].as_f64().unwrap() - want).abs() < 1e-3, "{b}");
    assert_eq!(b["cylindrical_faces"], 5);
}

#[test]
fn hole_counterbore_shell_press_pull_by_commands() {
    let mut s = session();
    s.execute("solid.box", &json!({"size": [50, 30, 20]})).unwrap();
    s.execute("solid.shell", &json!({"thickness": 2})).unwrap();
    s.execute("solid.press_pull", &json!({"face": "bottom", "distance": 3})).unwrap();
    s.execute(
        "solid.hole",
        &json!({"face": "bottom", "points": [[25, 15, 0]], "diameter": 4, "counterbore": {"diameter": 8, "depth": 2}}),
    )
    .unwrap();
    s.execute("solid.chamfer", &json!({"edges": "top_outer", "distance": 0.5})).unwrap();
    assert_eq!(s.model().error_count(), 0, "{:?}", s.model().status);
    let b = inspect_body(&mut s, 0);
    assert!((b["bbox"]["min"][2].as_f64().unwrap() + 3.0).abs() < 1e-6, "{b}");
    assert_eq!(b["valid"], true);
}

#[test]
fn revolve_and_errors_by_commands() {
    let mut s = session();
    s.execute(
        "sketch.create",
        &json!({"plane": "XZ", "polylines": [{"points": [[5, 0], [15, 0], [15, 20], [5, 20]]}]}),
    )
    .unwrap();
    s.execute("solid.revolve", &json!({"axis": "Z"})).unwrap();
    let v = vol0(&mut s);
    assert!((v - std::f64::consts::PI * 200.0 * 20.0).abs() < 1e-3, "{v}");
    assert!(s.execute("solid.extrude", &json!({"sketch": "Revolve1", "distance": 5})).is_err());
    assert!(s.execute("solid.fillet", &json!({"edges": "diagonal", "radius": 1})).is_err());
    assert!(s.execute("solid.fillet", &json!({"radius": 100})).is_err());
    assert!(s.execute("sketch.create", &json!({"rectangles": [{"size": [0, 5]}]})).is_err());
    assert!(s.execute("sketch.create", &json!({"plane": "QQ"})).is_err());
    assert!(s.execute("solid.hole", &json!({"points": [], "diameter": 3})).is_err());
    assert_eq!(s.model().error_count(), 0, "refused edits leave no red features");
}

#[test]
fn save_and_open_round_trip() {
    let mut s = session();
    s.execute("doc.parameter", &json!({"name": "L", "expression": 60})).unwrap();
    s.execute("sketch.create", &json!({"rectangles": [{"corner": [0, 0], "size": ["L", 20]}]})).unwrap();
    s.execute("solid.extrude", &json!({"distance": 4})).unwrap();
    assert!(s.is_dirty());
    let path = std::env::temp_dir().join(format!("fc-save-{}", std::process::id()));
    let r = s.execute("doc.save", &json!({"path": path.to_str().unwrap()})).unwrap();
    assert!(!s.is_dirty());
    let saved = r["path"].as_str().unwrap().to_string();
    assert!(saved.ends_with(".fcraft"));
    let v = vol0(&mut s);
    let mut t = session();
    t.execute("doc.open", &json!({"path": saved})).unwrap();
    assert!((vol0(&mut t) - v).abs() < 1e-9);
    assert_eq!(t.document().parameters.len(), 1);
    t.execute("doc.save", &json!({})).unwrap();
    let _ = std::fs::remove_file(&saved);
    assert!(t.execute("doc.open", &json!({"path": "nope.fcraft"})).is_err());
}

#[test]
fn edit_feature_and_preview() {
    let mut s = session();
    s.execute("sketch.create", &json!({"rectangles": [{"corner": [0, 0], "size": [30, 20]}]})).unwrap();
    let ex = s.execute("solid.extrude", &json!({"distance": 5})).unwrap();
    s.execute("solid.fillet", &json!({"edges": "vertical", "radius": 2})).unwrap();
    let id = ex["feature"].as_u64().unwrap();
    let p = s.execute("feature.params", &json!({"feature": id})).unwrap();
    assert_eq!(p["command"], "solid.extrude");
    assert_eq!(p["params"]["distance"], "5");
    // preview does not touch the document
    let m = s.preview("feature.edit", &json!({"feature": id, "params": {"distance": 12}})).unwrap();
    assert_eq!(m.error_count(), 0);
    assert!((inspect_body(&mut s, 0)["bbox"]["size"][2].as_f64().unwrap() - 5.0).abs() < 1e-9);
    s.execute("feature.edit", &json!({"feature": id, "params": {"distance": 12}})).unwrap();
    assert!((inspect_body(&mut s, 0)["bbox"]["size"][2].as_f64().unwrap() - 12.0).abs() < 1e-9);
    assert_eq!(s.model().error_count(), 0, "the fillet downstream still works");
    assert_eq!(s.document().features.len(), 3);
    s.execute("doc.undo", &json!({})).unwrap();
    assert!((inspect_body(&mut s, 0)["bbox"]["size"][2].as_f64().unwrap() - 5.0).abs() < 1e-9);
    assert!(s.execute("feature.edit", &json!({"feature": id, "params": {"distance": 0}})).is_err());
    assert!(s.execute("feature.edit", &json!({"feature": "Sketch1", "params": {}})).is_err());
}

#[test]
fn interactive_sketch_editing() {
    let mut s = session();
    let sk = s.execute("sketch.create", &json!({"plane": "XY"})).unwrap();
    let id = sk["feature"].as_u64().unwrap();
    let r = s
        .execute(
            "sketch.edit",
            &json!({"sketch": id, "ops": [
                {"op": "line", "a": [0, 0], "b": [40, 0]},
            ]}),
        )
        .unwrap();
    let ids: Vec<u64> = r["created"][0].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
    let (p0, p1, l0) = (ids[0], ids[1], ids[2]);
    // continue the chain from the last point, close the triangle on the first
    let r = s
        .execute(
            "sketch.edit",
            &json!({"sketch": id, "ops": [
                {"op": "line", "a_point": p1, "b": [20, 30]},
            ]}),
        )
        .unwrap();
    let p2 = r["created"][0][1].as_u64().unwrap();
    s.execute(
        "sketch.edit",
        &json!({"sketch": id, "ops": [
            {"op": "line", "a_point": p2, "b_point": p0},
            {"op": "constraint", "constraint": {"type": "horizontal", "line": l0}},
            {"op": "dimension", "dimension": {"type": "length", "line": l0}, "value": 50},
        ]}),
    )
    .unwrap();
    let g = s.execute("sketch.get", &json!({"sketch": id})).unwrap();
    assert_eq!(g["regions"].as_array().unwrap().len(), 1, "{g}");
    let area = g["regions"][0]["area"].as_f64().unwrap();
    assert!(area > 600.0, "triangle grew with the 50 mm base: {area}");
    s.execute("solid.extrude", &json!({"distance": 3})).unwrap();
    // over-constrain → refused, nothing changes
    let before = s.document().clone();
    assert!(s.execute("sketch.edit", &json!({"sketch": id, "ops": [{"op": "dimension", "dimension": {"type": "distance", "a": p0, "b": p1}, "value": 70}]})).is_err());
    assert_eq!(s.document(), &before);
    assert!(s.execute("sketch.edit", &json!({"sketch": id, "ops": [{"op": "fly"}]})).is_err());
    assert!(
        s.execute("sketch.edit", &json!({"sketch": id, "ops": [{"op": "move", "point": 9999, "to": [0, 0]}]})).is_err()
    );
}

#[test]
fn transaction_is_one_undo_step_with_a_group() {
    let mut s = session();
    s.execute("solid.box", &json!({"size": [10, 10, 10]})).unwrap();
    let ((), added) = s
        .transaction("AI: plate", Some("Plate with holes"), |s| {
            s.execute("doc.parameter", &json!({"name": "t", "expression": 5}))?;
            s.execute("solid.box", &json!({"corner": [20, 0, 0], "size": [30, 20, "t"]}))?;
            s.execute("solid.cylinder", &json!({"base": [35, 10, 0], "diameter": 6, "height": 5}))?;
            Ok(())
        })
        .unwrap();
    assert_eq!(added.len(), 2);
    let d = s.document();
    let g = d.feature(added[0]).unwrap().group.unwrap();
    assert_eq!(d.groups[&g], "Plate with holes");
    assert!(added.iter().all(|id| d.feature(*id).unwrap().group == Some(g)));
    assert_eq!(s.undo_label(), Some("AI: plate"));
    s.undo().unwrap();
    assert_eq!(s.document().features.len(), 1);
    assert!(s.document().parameter("t").is_none());
    s.redo().unwrap();
    assert_eq!(s.document().features.len(), 3);
}

#[test]
fn failed_transaction_rolls_back_everything() {
    let mut s = session();
    s.execute("solid.box", &json!({"size": [10, 10, 10]})).unwrap();
    let rev = s.document().clone();
    let r = s.transaction("bad", None, |s| {
        s.execute("solid.box", &json!({"corner": [20, 0, 0], "size": [5, 5, 5]}))?;
        s.execute("solid.fillet", &json!({"edges": "vertical", "radius": 500}))?;
        Ok(())
    });
    assert!(r.is_err());
    assert_eq!(*s.document(), rev);
    assert_eq!(s.model().bodies.len(), 1);
    assert_eq!(s.undo_label(), Some("Add Box"));
}

#[test]
fn fork_is_independent() {
    let mut s = session();
    s.execute("solid.box", &json!({"size": [10, 10, 10]})).unwrap();
    let mut f = s.fork();
    f.execute("solid.box", &json!({"corner": [20, 0, 0], "size": [5, 5, 5]})).unwrap();
    assert_eq!(f.model().bodies.len(), 2);
    assert_eq!(s.model().bodies.len(), 1);
    assert!(!f.can_undo() || f.undo_label().is_some());
}
