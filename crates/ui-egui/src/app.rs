//! The application shell: Fusion-style layout (PLAN.md §2).
//!
//! Application bar (top) → toolbar with tabs and groups → browser (left) → canvas with ViewCube
//! and navigation bar → timeline (bottom) → Text Commands (collapsible, bottom). Panels can be
//! closed, docked or floated, and reset; the layout is saved between runs.

use std::path::PathBuf;

use egui::{Align2, Color32, FontId, Id, Panel, Pos2, Rect, Sense, Stroke, Ui, Vec2, pos2};
use formcraft_engine::doc::{BodyId, FeatureId};
use formcraft_engine::{Pick, Session};
use formcraft_render::camera::{Camera, Projection};
use formcraft_render::grid::{self, LineKind};
use glam::{DVec2, DVec3};
use serde_json::Value;

use crate::dialog::{FeatureDialog, Outcome};
use crate::icons;
use crate::layout::{Dock, Layout, VisualStyle};
use crate::sketch_mode::{Request, SketchMode, Tool};
use crate::theme::{ThemeKind, Tokens};
use crate::timeline;
use crate::toolbar::{self, Action};
use crate::viewcube::{self, CubeAction};
use crate::viewport::Viewport;
use crate::{manipulator, menus};

const LAYOUT_KEY: &str = "formcraft.layout";
const CAMERA_KEY: &str = "formcraft.camera";

/// Options from the command line.
#[derive(Debug, Clone, Default)]
pub struct AppOptions {
    /// Save a PNG of the window after the first frames and exit.
    pub screenshot: Option<PathBuf>,
    /// Ignore the stored layout (tests, screenshots).
    pub fresh_layout: bool,
    /// Theme override.
    pub theme: Option<ThemeKind>,
    /// Start with the browser floating (screenshots of the layout).
    pub float_browser: bool,
    /// Start with Text Commands expanded.
    pub text_commands: bool,
    /// Start with a named view: top, front, right, home.
    pub view: Option<String>,
    /// Run a command script at startup (demos, screenshots).
    pub script: Option<PathBuf>,
    /// Open the dialog of this command at startup (screenshots).
    pub dialog: Option<String>,
    /// Edit this feature (by name) at startup: its dialog, or sketch mode for a sketch.
    pub edit: Option<String>,
    /// Open an overlay at startup (screenshots): marking, search, parameters.
    pub overlay: Option<String>,
}

/// Browser tree visibility toggles (Origin and bodies get real items in Phase 2).
#[derive(Debug, Clone)]
struct BrowserState {
    origin_visible: bool,
    planes: [bool; 3],
    axes: [bool; 3],
}

impl Default for BrowserState {
    fn default() -> Self {
        BrowserState { origin_visible: true, planes: [false; 3], axes: [false; 3] }
    }
}

/// Left-drag navigation mode chosen on the navigation bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NavMode {
    Select,
    Orbit,
    Pan,
    Zoom,
}

pub struct FormCraftApp {
    session: Session,
    layout: Layout,
    camera: Camera,
    tab: String,
    browser: BrowserState,
    nav: NavMode,
    status: String,
    command_line: String,
    doc_name: String,
    options: AppOptions,
    frame: u64,
    screenshot_requested: bool,
    /// Where the ViewCube was drawn last frame (tests, hit checks).
    cube_area: Rect,
    viewport: Viewport,
    /// Canvas rectangle of the last frame (tests).
    canvas_rect: Rect,
    dialog: Option<FeatureDialog>,
    /// Live preview while the dialog is open: (model, key).
    preview: Option<(formcraft_engine::regen::Model, u64)>,
    preview_counter: u64,
    rename: Option<(FeatureId, String)>,
    sketch_mode: Option<SketchMode>,
    plane_picker: bool,
    marking: Option<Pos2>,
    /// The marking menu opened this frame (its own click must not close it).
    marking_fresh: bool,
    search: Option<(String, Pos2)>,
    params_rows: Option<Vec<menus::ParamRow>>,
    last_command: Option<String>,
}

impl FormCraftApp {
    pub fn new(cc: &eframe::CreationContext<'_>, options: AppOptions) -> Self {
        let mut layout = Layout::default();
        let mut camera = Camera::home();
        if !options.fresh_layout
            && let Some(storage) = cc.storage
        {
            if let Some(l) = eframe::get_value::<Layout>(storage, LAYOUT_KEY) {
                layout = l;
            }
            if let Some(c) = eframe::get_value::<Camera>(storage, CAMERA_KEY).filter(|c| c.distance.is_finite()) {
                camera = c;
            }
        }
        layout.sanitize();
        if let Some(t) = options.theme {
            layout.theme = t;
        }
        let mut app = Self::with_state(layout, camera, options);
        app.viewport = Viewport::new(cc.wgpu_render_state.clone());
        app
    }

    /// App without eframe storage (tests and headless use).
    pub fn with_state(mut layout: Layout, camera: Camera, options: AppOptions) -> Self {
        if options.float_browser {
            layout.browser.dock = Dock::Floating;
        }
        if options.text_commands {
            layout.text_commands.open = true;
        }
        let view = options.view.clone();
        let mut app = FormCraftApp {
            session: Session::new(),
            layout,
            camera,
            tab: "Solid".into(),
            browser: BrowserState::default(),
            nav: NavMode::Select,
            status: "Ready".into(),
            command_line: String::new(),
            doc_name: "Untitled".into(),
            options,
            frame: 0,
            screenshot_requested: false,
            cube_area: Rect::NOTHING,
            viewport: Viewport::new(None),
            canvas_rect: Rect::NOTHING,
            dialog: None,
            preview: None,
            preview_counter: 0,
            rename: None,
            sketch_mode: None,
            plane_picker: false,
            marking: None,
            marking_fresh: false,
            search: None,
            params_rows: None,
            last_command: None,
        };
        if let Some(path) = app.options.script.clone() {
            app.run_script_file(&path);
            if view.is_none() {
                app.fit();
            }
        }
        if let Some(v) = view {
            app.apply_named_view(&v.to_uppercase());
            app.fit_keep_direction();
        }
        if let Some(d) = app.options.dialog.clone() {
            app.open_dialog(&d, &Value::Null, None);
        }
        match app.options.overlay.as_deref() {
            Some("marking") => app.marking = Some(Pos2::new(760.0, 420.0)),
            Some("search") => app.search = Some(("fil".into(), Pos2::new(700.0, 300.0))),
            Some("parameters") => app.params_rows = Some(menus::rows_from(&app.session)),
            _ => {}
        }
        if let Some(name) = app.options.edit.clone() {
            match app.session.document().features.iter().find(|f| f.name == name).map(|f| f.id) {
                Some(id) => app.edit_feature(id),
                None => app.status = format!("no feature named {name}"),
            }
        }
        app
    }

    fn run_script_file(&mut self, path: &std::path::Path) {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                self.status = format!("{}: {e}", path.display());
                return;
            }
        };
        self.status = match formcraft_engine::script::parse(&text)
            .map_err(|e| (0, e))
            .and_then(|steps| formcraft_engine::script::run(&mut self.session, &steps))
        {
            Ok(r) => format!("Script: {} steps", r.len()),
            Err((i, e)) => format!("Script step {}: {e}", i + 1),
        };
    }

    /// Fit without changing the view direction (used after a named view at startup).
    fn fit_keep_direction(&mut self) {
        if !self.session.model().bodies.is_empty() {
            self.fit();
        }
    }

    pub fn session(&mut self) -> &mut Session {
        &mut self.session
    }

    fn tokens(&self) -> Tokens {
        Tokens::get(self.layout.theme)
    }

    /// Run an engine command (or a UI-level one) and report the outcome in the status line.
    fn run(&mut self, id: &str, params: Value) {
        match id {
            "tools.text_commands" => {
                self.layout.text_commands.open = !self.layout.text_commands.open;
                return;
            }
            "doc.parameters" if params.is_null() => {
                self.params_rows = Some(menus::rows_from(&self.session));
                return;
            }
            "tools.settings" => {
                self.status = "Preferences: theme and layout are in the View menu".into();
                return;
            }
            _ => {}
        }
        if params.is_null() && self.sketch_command(id) {
            return;
        }
        // Toolbar/menu invocation of a modelling command with parameters opens its dialog.
        if params.is_null() && self.wants_dialog(id) {
            self.last_command = Some(id.to_string());
            self.open_dialog(id, &serde_json::json!({}), None);
            return;
        }
        self.status = match self.session.execute(id, &params) {
            Ok(v) => {
                v.get("message").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| format!("{id}: done"))
            }
            Err(e) => format!("{id}: {e}"),
        };
    }

    /// Sketch environment commands from the toolbar and shortcuts. Returns true when handled.
    fn sketch_command(&mut self, id: &str) -> bool {
        match id {
            "sketch.create" => {
                self.plane_picker = true;
                true
            }
            "sketch.finish" => {
                self.finish_sketch();
                true
            }
            _ if id.starts_with("sketch.") && id != "sketch.edit" && id != "sketch.get" => {
                match (&mut self.sketch_mode, Tool::from_command(id)) {
                    (Some(m), Some(tool)) => {
                        m.set_tool(tool);
                        self.status = format!("Sketch: {id}");
                    }
                    (Some(_), None) => {
                        self.status = "Select sketch entities, then pick the constraint in the Sketch Palette".into()
                    }
                    (None, _) => {
                        self.status = "Create a sketch first (Create Sketch, then pick a plane)".into();
                        self.plane_picker = true;
                    }
                }
                true
            }
            _ => false,
        }
    }

    fn enter_sketch(&mut self, id: FeatureId) {
        if let Some(r) = self.session.model().sketch(id) {
            SketchMode::look_at(&r.plane, &mut self.camera);
            self.camera.projection = Projection::Orthographic;
            // Frame the sketch (or 100 mm around its origin when empty).
            let mut bb = formcraft_kernel::Aabb::empty();
            for (_, e) in &r.sketch.entities {
                match e {
                    formcraft_sketch::Entity::Point { x, y } => {
                        bb.grow(r.plane.to_world(formcraft_kernel::DVec2::new(*x, *y)))
                    }
                    formcraft_sketch::Entity::Circle { center, radius, .. } => {
                        if let Ok((x, y)) = r.sketch.point(*center) {
                            for (dx, dy) in [(-1.0, -1.0), (1.0, 1.0)] {
                                bb.grow(
                                    r.plane.to_world(formcraft_kernel::DVec2::new(x + dx * radius, y + dy * radius)),
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
            if bb.is_empty() || bb.size().length() < 1e-6 {
                bb = formcraft_kernel::Aabb {
                    min: r.plane.origin - DVec3::splat(50.0),
                    max: r.plane.origin + DVec3::splat(50.0),
                };
            }
            let pad = bb.size() * 0.25;
            self.camera.fit(bb.min - pad, bb.max + pad, 1.6);
        }
        self.sketch_mode = Some(SketchMode::new(id));
        self.tab = "Sketch".into();
        self.session.selection_mut().clear();
        self.status = "Sketch: L line, R rectangle, C circle, D dimension, Esc select".into();
    }

    fn finish_sketch(&mut self) {
        if self.sketch_mode.take().is_some() {
            self.tab = "Solid".into();
            self.camera.projection = Projection::Perspective;
            self.camera.look_from(DVec3::new(1.0, -1.0, 1.0));
            self.status = "Sketch finished".into();
        }
    }

    fn create_sketch(&mut self, plane: Value) {
        match self.session.execute("sketch.create", &serde_json::json!({"plane": plane})) {
            Ok(v) => {
                if let Some(id) = v.get("feature").and_then(Value::as_u64) {
                    self.enter_sketch(FeatureId(id as u32));
                }
            }
            Err(e) => self.status = format!("Create Sketch: {e}"),
        }
    }

    fn plane_picker_ui(&mut self, ui: &mut Ui) {
        if !self.plane_picker {
            return;
        }
        let face = self.session.selection().items.iter().find_map(|p| match p {
            Pick::Face { body, index } => Some((*body, *index)),
            _ => None,
        });
        let mut choice: Option<Value> = None;
        let mut open = true;
        egui::Window::new("Create Sketch")
            .id(Id::new("plane_picker"))
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::RIGHT_TOP, Vec2::new(-12.0, 300.0))
            .open(&mut open)
            .show(ui.ctx(), |ui| {
                ui.label("Select a plane:");
                for (name, tip) in [("XY", "Top (ground)"), ("XZ", "Front"), ("YZ", "Right")] {
                    if ui.button(format!("{name} plane")).on_hover_text(tip).clicked() {
                        choice = Some(serde_json::json!(name));
                    }
                }
                let label = if face.is_some() { "Selected face" } else { "Selected face (select a planar face first)" };
                if ui.add_enabled(face.is_some(), egui::Button::new(label)).clicked()
                    && let Some((body, index)) = face
                    && let Some(b) = self.session.model().body(body)
                    && let Some(fi) = self.session.kernel().faces(&b.shape).ok().and_then(|v| v.into_iter().find(|f| f.index == index))
                {
                    choice = Some(serde_json::json!({"body": body.0, "face": {"face": formcraft_engine::regen::select::face_signature(&fi)}}));
                }
            });
        if let Some(c) = choice {
            self.plane_picker = false;
            self.create_sketch(c);
        } else if !open {
            self.plane_picker = false;
        }
    }

    fn sketch_palette(&mut self, ui: &mut Ui, t: &Tokens) {
        let Some(mut m) = self.sketch_mode.take() else { return };
        let mut req = Request::None;
        egui::Window::new("Sketch Palette")
            .id(Id::new("sketch_palette"))
            .title_bar(false)
            .anchor(Align2::RIGHT_TOP, Vec2::new(-12.0, 300.0))
            .resizable(false)
            .show(ui.ctx(), |ui| req = m.palette(ui, &self.session, t));
        self.sketch_mode = Some(m);
        self.handle_sketch_request(req);
    }

    fn handle_sketch_request(&mut self, req: Request) {
        match req {
            Request::None => {}
            Request::Finish => self.finish_sketch(),
            Request::Edit(ops) => {
                let Some(id) = self.sketch_mode.as_ref().map(|m| m.sketch) else { return };
                match self.session.execute("sketch.edit", &serde_json::json!({"sketch": id.0, "ops": ops})) {
                    Ok(v) => {
                        self.status = v.get("message").and_then(Value::as_str).unwrap_or("ok").to_string();
                        if let Some(m) = self.sketch_mode.as_mut() {
                            m.after_edit(&v);
                            m.message = None;
                        }
                    }
                    Err(e) => {
                        self.status = e.to_string();
                        if let Some(m) = self.sketch_mode.as_mut() {
                            m.message = Some(e.to_string());
                        }
                    }
                }
            }
        }
    }

    fn overlays(&mut self, ui: &mut Ui, t: &Tokens) {
        if let Some(at) = self.marking {
            match menus::marking_menu(ui, t, &self.session, at, self.last_command.as_deref()) {
                Some(menus::MenuAction::Run(id)) => {
                    self.marking = None;
                    self.run(&id, Value::Null);
                }
                Some(menus::MenuAction::Close) if !self.marking_fresh => self.marking = None,
                Some(menus::MenuAction::Close) => {}
                None => {}
            }
        }
        self.marking_fresh = false;
        if let Some((mut text, at)) = self.search.take() {
            match menus::shortcut_box(ui, t, &self.session, &mut text, at) {
                Some(menus::BoxAction::Run(id)) => self.run(&id, Value::Null),
                Some(menus::BoxAction::Ai(prompt)) => self.run("ai.generate", serde_json::json!({"prompt": prompt})),
                Some(menus::BoxAction::Close) => {}
                None => self.search = Some((text, at)),
            }
        }
        if let Some(mut rows) = self.params_rows.take() {
            let mut open = true;
            let cmds = menus::parameters_window(ui, t, &mut rows, &mut open);
            let changed = !cmds.is_empty();
            for (id, p) in cmds {
                self.run(&id, p);
            }
            if open {
                // Refresh values after a change (keeps rows that are still being typed).
                if changed {
                    let fresh = menus::rows_from(&self.session);
                    rows.retain(|r| !r.existing);
                    rows.retain(|r| !fresh.iter().any(|f| f.name == r.name.trim()));
                    let mut all = fresh;
                    all.extend(rows);
                    rows = all;
                }
                self.params_rows = Some(rows);
            }
        }
    }

    /// Hotkeys from the command registry (E, F, H, Q, Ctrl+S…), S for the shortcut box.
    fn registry_shortcuts(&mut self, ui: &Ui) {
        if ui.ctx().egui_wants_keyboard_input() || self.search.is_some() {
            return;
        }
        if ui.input(|i| i.key_pressed(egui::Key::S) && i.modifiers.is_none()) {
            let at =
                ui.input(|i| i.pointer.hover_pos()).unwrap_or(ui.ctx().content_rect().center()) + Vec2::new(10.0, 10.0);
            self.search = Some((String::new(), at));
            return;
        }
        let in_sketch = self.sketch_mode.is_some();
        let bindings: Vec<(String, String)> =
            self.session.registry().specs().filter_map(|c| c.shortcut.clone().map(|k| (k, c.id.clone()))).collect();
        for (key, id) in bindings {
            let Some((mods, k)) = menus::parse_shortcut(&key) else { continue };
            // In sketch mode the plain letters belong to the sketch tools.
            if in_sketch && mods.is_none() {
                continue;
            }
            if id == "doc.undo" || id == "doc.redo" {
                continue; // handled with Ctrl+Y / Ctrl+Shift+Z
            }
            if ui.input_mut(|i| i.consume_key(mods, k)) {
                if id == "doc.save" || id == "doc.open" {
                    self.file_action(&id);
                } else {
                    self.run(&id, Value::Null);
                }
            }
        }
    }

    fn wants_dialog(&self, id: &str) -> bool {
        self.session.registry().get(id).is_some_and(|c| {
            c.spec.undoable && c.spec.params.get("properties").and_then(Value::as_object).is_some_and(|p| !p.is_empty())
        })
    }

    fn open_dialog(&mut self, id: &str, initial: &Value, editing: Option<FeatureId>) {
        self.dialog = FeatureDialog::new(&self.session, id, initial, editing);
        self.preview = None;
        if self.dialog.is_none() {
            self.status = format!("{id}: not available");
        }
    }

    /// Edit a timeline feature: its dialog, prefilled.
    fn edit_feature(&mut self, id: FeatureId) {
        match self.session.execute("feature.params", &serde_json::json!({"feature": id.0})) {
            Ok(v) => match v.get("command").and_then(Value::as_str) {
                Some(cmd) => {
                    let cmd = cmd.to_string();
                    let params = v.get("params").cloned().unwrap_or(Value::Null);
                    self.open_dialog(&cmd, &params, Some(id));
                }
                None => {
                    if matches!(
                        self.session.document().feature(id).map(|f| &f.kind),
                        Some(formcraft_engine::doc::FeatureKind::Sketch { .. })
                    ) {
                        self.enter_sketch(id);
                    } else {
                        self.status = "This feature cannot be edited".into();
                    }
                }
            },
            Err(e) => self.status = e.to_string(),
        }
    }

    fn feature_dialog(&mut self, ui: &mut Ui, t: &Tokens) {
        let Some(mut dlg) = self.dialog.take() else { return };
        let mut outcome = Outcome::None;
        let mut open = true;
        egui::Window::new(dlg.title.clone())
            .id(Id::new("feature_dialog"))
            .anchor(Align2::RIGHT_TOP, Vec2::new(-12.0, 300.0))
            .resizable(false)
            .collapsible(false)
            .open(&mut open)
            .show(ui.ctx(), |ui| outcome = dlg.ui(ui, &self.session, t));
        // Live preview when the parameters changed.
        if let Ok((cmd, params)) = dlg.invocation()
            && dlg.previewed.as_ref() != Some(&params)
        {
            dlg.previewed = Some(params.clone());
            match self.session.preview(&cmd, &params) {
                Ok(m) => {
                    self.preview_counter += 1;
                    self.preview = Some((m, u64::MAX - self.preview_counter));
                    dlg.error = None;
                }
                Err(e) => {
                    self.preview = None;
                    dlg.error = Some(e.to_string());
                }
            }
        }
        match outcome {
            Outcome::Ok => match dlg.invocation() {
                Ok((cmd, params)) => match self.session.execute(&cmd, &params) {
                    Ok(v) => {
                        self.status = v.get("message").and_then(Value::as_str).unwrap_or("Done").to_string();
                        self.preview = None;
                        return;
                    }
                    Err(e) => dlg.error = Some(e.to_string()),
                },
                Err(e) => dlg.error = Some(e),
            },
            Outcome::Cancel => {
                self.preview = None;
                return;
            }
            Outcome::None => {}
        }
        if !open {
            self.preview = None;
            return;
        }
        self.dialog = Some(dlg);
    }

    fn rename_dialog(&mut self, ui: &mut Ui) {
        let Some((id, mut name)) = self.rename.take() else { return };
        let mut done = None;
        egui::Window::new("Rename")
            .id(Id::new("rename_dialog"))
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ui.ctx(), |ui| {
                let r = ui.text_edit_singleline(&mut name);
                r.request_focus();
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        done = Some(true);
                    }
                    if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        done = Some(false);
                    }
                });
            });
        match done {
            Some(true) => self.run("feature.rename", serde_json::json!({"feature": id.0, "name": name})),
            Some(false) => {}
            None => self.rename = Some((id, name)),
        }
    }

    // ------------------------------------------------------------------ application bar

    fn app_bar(&mut self, ui: &mut Ui, t: &Tokens) {
        Panel::top("app_bar")
            .exact_size(30.0)
            .frame(egui::Frame::new().fill(t.app_bar).inner_margin(egui::Margin::symmetric(8, 4)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(egui::RichText::new("FormCraft").strong().color(t.accent));
                    ui.separator();
                    ui.menu_button("File", |ui| {
                        for (label, id) in [
                            ("New Design", "doc.new"),
                            ("Open…", "doc.open"),
                            ("Save", "doc.save"),
                            ("Export…", "doc.export"),
                        ] {
                            let on = self.session.registry().get(id).is_some();
                            if ui.add_enabled(on, egui::Button::new(label)).clicked() {
                                ui.close();
                                self.file_action(id);
                            }
                        }
                    });
                    let undo_on = self.session.can_undo();
                    if icons::button(ui, t, "undo", "Undo (Ctrl+Z)", 20.0, undo_on, false).clicked() {
                        self.run("doc.undo", Value::Null);
                    }
                    let redo_on = self.session.can_redo();
                    if icons::button(ui, t, "redo", "Redo (Ctrl+Y)", 20.0, redo_on, false).clicked() {
                        self.run("doc.redo", Value::Null);
                    }
                    ui.menu_button("View", |ui| self.view_menu(ui));
                    ui.menu_button("Help", |ui| {
                        if ui.button("About FormCraft").clicked() {
                            self.run("app.about", Value::Null);
                            ui.close();
                        }
                    });
                    // Document name in the middle.
                    let rect = ui.max_rect();
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        &self.doc_name,
                        FontId::proportional(13.0),
                        t.text,
                    );
                });
            });
    }

    /// File menu actions that need a file dialog first (programmatic calls never open one).
    fn file_action(&mut self, id: &str) {
        match id {
            "doc.export" => {
                let picked = rfd::FileDialog::new()
                    .set_title("Export")
                    .add_filter("STEP", &["step", "stp"])
                    .add_filter("STL", &["stl"])
                    .set_file_name(format!("{}.step", self.session.document().name))
                    .save_file();
                if let Some(path) = picked {
                    self.run(id, serde_json::json!({"path": path.to_string_lossy()}));
                }
            }
            "doc.new" => {
                self.run(id, Value::Null);
                self.doc_name = self.session.document().name.clone();
            }
            "doc.save" => {
                if self.session.file().is_some() {
                    self.run(id, serde_json::json!({}));
                } else if let Some(path) = rfd::FileDialog::new()
                    .set_title("Save")
                    .add_filter("FormCraft design", &["fcraft"])
                    .set_file_name(format!("{}.fcraft", self.session.document().name))
                    .save_file()
                {
                    self.run(id, serde_json::json!({"path": path.to_string_lossy()}));
                }
            }
            "doc.open" => {
                if let Some(path) =
                    rfd::FileDialog::new().set_title("Open").add_filter("FormCraft design", &["fcraft"]).pick_file()
                {
                    self.run(id, serde_json::json!({"path": path.to_string_lossy()}));
                    self.doc_name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    self.fit();
                }
            }
            other => self.run(other, Value::Null),
        }
    }

    fn view_menu(&mut self, ui: &mut Ui) {
        let l = &mut self.layout;
        ui.checkbox(&mut l.browser.open, "Browser");
        ui.checkbox(&mut l.timeline.open, "Timeline");
        ui.checkbox(&mut l.text_commands.open, "Text Commands");
        ui.checkbox(&mut l.view_cube, "ViewCube");
        ui.checkbox(&mut l.nav_bar, "Navigation Bar");
        ui.separator();
        let mut floating = l.browser.dock == Dock::Floating;
        if ui.checkbox(&mut floating, "Float Browser").changed() {
            l.browser.dock = if floating { Dock::Floating } else { Dock::Docked };
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Theme");
            ui.selectable_value(&mut l.theme, ThemeKind::Dark, "Dark");
            ui.selectable_value(&mut l.theme, ThemeKind::Light, "Light");
        });
        if ui.button("Reset Layout").clicked() {
            l.reset();
            self.status = "Layout reset".into();
            ui.close();
        }
    }

    // ------------------------------------------------------------------ toolbar

    fn toolbar(&mut self, ui: &mut Ui, t: &Tokens) {
        let mut action = None;
        Panel::top("toolbar")
            .frame(egui::Frame::new().fill(t.toolbar).inner_margin(egui::Margin::symmetric(8, 4)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("workspace")
                        .selected_text(egui::RichText::new("DESIGN").strong())
                        .width(90.0)
                        .show_ui(ui, |ui| {
                            let _ = ui.selectable_label(true, "Design");
                        });
                    ui.add_space(8.0);
                    toolbar::tabs(ui, t, &mut self.tab);
                });
                ui.add_space(2.0);
                action = toolbar::groups_ui(ui, t, self.session.registry(), &self.tab);
            });
        if let Some(Action::Run(id)) = action {
            self.run(&id, Value::Null);
        }
    }

    // ------------------------------------------------------------------ browser

    fn browser_contents(&mut self, ui: &mut Ui, t: &Tokens) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("BROWSER").size(11.0).strong().color(t.text_dim));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icons::button(ui, t, "close", "Close", 16.0, true, false).clicked() {
                    self.layout.browser.open = false;
                }
                let dock = self.layout.browser.dock;
                let (icon, tip) = if dock == Dock::Docked { ("float", "Float") } else { ("dock", "Dock") };
                if icons::button(ui, t, icon, tip, 16.0, true, false).clicked() {
                    self.layout.browser.dock = if dock == Dock::Docked { Dock::Floating } else { Dock::Docked };
                }
            });
        });
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::CollapsingHeader::new(self.doc_name.clone()).default_open(true).show(ui, |ui| {
                egui::CollapsingHeader::new("Document Settings").default_open(false).show(ui, |ui| {
                    ui.label("Units: mm");
                });
                egui::CollapsingHeader::new("Named Views").default_open(false).show(ui, |ui| {
                    for v in ["TOP", "FRONT", "RIGHT", "HOME"] {
                        if ui.selectable_label(false, v).clicked() {
                            self.apply_named_view(v);
                        }
                    }
                });
                let b = &mut self.browser;
                eye_header(ui, t, "Origin", &mut b.origin_visible, |ui| {
                    for (i, name) in ["XY", "XZ", "YZ"].iter().enumerate() {
                        if let Some(v) = b.planes.get_mut(i) {
                            eye_row(ui, t, name, v);
                        }
                    }
                    for (i, name) in ["X", "Y", "Z"].iter().enumerate() {
                        if let Some(v) = b.axes.get_mut(i) {
                            eye_row(ui, t, name, v);
                        }
                    }
                });
                egui::CollapsingHeader::new("Sketches").default_open(true).show(ui, |ui| {
                    let sketches: Vec<(FeatureId, String)> = self
                        .session
                        .document()
                        .features
                        .iter()
                        .filter(|f| matches!(f.kind, formcraft_engine::doc::FeatureKind::Sketch { .. }))
                        .map(|f| (f.id, f.name.clone()))
                        .collect();
                    if sketches.is_empty() {
                        ui.label(egui::RichText::new("(none)").color(t.text_dim));
                    }
                    for (id, name) in sketches {
                        ui.horizontal(|ui| {
                            let (r, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                            crate::icons::paint(ui.painter(), r, "sketch", t.text, t);
                            let dof = self.session.model().sketch(id).map(|s| s.dof);
                            let resp = ui.selectable_label(false, &name);
                            let resp = match dof {
                                Some(0) => resp.on_hover_text("Fully constrained"),
                                Some(n) => resp.on_hover_text(format!("{n} degrees of freedom")),
                                None => resp.on_hover_text("Failed to compute"),
                            };
                            if resp.double_clicked() {
                                self.edit_feature(id);
                            }
                        });
                    }
                });
                egui::CollapsingHeader::new("Bodies").default_open(true).show(ui, |ui| {
                    let bodies: Vec<(BodyId, String, bool)> =
                        self.session.model().bodies.iter().map(|b| (b.id, b.name.clone(), b.visible)).collect();
                    if bodies.is_empty() {
                        ui.label(egui::RichText::new("(none)").color(t.text_dim));
                    }
                    for (id, name, visible) in bodies {
                        ui.horizontal(|ui| {
                            let mut v = visible;
                            eye_toggle(ui, t, &mut v);
                            if v != visible {
                                self.run("body.visibility", serde_json::json!({"body": id.0, "visible": v}));
                            }
                            let pick = Pick::Body { body: id };
                            let selected = self.session.selection().contains(&pick);
                            let r = ui.selectable_label(selected, &name);
                            if r.clicked() {
                                let multi = ui.input(|i| i.modifiers.ctrl || i.modifiers.shift);
                                let sel = self.session.selection_mut();
                                if multi { sel.toggle(pick) } else { sel.set(pick) }
                            }
                            if r.hovered() {
                                self.session.selection_mut().hover = Some(pick);
                            }
                        });
                    }
                });
            });
        });
    }

    fn apply_named_view(&mut self, v: &str) {
        match v {
            "TOP" => self.camera.look_from(DVec3::Z),
            "FRONT" => self.camera.look_from(DVec3::NEG_Y),
            "RIGHT" => self.camera.look_from(DVec3::X),
            _ => self.camera = Camera { projection: self.camera.projection, ..Camera::home() },
        }
    }

    fn browser(&mut self, ui: &mut Ui, t: &Tokens) {
        if !self.layout.browser.open {
            return;
        }
        match self.layout.browser.dock {
            Dock::Docked => {
                let r = Panel::left("browser")
                    .resizable(true)
                    .default_size(self.layout.browser.size)
                    .size_range(150.0..=700.0)
                    .frame(egui::Frame::new().fill(t.panel).inner_margin(6))
                    .show(ui, |ui| self.browser_contents(ui, t));
                self.layout.browser.size = r.response.rect.width();
            }
            Dock::Floating => {
                let mut open = true;
                egui::Window::new("Browser")
                    .id(Id::new("browser_window"))
                    .title_bar(false)
                    .default_pos(pos2(20.0, 120.0))
                    .default_width(self.layout.browser.size)
                    .resizable(true)
                    .open(&mut open)
                    .show(ui.ctx(), |ui| self.browser_contents(ui, t));
                if !open {
                    self.layout.browser.open = false;
                }
            }
        }
    }

    // ------------------------------------------------------------------ timeline and text commands

    fn timeline(&mut self, ui: &mut Ui, t: &Tokens) {
        if !self.layout.timeline.open {
            return;
        }
        let mut action = None;
        Panel::bottom("timeline")
            .exact_size(self.layout.timeline.size)
            .frame(egui::Frame::new().fill(t.panel).inner_margin(egui::Margin::symmetric(8, 6)))
            .show(ui, |ui| action = timeline::ui(ui, t, &self.session));
        let Some(a) = action else { return };
        use serde_json::json;
        match a {
            timeline::Action::Edit(id) => self.edit_feature(id),
            timeline::Action::Rename(id) => {
                let name = self.session.document().feature(id).map(|f| f.name.clone()).unwrap_or_default();
                self.rename = Some((id, name));
            }
            timeline::Action::Suppress(id, on) => {
                self.run("feature.suppress", json!({"feature": id.0, "suppressed": on}))
            }
            timeline::Action::Delete(id) => self.run("feature.delete", json!({"feature": id.0})),
            timeline::Action::Reorder(id, to) => self.run("timeline.reorder", json!({"feature": id.0, "index": to})),
            timeline::Action::Marker(m) => self.run("timeline.marker", json!({"index": m})),
            timeline::Action::Explain(id) => {
                let r = if self.session.registry().get("ai.explain").is_some() {
                    self.session.execute("ai.explain", &json!({"feature": id.0}))
                } else {
                    self.session.execute("feature.params", &json!({"feature": id.0}))
                };
                self.status = match r {
                    Ok(v) => {
                        v.get("message").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| v.to_string())
                    }
                    Err(e) => e.to_string(),
                };
            }
        }
    }

    fn text_commands(&mut self, ui: &mut Ui, t: &Tokens) {
        if !self.layout.text_commands.open {
            // Collapsed: a thin strip that expands on click (like Fusion's).
            Panel::bottom("text_commands_bar")
                .exact_size(22.0)
                .frame(egui::Frame::new().fill(t.panel_header).inner_margin(egui::Margin::symmetric(6, 2)))
                .show(ui, |ui| {
                    ui.horizontal_centered(|ui| {
                        if icons::button(ui, t, "expand", "Show Text Commands", 16.0, true, false).clicked() {
                            self.layout.text_commands.open = true;
                        }
                        let r = ui.add(
                            egui::Label::new(
                                egui::RichText::new("TEXT COMMANDS").size(10.5).strong().color(t.text_dim),
                            )
                            .sense(Sense::click()),
                        );
                        if r.clicked() {
                            self.layout.text_commands.open = true;
                        }
                    });
                });
            return;
        }
        let mut open = self.layout.text_commands.open;
        let mut submit = None;
        let r = Panel::bottom("text_commands")
            .resizable(true)
            .default_size(self.layout.text_commands.size)
            .size_range(80.0..=600.0)
            .frame(egui::Frame::new().fill(t.panel).inner_margin(6))
            .show_collapsible(ui, &mut open, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("TEXT COMMANDS").size(11.0).strong().color(t.text_dim));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icons::button(ui, t, "dropdown", "Collapse", 16.0, true, false).clicked() {
                            self.layout.text_commands.open = false;
                        }
                    });
                });
                let input_h = 26.0;
                egui::ScrollArea::vertical().max_height(ui.available_height() - input_h).stick_to_bottom(true).show(
                    ui,
                    |ui| {
                        for e in self.session.log() {
                            let c = if e.ok { t.text } else { t.error };
                            ui.label(
                                egui::RichText::new(format!("{} {} → {}", e.command, e.params, e.message))
                                    .monospace()
                                    .size(11.5)
                                    .color(c),
                            );
                        }
                    },
                );
                ui.horizontal(|ui| {
                    ui.label(">");
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.command_line)
                            .hint_text("command.id {\"param\": value}")
                            .desired_width(f32::INFINITY)
                            .font(egui::TextStyle::Monospace),
                    );
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        submit = Some(std::mem::take(&mut self.command_line));
                        resp.request_focus();
                    }
                });
            });
        if let Some(r) = r {
            self.layout.text_commands.size = r.response.rect.height();
        }
        self.layout.text_commands.open = open;
        if let Some(line) = submit {
            self.submit_command_line(&line);
        }
    }

    /// `id` or `id {json}`.
    fn submit_command_line(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        let (id, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let params = if rest.trim().is_empty() {
            Ok(Value::Null)
        } else {
            serde_json::from_str::<Value>(rest.trim()).map_err(|e| e.to_string())
        };
        match params {
            Ok(p) => self.run(id, p),
            Err(e) => {
                self.status = format!("bad JSON parameters: {e}");
                self.session.push_log(formcraft_engine::LogEntry {
                    command: id.into(),
                    params: Value::Null,
                    ok: false,
                    message: self.status.clone(),
                });
            }
        }
    }

    // ------------------------------------------------------------------ canvas

    fn canvas(&mut self, ui: &mut Ui, t: &Tokens) {
        let rect = ui.max_rect();
        self.canvas_rect = rect;
        let resp = ui.interact(rect, Id::new("canvas"), Sense::click_and_drag());
        let size = DVec2::new(rect.width() as f64, rect.height() as f64);
        let painter = ui.painter_at(rect);

        // Background gradient.
        let mut mesh = egui::Mesh::default();
        mesh.colored_vertex(rect.left_top(), t.canvas_top);
        mesh.colored_vertex(rect.right_top(), t.canvas_top);
        mesh.colored_vertex(rect.right_bottom(), t.canvas_bottom);
        mesh.colored_vertex(rect.left_bottom(), t.canvas_bottom);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        painter.add(egui::Shape::mesh(mesh));

        self.navigate(ui, &resp, rect, size);

        let shown = self.preview.as_ref().map(|(m, k)| (m, *k));
        // Bodies, grid and axes on the GPU (with depth); without a GPU (tests) the grid is
        // painted in 2D below.
        let gpu = self.viewport.paint(
            ui,
            rect,
            &self.session,
            shown,
            &self.camera,
            t,
            self.layout.grid,
            self.layout.visual_style,
        );
        self.manipulators(ui, rect, t);
        if resp.secondary_clicked() && self.sketch_mode.as_ref().is_none_or(|m| m.idle()) {
            self.marking = resp.interact_pointer_pos();
            self.marking_fresh = true;
        }
        if let Some(mut m) = self.sketch_mode.take() {
            let req = m.ui(ui, &resp, rect, &self.session, &self.camera, t);
            self.sketch_mode = Some(m);
            self.handle_sketch_request(req);
        } else {
            self.handle_picking(ui, &resp, rect, size);
        }

        let origin = rect.min.to_vec2();
        let to_pos = |p: DVec2| Pos2::new(p.x as f32, p.y as f32) + origin;
        if !gpu && self.layout.grid {
            for (a, b, kind) in grid::lines(&self.camera) {
                let (color, w) = match kind {
                    LineKind::Minor => (t.grid_minor, 1.0),
                    LineKind::Major => (t.grid_major, 1.0),
                    LineKind::AxisX => (t.axis_x, 1.5),
                    LineKind::AxisY => (t.axis_y, 1.5),
                };
                if let Some((p, q)) = grid::project_segment(&self.camera, a, b, size) {
                    painter.line_segment([to_pos(p), to_pos(q)], Stroke::new(w, color));
                }
            }
        }
        if self.browser.origin_visible {
            let len = grid::spacing(&self.camera) * 10.0;
            if !gpu && let Some((p, q)) = grid::project_segment(&self.camera, DVec3::ZERO, DVec3::Z * len, size) {
                painter.line_segment([to_pos(p), to_pos(q)], Stroke::new(1.5, t.axis_z));
            }
            if let Some(o) = self.camera.project(DVec3::ZERO, size) {
                painter.circle_filled(to_pos(o), 3.0, t.text_dim);
            }
            self.paint_origin_items(&painter, t, size, to_pos);
        }

        if self.layout.view_cube {
            let area = Rect::from_min_size(pos2(rect.right() - 150.0, rect.top() + 8.0), Vec2::splat(140.0));
            self.cube_area = area;
            match viewcube::show(ui, area, &self.camera, t) {
                Some(CubeAction::View(v)) => {
                    self.camera.look_from(v.dir());
                    self.status = format!("View: {}", v.label().unwrap_or("custom"));
                }
                Some(CubeAction::Home) => self.apply_named_view("HOME"),
                None => {}
            }
        }
        if self.layout.nav_bar {
            self.nav_bar(ui, t, rect);
        }

        // Status line, bottom-left of the canvas.
        painter.text(
            rect.left_bottom() + Vec2::new(10.0, -10.0),
            Align2::LEFT_BOTTOM,
            &self.status,
            FontId::proportional(12.0),
            t.text_dim,
        );
    }

    /// Manipulators for the open dialog (distance arrow, angle ring).
    fn manipulators(&mut self, ui: &mut Ui, rect: Rect, t: &Tokens) {
        let Some(dlg) = self.dialog.as_ref() else { return };
        let cmd = dlg.command.clone();
        let d = self.session.document();
        let num = |text: Option<&str>| text.and_then(|x| d.eval(&formcraft_engine::doc::Expr(x.to_string())).ok());
        // The sketch the dialog refers to (field or latest).
        let sketch = || {
            let id = dlg
                .text("sketch")
                .and_then(|x| x.parse::<u32>().ok())
                .map(FeatureId)
                .or_else(|| self.session.model().sketches.last().map(|s| s.id))?;
            self.session.model().sketch(id)
        };
        match cmd.as_str() {
            "solid.extrude" => {
                let (Some(sk), Some(v)) = (sketch(), num(dlg.text("distance"))) else { return };
                let Some(region) =
                    formcraft_sketch::default_regions(&sk.regions).first().and_then(|i| sk.regions.get(*i))
                else {
                    return;
                };
                let origin = sk.plane.to_world(region.sample);
                if let Some(nv) = manipulator::arrow(ui, rect, &self.camera, t, origin, sk.plane.normal, v)
                    && let Some(dl) = self.dialog.as_mut()
                {
                    dl.set_text("distance", formcraft_engine::doc::expr::fmt_num(nv));
                }
            }
            "solid.press_pull" => {
                let Some(v) = num(dlg.text("distance")) else { return };
                let body = dlg
                    .text("body")
                    .and_then(|x| x.parse::<u32>().ok())
                    .map(BodyId)
                    .or_else(|| self.session.model().bodies.last().map(|b| b.id));
                let face_text = dlg.text("face").unwrap_or("top").to_string();
                let face: formcraft_engine::doc::FaceSel =
                    serde_json::from_str(&face_text).unwrap_or(formcraft_engine::doc::FaceSel::Name(face_text.clone()));
                let Some(b) = body.and_then(|b| self.session.model().body(b)) else { return };
                let Ok(f) = formcraft_engine::regen::select::resolve_planar_face(
                    self.session.kernel().as_ref(),
                    &b.shape,
                    &face,
                ) else {
                    return;
                };
                if let Some(nv) = manipulator::arrow(ui, rect, &self.camera, t, f.center, f.normal.normalize(), v)
                    && let Some(dl) = self.dialog.as_mut()
                {
                    dl.set_text("distance", formcraft_engine::doc::expr::fmt_num(nv));
                }
            }
            "solid.revolve" => {
                let (Some(sk), Some(deg)) = (sketch(), num(dlg.text("angle")).or(Some(360.0))) else { return };
                let axis = match dlg.text("axis").unwrap_or("Z") {
                    "X" | "x" => DVec3::X,
                    "Y" | "y" => DVec3::Y,
                    _ => DVec3::Z,
                };
                let Some(region) = sk.regions.first() else { return };
                let start = sk.plane.to_world(region.sample);
                let r = (start - axis * start.dot(axis)).length();
                if let Some(nv) = manipulator::ring(
                    ui,
                    rect,
                    &self.camera,
                    t,
                    DVec3::ZERO + axis * start.dot(axis),
                    axis,
                    r,
                    start,
                    deg,
                ) && let Some(dl) = self.dialog.as_mut()
                {
                    dl.set_text("angle", formcraft_engine::doc::expr::fmt_num(nv));
                }
            }
            _ => {}
        }
    }

    /// Hover pre-selection and click selection on the canvas (Ctrl/Shift adds/removes).
    fn handle_picking(&mut self, ui: &Ui, resp: &egui::Response, rect: Rect, size: DVec2) {
        let dragging = ui.input(|i| i.pointer.any_down());
        let at = resp.hover_pos().map(|h| DVec2::new((h.x - rect.left()) as f64, (h.y - rect.top()) as f64));
        let hit = match at {
            Some(px) if !dragging || resp.clicked() => self.viewport.pick(&self.camera, px, size, &self.session),
            _ => None,
        };
        if !dragging {
            self.session.selection_mut().hover = hit;
        }
        if resp.clicked() && self.nav == NavMode::Select {
            let multi = ui.input(|i| i.modifiers.ctrl || i.modifiers.shift);
            let sel = self.session.selection_mut();
            match hit {
                Some(p) if multi => sel.toggle(p),
                Some(p) => sel.set(p),
                None if !multi => sel.clear(),
                None => {}
            }
            self.status = self.selection_summary();
        }
    }

    fn selection_summary(&self) -> String {
        let items = &self.session.selection().items;
        match items.as_slice() {
            [] => "Ready".into(),
            [one] => {
                let body = self.session.document().body_name(one.body());
                match one {
                    Pick::Body { .. } => format!("Selected: {body}"),
                    Pick::Face { index, .. } => format!("Selected: face {index} of {body}"),
                    Pick::Edge { index, .. } => format!("Selected: edge {index} of {body}"),
                }
            }
            many => format!("{} items selected", many.len()),
        }
    }

    fn paint_origin_items(&self, painter: &egui::Painter, t: &Tokens, size: DVec2, to_pos: impl Fn(DVec2) -> Pos2) {
        let s = grid::spacing(&self.camera) * 5.0;
        let planes = [(DVec3::X, DVec3::Y, "XY"), (DVec3::X, DVec3::Z, "XZ"), (DVec3::Y, DVec3::Z, "YZ")];
        for (i, (a, b, name)) in planes.iter().enumerate() {
            if !self.browser.planes.get(i).copied().unwrap_or(false) {
                continue;
            }
            let quad = [DVec3::ZERO, *a * s, (*a + *b) * s, *b * s];
            let pts: Vec<Pos2> = quad.iter().filter_map(|p| self.camera.project(*p, size)).map(&to_pos).collect();
            if pts.len() == 4 {
                painter.add(egui::Shape::convex_polygon(
                    pts.clone(),
                    t.accent.gamma_multiply(0.15),
                    Stroke::new(1.0, t.accent),
                ));
                if let Some(p) = pts.get(2) {
                    painter.text(*p, Align2::LEFT_BOTTOM, *name, FontId::proportional(11.0), t.accent);
                }
            }
        }
        let axes = [(DVec3::X, t.axis_x), (DVec3::Y, t.axis_y), (DVec3::Z, t.axis_z)];
        for (i, (d, c)) in axes.iter().enumerate() {
            if !self.browser.axes.get(i).copied().unwrap_or(false) {
                continue;
            }
            if let Some((p, q)) = grid::project_segment(&self.camera, -*d * s * 4.0, *d * s * 4.0, size) {
                painter.line_segment([to_pos(p), to_pos(q)], Stroke::new(2.0, *c));
            }
        }
    }

    /// Fusion-style mouse navigation: wheel zooms at the cursor, middle drag pans,
    /// Shift + middle drag orbits; the navigation bar can bind the left button too.
    fn navigate(&mut self, ui: &Ui, resp: &egui::Response, rect: Rect, size: DVec2) {
        let (shift, scroll, middle_down, delta) =
            ui.input(|i| (i.modifiers.shift, i.smooth_scroll_delta.y, i.pointer.middle_down(), i.pointer.delta()));
        let d = DVec2::new(delta.x as f64, delta.y as f64);
        let hover = resp.hover_pos();
        if resp.dragged_by(egui::PointerButton::Middle)
            || (middle_down && resp.hovered() && d != DVec2::ZERO && resp.is_pointer_button_down_on())
        {
            if shift { self.camera.orbit(d) } else { self.camera.pan(d, size.y) }
        } else if resp.dragged_by(egui::PointerButton::Primary) {
            match self.nav {
                NavMode::Orbit => self.camera.orbit(d),
                NavMode::Pan => self.camera.pan(d, size.y),
                NavMode::Zoom => self.camera.zoom(-d.y / 40.0, None),
                NavMode::Select => {}
            }
        }
        if resp.double_clicked_by(egui::PointerButton::Middle) {
            self.fit();
        }
        if let Some(h) = hover
            && scroll != 0.0
        {
            let px = DVec2::new((h.x - rect.left()) as f64, (h.y - rect.top()) as f64);
            let anchor = self.cursor_anchor(px, size);
            self.camera.zoom(scroll as f64 / 50.0, anchor);
        }
    }

    /// World point under the cursor for zooming: the hit on the ground plane, else the
    /// point at target depth.
    fn cursor_anchor(&self, px: DVec2, size: DVec2) -> Option<DVec3> {
        let (o, d) = self.camera.ray(px, size);
        if d.z.abs() > 1e-6 {
            let s = -o.z / d.z;
            if s > 0.0 && s < self.camera.distance * 50.0 {
                return Some(o + d * s);
            }
        }
        Some(o + d * (self.camera.target - o).dot(d))
    }

    fn fit(&mut self) {
        let aspect = 1.6;
        let k = self.session.kernel().clone();
        let mut bb = formcraft_kernel::Aabb::empty();
        for b in self.session.model().bodies.iter().filter(|b| b.visible) {
            if let Ok(m) = k.mass_props(&b.shape) {
                bb = bb.union(&m.bbox);
            }
        }
        if bb.is_empty() {
            // Nothing to frame: the default work area.
            self.camera.fit(DVec3::new(-50.0, -50.0, 0.0), DVec3::new(50.0, 50.0, 20.0), aspect);
        } else {
            self.camera.fit(bb.min, bb.max, aspect);
        }
        self.status = "Fit".into();
    }

    fn nav_bar(&mut self, ui: &mut Ui, t: &Tokens, canvas: Rect) {
        let pos = pos2(canvas.center().x, canvas.bottom() - 12.0);
        egui::Area::new(Id::new("nav_bar")).fixed_pos(pos).pivot(Align2::CENTER_BOTTOM).show(ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(t.panel.gamma_multiply(0.95))
                .corner_radius(6)
                .stroke(Stroke::new(1.0, t.separator))
                .inner_margin(egui::Margin::symmetric(8, 3))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for (mode, icon, tip) in [
                            (NavMode::Orbit, "orbit", "Orbit (Shift + middle drag)"),
                            (NavMode::Pan, "pan", "Pan (middle drag)"),
                            (NavMode::Zoom, "zoom", "Zoom (wheel)"),
                        ] {
                            let on = self.nav == mode;
                            if icons::button(ui, t, icon, tip, 22.0, true, on).clicked() {
                                self.nav = if on { NavMode::Select } else { mode };
                            }
                        }
                        if icons::button(ui, t, "fit", "Fit (F6, double-click middle button)", 22.0, true, false)
                            .clicked()
                        {
                            self.fit();
                        }
                        ui.separator();
                        let style = icons::button(ui, t, "style", "Visual Style", 22.0, true, false);
                        egui::Popup::menu(&style).show(|ui| {
                            for s in VisualStyle::ALL {
                                ui.selectable_value(&mut self.layout.visual_style, s, s.label());
                            }
                        });
                        if icons::button(ui, t, "grid", "Grid", 22.0, true, self.layout.grid).clicked() {
                            self.layout.grid = !self.layout.grid;
                        }
                        let persp = self.camera.projection == Projection::Perspective;
                        if icons::button(ui, t, "ortho", "Orthographic / Perspective", 22.0, true, !persp).clicked() {
                            self.camera.projection =
                                if persp { Projection::Orthographic } else { Projection::Perspective };
                        }
                    });
                });
        });
    }

    // ------------------------------------------------------------------ keyboard

    fn shortcuts(&mut self, ui: &Ui) {
        if ui.ctx().egui_wants_keyboard_input() {
            return;
        }
        self.registry_shortcuts(ui);
        let (undo, redo) = ui.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)
                    || i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::Z),
            )
        });
        if undo && self.session.can_undo() {
            self.run("doc.undo", Value::Null);
        }
        if redo && self.session.can_redo() {
            self.run("doc.redo", Value::Null);
        }
        if let Some(m) = self.sketch_mode.as_mut() {
            let key = ui.input(|i| {
                [
                    (egui::Key::L, Tool::Line),
                    (egui::Key::R, Tool::Rectangle),
                    (egui::Key::C, Tool::Circle),
                    (egui::Key::D, Tool::Dimension),
                ]
                .into_iter()
                .find(|(k, _)| i.key_pressed(*k) && i.modifiers.is_none())
                .map(|(_, t)| t)
            });
            if let Some(tool) = key {
                m.set_tool(tool);
            }
        }
        let (home, f6) = ui.input(|i| (i.key_pressed(egui::Key::Home), i.key_pressed(egui::Key::F6)));
        if home {
            self.apply_named_view("HOME");
        }
        if f6 {
            self.fit();
        }
    }

    // ------------------------------------------------------------------ screenshots

    fn handle_screenshot(&mut self, ui: &Ui) {
        let Some(path) = self.options.screenshot.clone() else { return };
        self.frame += 1;
        if self.frame == 5 && !self.screenshot_requested {
            self.screenshot_requested = true;
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        let image = ui.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = image {
            if let Err(e) = save_png(&img, &path) {
                log::error!("screenshot: {e}");
            }
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ui.ctx().request_repaint();
    }

    /// Paint the whole UI into `ui` (the root of the window).
    pub fn draw(&mut self, ui: &mut Ui) {
        let t = self.tokens();
        ui.ctx().set_visuals(t.visuals());
        self.shortcuts(ui);
        self.app_bar(ui, &t);
        self.toolbar(ui, &t);
        self.timeline(ui, &t);
        self.text_commands(ui, &t);
        self.browser(ui, &t);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::TRANSPARENT))
            .show(ui, |ui| self.canvas(ui, &t));
        self.feature_dialog(ui, &t);
        self.rename_dialog(ui);
        self.plane_picker_ui(ui);
        self.overlays(ui, &t);
        if self.sketch_mode.is_some() {
            self.sketch_palette(ui, &t);
        }
        self.handle_screenshot(ui);
    }
}

fn eye_toggle(ui: &mut Ui, t: &Tokens, visible: &mut bool) {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::click());
    let c = if *visible { t.text } else { t.text_dim.gamma_multiply(0.5) };
    crate::icons::paint(ui.painter(), rect, "eye", c, t);
    if resp.on_hover_text(if *visible { "Hide" } else { "Show" }).clicked() {
        *visible = !*visible;
    }
}

fn eye_row(ui: &mut Ui, t: &Tokens, name: &str, visible: &mut bool) {
    ui.horizontal(|ui| {
        eye_toggle(ui, t, visible);
        ui.label(name);
    });
}

fn eye_header(ui: &mut Ui, t: &Tokens, name: &str, visible: &mut bool, body: impl FnOnce(&mut Ui)) {
    let id = ui.make_persistent_id(name);
    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false)
        .show_header(ui, |ui| {
            eye_toggle(ui, t, visible);
            ui.label(name);
        })
        .body(body);
}

fn save_png(img: &egui::ColorImage, path: &std::path::Path) -> Result<(), String> {
    let [w, h] = img.size;
    let mut bytes = Vec::with_capacity(w * h * 4);
    for p in &img.pixels {
        bytes.extend_from_slice(&p.to_array());
    }
    let buf = image::RgbaImage::from_raw(w as u32, h as u32, bytes).ok_or("bad image size")?;
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    buf.save(path).map_err(|e| e.to_string())
}

impl eframe::App for FormCraftApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if !self.options.fresh_layout {
            eframe::set_value(storage, LAYOUT_KEY, &self.layout);
            eframe::set_value(storage, CAMERA_KEY, &self.camera);
        }
    }
}

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;
