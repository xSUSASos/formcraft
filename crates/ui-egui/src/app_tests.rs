//! Interaction tests: drive the whole shell with simulated input (no window, no GPU).

use egui::{Event, Modifiers, MouseWheelUnit, PointerButton, Pos2, TouchPhase, Vec2, pos2};
use egui_kittest::Harness;

use super::*;

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
