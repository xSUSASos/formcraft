//! The application shell: Fusion-style layout (PLAN.md §2).
//!
//! Application bar (top) → toolbar with tabs and groups → browser (left) → canvas with ViewCube
//! and navigation bar → timeline (bottom) → Text Commands (collapsible, bottom). Panels can be
//! closed, docked or floated, and reset; the layout is saved between runs.

use std::path::PathBuf;

use egui::{Align2, Color32, FontId, Id, Panel, Pos2, Rect, Sense, Stroke, Ui, Vec2, pos2};
use formcraft_engine::Session;
use formcraft_render::camera::{Camera, Projection};
use formcraft_render::grid::{self, LineKind};
use glam::{DVec2, DVec3};
use serde_json::Value;

use crate::icons;
use crate::layout::{Dock, Layout, VisualStyle};
use crate::theme::{ThemeKind, Tokens};
use crate::toolbar::{self, Action};
use crate::viewcube::{self, CubeAction};

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
        Self::with_state(layout, camera, options)
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
        };
        if let Some(v) = view {
            app.apply_named_view(&v.to_uppercase());
        }
        app
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
            "tools.settings" => {
                self.status = "Preferences: theme and layout are in the View menu".into();
                return;
            }
            _ => {}
        }
        self.status = match self.session.execute(id, &params) {
            Ok(v) => {
                v.get("message").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| format!("{id}: done"))
            }
            Err(e) => format!("{id}: {e}"),
        };
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
                                self.run(id, Value::Null);
                                ui.close();
                            }
                        }
                    });
                    let undo_on = self.session.registry().get("doc.undo").is_some();
                    if icons::button(ui, t, "undo", "Undo (Ctrl+Z)", 20.0, undo_on, false).clicked() {
                        self.run("doc.undo", Value::Null);
                    }
                    let redo_on = self.session.registry().get("doc.redo").is_some();
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
                    ui.label(egui::RichText::new("(none)").color(t.text_dim));
                });
                egui::CollapsingHeader::new("Bodies").default_open(true).show(ui, |ui| {
                    ui.label(egui::RichText::new("(none)").color(t.text_dim));
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
        Panel::bottom("timeline")
            .exact_size(self.layout.timeline.size)
            .frame(egui::Frame::new().fill(t.panel).inner_margin(egui::Margin::symmetric(8, 6)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    for (icon, tip) in [
                        ("tl_start", "Move to beginning"),
                        ("tl_back", "Step back"),
                        ("tl_play", "Play"),
                        ("tl_fwd", "Step forward"),
                        ("tl_end", "Move to end"),
                    ] {
                        let _ = icons::button(ui, t, icon, tip, 18.0, false, false);
                    }
                    ui.separator();
                    let (rect, _) =
                        ui.allocate_exact_size(Vec2::new(ui.available_width() - 30.0, 24.0), Sense::hover());
                    ui.painter().rect_filled(rect, 3.0, t.panel_header);
                    ui.painter().text(
                        rect.left_center() + Vec2::new(10.0, 0.0),
                        Align2::LEFT_CENTER,
                        "Timeline: no features yet",
                        FontId::proportional(12.0),
                        t.text_dim,
                    );
                    // History marker at the end of the (empty) timeline.
                    let x = rect.left() + 4.0;
                    ui.painter().line_segment(
                        [pos2(x, rect.top() - 2.0), pos2(x, rect.bottom() + 2.0)],
                        Stroke::new(3.0, t.accent),
                    );
                });
            });
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

        // Grid and axes.
        let origin = rect.min.to_vec2();
        let to_pos = |p: DVec2| Pos2::new(p.x as f32, p.y as f32) + origin;
        if self.layout.grid {
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
            if let Some((p, q)) = grid::project_segment(&self.camera, DVec3::ZERO, DVec3::Z * len, size) {
                painter.line_segment([to_pos(p), to_pos(q)], Stroke::new(1.5, t.axis_z));
            }
            if let Some(o) = self.camera.project(DVec3::ZERO, size) {
                painter.circle_filled(to_pos(o), 3.0, t.text_dim);
            }
            self.paint_origin_items(&painter, t, size, to_pos);
        }

        if self.layout.view_cube {
            let area = Rect::from_min_size(pos2(rect.right() - 150.0, rect.top() + 8.0), Vec2::splat(140.0));
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
        // Nothing to frame yet: fit the default work area.
        let aspect = 1.6;
        self.camera.fit(DVec3::new(-50.0, -50.0, 0.0), DVec3::new(50.0, 50.0, 20.0), aspect);
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
