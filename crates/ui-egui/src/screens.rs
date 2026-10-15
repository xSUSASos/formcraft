//! Offscreen screenshots of the whole UI (UI_SPEC.md §8): the real app drawn by egui_kittest
//! through wgpu, including the 3D viewport. Used by `formcraft-cli screenshot` and the
//! screenshot tests.

use egui::{Event, Pos2};
use egui_kittest::Harness;
use egui_kittest::wgpu::{WgpuTestRenderer, create_render_state, default_wgpu_setup};
use formcraft_render::camera::Camera;
use serde_json::{Value, json};

use crate::app::{AppOptions, FormCraftApp};
use crate::layout::Layout;
use crate::theme::ThemeKind;

/// The acceptance scenes of UI_SPEC.md §8: (id, description).
pub const SCENES: &[(&str, &str)] = &[
    ("1-empty", "Empty document: all zones, canvas gradient and grid"),
    ("2-selected", "Body selected: highlight, Browser expanded, tooltip"),
    ("3-extrude", "Active extrude: feature dialog, Join preview, manipulator"),
    ("4-sketch", "Sketch mode: constraint colours, dimensions, sketch palette, Finish Sketch"),
    ("5-marking", "Marking menu open"),
    ("6-shortcut-box", "Shortcut box with the AI line"),
    ("7-ai-group", "AI group on the timeline (+ Assistant panel)"),
    ("8-error", "Feature error on the timeline (+ toast)"),
];

const PLATE: &[(&str, &str)] = &[
    ("doc.parameter", r#"{"name": "L", "expression": 60}"#),
    ("sketch.create", r#"{"plane": "XY", "rectangles": [{"corner": [0, 0], "size": ["L", 40]}]}"#),
    ("solid.extrude", r#"{"distance": 5}"#),
    ("solid.fillet", r#"{"edges": "vertical", "radius": 3}"#),
    ("solid.hole", r#"{"points": [[10, 20, 5], [50, 20, 5]], "diameter": 6.6}"#),
];

fn run_steps(app: &mut FormCraftApp, steps: &[(&str, &str)]) -> Result<(), String> {
    for (cmd, p) in steps {
        let v: Value = serde_json::from_str(p).map_err(|e| e.to_string())?;
        app.session().execute(cmd, &v).map_err(|e| format!("{cmd}: {e}"))?;
    }
    Ok(())
}

fn options(scene: &str, theme: ThemeKind) -> AppOptions {
    let mut o = AppOptions { theme: Some(theme), fresh_layout: true, ..AppOptions::default() };
    match scene {
        "3-extrude" => o.dialog = Some("solid.extrude".into()),
        "4-sketch" => o.edit = Some("Sketch1".into()),
        "5-marking" => o.overlay = Some("marking".into()),
        "6-shortcut-box" => o.overlay = Some("search:flange 100 with 4 holes".into()),
        _ => {}
    }
    o
}

/// Render one scene to RGBA pixels. Errors (unknown scene, no GPU) are returned, not panics.
pub fn render(scene: &str, theme: ThemeKind, size: [u32; 2]) -> Result<image::RgbaImage, String> {
    if !SCENES.iter().any(|(s, _)| *s == scene) {
        let names: Vec<&str> = SCENES.iter().map(|(s, _)| *s).collect();
        return Err(format!("unknown scene `{scene}` ({})", names.join(", ")));
    }
    // kittest panics when there is no adapter; keep that inside.
    let rs =
        std::panic::catch_unwind(|| create_render_state(default_wgpu_setup(), egui_wgpu::RendererOptions::PREDICTABLE))
            .map_err(|_| "no GPU adapter for offscreen rendering".to_string())?;

    // The document is built before the app opens the dialog/sketch from its options.
    let mut pre = FormCraftApp::with_state(Layout::default(), Camera::home(), AppOptions::default());
    match scene {
        "1-empty" | "6-shortcut-box" => {}
        "3-extrude" => run_steps(&mut pre, PLATE.get(..2).unwrap_or_default())?,
        "7-ai-group" => {
            run_steps(&mut pre, PLATE.get(..1).unwrap_or_default())?;
            pre.session()
                .transaction("AI: Plate 60x40", Some("Plate 60x40, 2 x M6"), |s| {
                    for (cmd, p) in PLATE.get(1..).unwrap_or_default() {
                        let v: Value = serde_json::from_str(p).unwrap_or(json!({}));
                        s.execute(cmd, &v)?;
                    }
                    Ok(())
                })
                .map_err(|e| e.to_string())?;
        }
        "8-error" => {
            run_steps(&mut pre, PLATE)?;
            // a later edit of the sketch makes the fillet impossible → error on the timeline
            let id = pre.session().document().features.iter().find(|f| f.name == "Fillet1").map(|f| f.id.0);
            if let Some(id) = id {
                let _ = pre.session().execute("feature.suppress", &json!({"feature": id, "suppressed": false}));
            }
            pre.session().execute("doc.parameter", &json!({"name": "L", "expression": 4})).ok();
        }
        _ => run_steps(&mut pre, PLATE)?,
    }
    // re-create with the scene's options on top of the prepared document
    let mut app = FormCraftApp::with_state_and_session(
        Layout::default(),
        Camera::home(),
        options(scene, theme),
        pre.take_session(),
    );
    if scene != "4-sketch" {
        app.fit_view();
    }
    app.set_render_state(rs.clone());
    let mut h = Harness::builder()
        .with_size(egui::vec2(size[0] as f32, size[1] as f32))
        .with_pixels_per_point(1.0)
        .renderer(WgpuTestRenderer::from_render_state(rs))
        .build_eframe(move |_| app);
    h.run_steps(4);
    if scene == "2-selected" {
        let p = h.state_mut().control_call("ui.click", json!({"x": 30, "y": 30, "z": 5, "space": "world"}), 1.0);
        if let Ok(v) = p
            && let Some([x, y]) = v["canvasPoint"].as_array().map(|a| [a[0].as_f64(), a[1].as_f64()])
        {
            let r = h.state().canvas_rect();
            let at = Pos2::new(r.left() + x.unwrap_or(0.0) as f32, r.top() + y.unwrap_or(0.0) as f32 + 60.0);
            h.event(Event::PointerMoved(at));
        }
        h.run_steps(2);
        std::thread::sleep(std::time::Duration::from_millis(700));
        h.run_steps(2);
    }
    h.run_steps(3);
    h.render()
}
