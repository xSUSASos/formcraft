//! Interaction tests: drive the whole shell with simulated input (no window, no GPU).

use egui::{Event, Modifiers, MouseWheelUnit, PointerButton, Pos2, TouchPhase, Vec2, pos2};
use egui_kittest::Harness;

use super::*;
use crate::sketch_mode::Tool;

const SIZE: Vec2 = Vec2::new(1440.0, 900.0);

fn harness() -> Harness<'static, FormCraftApp> {
    let app = FormCraftApp::with_state(Layout::default(), Camera::home(), AppOptions::default());
    let mut h = Harness::builder().with_size(SIZE).build_ui_state(|ui, app: &mut FormCraftApp| app.draw(ui), app);
    h.run_steps(3);
    h
}

/// A point in the middle of the canvas (right of the browser, above the bottom panels).
fn canvas_point() -> Pos2 {
    pos2(800.0, 450.0)
}

fn drag(h: &mut Harness<'_, FormCraftApp>, button: PointerButton, modifiers: Modifiers, from: Pos2, by: Vec2) {
    h.event(Event::ModifiersChanged(modifiers));
    h.event(Event::PointerMoved(from));
    h.step();
    h.event(Event::PointerButton { pos: from, button, pressed: true, modifiers });
    h.step();
    for k in 1..=5 {
        h.event(Event::PointerMoved(from + by * (k as f32 / 5.0)));
        h.step();
    }
    h.event(Event::PointerButton { pos: from + by, button, pressed: false, modifiers });
    h.step();
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.step();
}

fn click(h: &mut Harness<'_, FormCraftApp>, at: Pos2) {
    h.event(Event::PointerMoved(at));
    h.step();
    h.event(Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.step();
    h.event(Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(2);
}

#[test]
fn shift_middle_drag_orbits() {
    let mut h = harness();
    let before = h.state().camera;
    drag(&mut h, PointerButton::Middle, Modifiers::SHIFT, canvas_point(), Vec2::new(120.0, 0.0));
    let after = h.state().camera;
    assert!((after.yaw - before.yaw).abs() > 0.1, "yaw {} -> {}", before.yaw, after.yaw);
    assert_eq!(after.target, before.target, "orbit keeps the target");
}

#[test]
fn middle_drag_pans() {
    let mut h = harness();
    let before = h.state().camera;
    drag(&mut h, PointerButton::Middle, Modifiers::NONE, canvas_point(), Vec2::new(100.0, 40.0));
    let after = h.state().camera;
    assert!(after.target.distance(before.target) > 1.0, "{:?} -> {:?}", before.target, after.target);
    assert_eq!(after.yaw, before.yaw, "pan does not rotate");
}

#[test]
fn wheel_zooms_in_and_out() {
    let mut h = harness();
    let d0 = h.state().camera.distance;
    h.event(Event::PointerMoved(canvas_point()));
    h.step();
    let wheel = |dy: f32| Event::MouseWheel {
        unit: MouseWheelUnit::Line,
        delta: Vec2::new(0.0, dy),
        phase: TouchPhase::Move,
        modifiers: Modifiers::NONE,
    };
    h.event(wheel(3.0));
    h.run_steps(20);
    let d1 = h.state().camera.distance;
    assert!(d1 < d0, "wheel up zooms in: {d0} -> {d1}");
    h.event(wheel(-6.0));
    h.run_steps(20);
    assert!(h.state().camera.distance > d1, "wheel down zooms out");
}

#[test]
fn view_cube_top_face_sets_top_view() {
    let mut h = harness();
    // Screen position of the TOP face centre, computed the way viewcube::show projects it.
    let cam = h.state().camera;
    let area = h.state().cube_area;
    assert!(area.is_positive(), "the cube was drawn");
    let center = area.center() + Vec2::new(8.0, 6.0);
    let scale = area.width() * 0.24;
    let p = center + Vec2::new(DVec3::Z.dot(cam.right()) as f32, -(DVec3::Z.dot(cam.up()) as f32)) * scale;
    click(&mut h, p);
    let switched = h.state().camera.back().z > 0.99;
    assert!(switched, "clicking TOP on the ViewCube looks from +Z (back = {:?})", h.state().camera.back());
}

#[test]
fn browser_float_close_and_reset() {
    let mut h = harness();
    h.state_mut().layout.browser.dock = Dock::Floating;
    h.run_steps(3);
    h.state_mut().layout.browser.open = false;
    h.run_steps(3);
    h.state_mut().layout.reset();
    h.run_steps(3);
    let l = &h.state().layout;
    assert!(l.browser.open && l.browser.dock == Dock::Docked);
}

#[test]
fn text_command_line_runs_commands() {
    let mut h = harness();
    h.state_mut().submit_command_line("app.about");
    h.state_mut().submit_command_line("app.commands {\"filter\": \"about\"}");
    h.state_mut().submit_command_line("app.about {not json");
    h.state_mut().submit_command_line("no.such.command");
    h.run_steps(2);
    let log: Vec<bool> = h.state_mut().session().log().iter().map(|e| e.ok).collect();
    assert_eq!(log, vec![true, true, false, false]);
}

#[test]
fn both_themes_render() {
    let mut h = harness();
    h.state_mut().layout.theme = ThemeKind::Light;
    h.state_mut().layout.text_commands.open = true;
    h.run_steps(3);
    h.state_mut().layout.theme = ThemeKind::Dark;
    h.run_steps(3);
}

#[test]
fn clicking_the_canvas_picks_faces() {
    let mut h = harness();
    h.state_mut()
        .session()
        .execute("solid.box", &serde_json::json!({"corner": [-30, -20, 0], "size": [60, 40, 5]}))
        .unwrap();
    h.state_mut().apply_named_view("TOP");
    h.state_mut().fit();
    h.run_steps(3);
    let c = h.state().canvas_rect.center();
    click(&mut h, c);
    let sel = h.state_mut().session().selection().items.clone();
    assert!(matches!(sel.as_slice(), [Pick::Face { .. }]), "{sel:?}");
    // Ctrl-click the same face removes it again.
    h.event(Event::ModifiersChanged(Modifiers::CTRL));
    click(&mut h, c);
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.step();
    assert!(h.state_mut().session().selection().items.is_empty());
    // Click on empty space clears.
    click(&mut h, c);
    let empty = h.state().canvas_rect.left_top() + Vec2::new(20.0, 200.0);
    click(&mut h, empty);
    assert!(h.state_mut().session().selection().items.is_empty());
}

/// Screen position of a point on the active sketch plane.
fn sketch_px(h: &Harness<'_, FormCraftApp>, x: f64, y: f64) -> Pos2 {
    let app = h.state();
    let m = app.sketch_mode.as_ref().unwrap();
    let r = app.session.model().sketch(m.sketch).unwrap();
    let rect = app.canvas_rect;
    let size = DVec2::new(rect.width() as f64, rect.height() as f64);
    let s = app.camera.project(r.plane.to_world(formcraft_kernel::DVec2::new(x, y)), size).unwrap();
    rect.min + Vec2::new(s.x as f32, s.y as f32)
}

#[test]
fn drawing_in_sketch_mode() {
    let mut h = harness();
    h.state_mut().create_sketch(serde_json::json!("XY"));
    h.state_mut().camera.distance = 400.0;
    h.state_mut().camera.target = DVec3::new(70.0, 15.0, 0.0);
    h.run_steps(3);
    assert!(h.state().sketch_mode.is_some());
    // Rectangle: two clicks.
    h.state_mut().sketch_mode.as_mut().unwrap().set_tool(Tool::Rectangle);
    let (a, b) = (sketch_px(&h, 0.0, 0.0), sketch_px(&h, 40.0, 30.0));
    click(&mut h, a);
    click(&mut h, b);
    let id = h.state().sketch_mode.as_ref().unwrap().sketch;
    let regions = h.state().session.model().sketch(id).unwrap().regions.len();
    assert_eq!(regions, 1, "{}", h.state().status);
    // Triangle by a closed line chain to the right of it.
    h.state_mut().sketch_mode.as_mut().unwrap().set_tool(Tool::Line);
    for (x, y) in [(60.0, 0.0), (90.0, 0.0), (75.0, 25.0), (60.0, 0.0)] {
        let p = sketch_px(&h, x, y);
        click(&mut h, p);
    }
    let r = h.state().session.model().sketch(id).unwrap();
    assert_eq!(r.regions.len(), 2, "{}", h.state().status);
    // Dimension the rectangle's bottom edge: 50 mm.
    h.state_mut().sketch_mode.as_mut().unwrap().set_tool(Tool::Dimension);
    let p = sketch_px(&h, 20.0, 0.0);
    click(&mut h, p);
    for c in "50".chars() {
        h.event(Event::Text(c.to_string()));
    }
    // clear the prefilled value first: select all + type is fiddly in tests, so set it directly
    h.event(Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(3);
    h.state_mut().finish_sketch();
    h.state_mut().session().execute("solid.extrude", &serde_json::json!({"distance": 5})).unwrap();
    assert_eq!(h.state_mut().session().model().bodies.len(), 1);
    assert!(h.state().sketch_mode.is_none());
}

fn key(h: &mut Harness<'_, FormCraftApp>, k: egui::Key, modifiers: Modifiers) {
    h.event(Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers });
    h.step();
    h.event(Event::Key { key: k, physical_key: None, pressed: false, repeat: false, modifiers });
    h.run_steps(2);
}

#[test]
fn shortcut_box_runs_a_command() {
    let mut h = harness();
    h.event(Event::PointerMoved(canvas_point()));
    key(&mut h, egui::Key::S, Modifiers::NONE);
    assert!(h.state().search.is_some(), "S opens the shortcut box");
    for c in "box".chars() {
        h.event(Event::Text(c.to_string()));
        h.step();
    }
    key(&mut h, egui::Key::Enter, Modifiers::NONE);
    assert!(h.state().search.is_none());
    let d = h.state().dialog.as_ref().map(|d| d.command.clone());
    assert_eq!(d.as_deref(), Some("solid.box"), "Enter opens the Box dialog");
}

#[test]
fn hotkey_opens_dialog_and_marking_menu_opens() {
    let mut h = harness();
    h.state_mut().session().execute("solid.box", &serde_json::json!({"size": [10, 10, 10]})).unwrap();
    h.event(Event::PointerMoved(canvas_point()));
    key(&mut h, egui::Key::F, Modifiers::NONE);
    assert_eq!(h.state().dialog.as_ref().map(|d| d.command.as_str()), Some("solid.fillet"));
    h.state_mut().dialog = None;
    // right-click → marking menu
    let p = canvas_point();
    h.event(Event::PointerButton {
        pos: p,
        button: PointerButton::Secondary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.step();
    h.event(Event::PointerButton {
        pos: p,
        button: PointerButton::Secondary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(2);
    assert!(h.state().marking.is_some());
    key(&mut h, egui::Key::Escape, Modifiers::NONE);
    assert!(h.state().marking.is_none());
}

#[test]
fn parameters_window_edits_parameters() {
    let mut h = harness();
    h.state_mut().run("doc.parameters", Value::Null);
    assert!(h.state().params_rows.is_some());
    h.state_mut().run("doc.parameter", serde_json::json!({"name": "L", "expression": "60"}));
    h.run_steps(2);
    assert_eq!(h.state_mut().session().document().parameters.len(), 1);
}

/// Phase 3 acceptance: a bracket built with mouse and keyboard only, then a parameter change.
#[test]
fn acceptance_bracket_with_the_mouse() {
    let mut h = harness();
    // Parameters first (Change Parameters table → doc.parameter).
    h.state_mut().run("doc.parameter", serde_json::json!({"name": "L", "expression": "60"}));
    // Create Sketch on XY via the toolbar command and the plane picker.
    h.state_mut().run("sketch.create", Value::Null);
    assert!(h.state().plane_picker);
    h.state_mut().create_sketch(serde_json::json!("XY"));
    h.state_mut().camera.distance = 300.0;
    h.state_mut().camera.target = DVec3::new(20.0, 15.0, 0.0);
    h.run_steps(3);
    // R, then two clicks.
    h.event(Event::PointerMoved(canvas_point()));
    key(&mut h, egui::Key::R, Modifiers::NONE);
    assert_eq!(h.state().sketch_mode.as_ref().map(|m| m.tool), Some(Tool::Rectangle));
    let (a, b) = (sketch_px(&h, 0.0, 0.0), sketch_px(&h, 40.0, 30.0));
    click(&mut h, a);
    click(&mut h, b);
    // D, click the bottom edge, type "L", Enter.
    key(&mut h, egui::Key::D, Modifiers::NONE);
    let bottom = sketch_px(&h, 20.0, 0.0);
    click(&mut h, bottom);
    let typed = h.state_mut().sketch_mode.as_mut().map(|m| m.set_pending_dimension_text("L")).unwrap_or(false);
    assert!(typed, "the dimension field is open");
    key(&mut h, egui::Key::Enter, Modifiers::NONE);
    let sk = h.state().sketch_mode.as_ref().unwrap().sketch;
    assert_eq!(h.state().session.model().sketch(sk).unwrap().sketch.dimensions.len(), 1, "{}", h.state().status);
    // Finish Sketch, E → Extrude dialog, distance 5, OK.
    h.state_mut().finish_sketch();
    h.run_steps(2);
    key(&mut h, egui::Key::E, Modifiers::NONE);
    assert_eq!(h.state().dialog.as_ref().map(|d| d.command.as_str()), Some("solid.extrude"));
    h.state_mut().dialog.as_mut().unwrap().set_text("distance", "5".into());
    h.run_steps(2);
    h.state_mut().dialog_ok();
    assert_eq!(h.state().session.model().bodies.len(), 1, "{}", h.state().status);
    // F → Fillet dialog: vertical edges, R3.
    key(&mut h, egui::Key::F, Modifiers::NONE);
    {
        let d = h.state_mut().dialog.as_mut().unwrap();
        d.set_text("radius", "3".into());
        d.set_text("edges", "vertical".into());
    }
    h.run_steps(2);
    h.state_mut().dialog_ok();
    let vol = |h: &mut Harness<'_, FormCraftApp>| {
        h.state_mut().session().execute("model.inspect", &serde_json::json!({})).unwrap()["bodies"][0]["volume"]
            .as_f64()
            .unwrap()
    };
    let pi = std::f64::consts::PI;
    let want = |l: f64| (l * 30.0 - (4.0 - pi) * 9.0) * 5.0;
    assert!((vol(&mut h) - want(60.0)).abs() < 1e-3, "{}", vol(&mut h));
    // Change the parameter: the whole history rebuilds.
    h.state_mut().run("doc.parameter", serde_json::json!({"name": "L", "expression": "80"}));
    h.run_steps(2);
    assert_eq!(h.state().session.model().error_count(), 0);
    assert!((vol(&mut h) - want(80.0)).abs() < 1e-3, "{}", vol(&mut h));
    // Timeline: Sketch, Extrude, Fillet.
    let names: Vec<String> = h.state().session.document().features.iter().map(|f| f.name.clone()).collect();
    assert_eq!(names, vec!["Sketch1", "Extrude1", "Fillet1"]);
}

/// Control channel end to end: a TCP client builds a part in the running UI, inspects it and
/// clicks the canvas; requests are served on the UI thread between frames.
#[test]
fn control_channel_drives_the_app() {
    use std::io::{BufRead, BufReader, Write};

    let mut h = harness();
    let (rx, port) = crate::control::listen(0, || {}).expect("bind");
    h.state_mut().attach_control(rx);
    let client = std::thread::spawn(move || {
        let s = std::net::TcpStream::connect(("127.0.0.1", port)).expect("connect");
        let mut r = BufReader::new(s.try_clone().expect("clone"));
        let mut w = s;
        let mut call = |id: u64, line: String| -> serde_json::Value {
            writeln!(w, "{line}").expect("write");
            let mut out = String::new();
            r.read_line(&mut out).expect("read");
            let v: serde_json::Value = serde_json::from_str(&out).expect("json");
            assert_eq!(v["id"], id);
            v
        };
        let box_ = call(
            1,
            r#"{"id":1,"method":"engine.execute","params":{"command":"solid.box","params":{"size":[40,30,10]}}}"#
                .into(),
        );
        assert!(box_["result"].is_object(), "{box_}");
        let m = call(2, r#"{"id":2,"method":"model.inspect"}"#.into());
        let vol = m["result"]["bodies"][0]["volume"].as_f64().unwrap_or(0.0);
        assert!((vol - 12000.0).abs() < 1e-6, "{m}");
        // the top face centre, projected through the app's camera
        let click = call(3, r#"{"id":3,"method":"ui.click","params":{"x":20,"y":15,"z":10,"space":"world"}}"#.into());
        assert_eq!(click["result"]["hit"], true, "{click}");
        let bad = call(4, r#"{"id":4,"method":"ui.click","params":{"x":"nan","y":1}}"#.into());
        assert!(bad["error"].is_string(), "{bad}");
        let unknown = call(5, r#"{"id":5,"method":"teleport"}"#.into());
        assert!(unknown["error"].is_string());
        writeln!(w, "not json").expect("write");
        let mut out = String::new();
        r.read_line(&mut out).expect("read");
        assert!(out.contains("parse error"));
    });
    let start = std::time::Instant::now();
    while !client.is_finished() && start.elapsed() < std::time::Duration::from_secs(60) {
        h.step();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    client.join().expect("client assertions");
    assert!(h.state().selection_summary().starts_with("Selected: face"), "{}", h.state().selection_summary());
}
