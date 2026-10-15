//! Sketch mode: draw and constrain a sketch on its plane, like Fusion's sketch environment.
//!
//! Every edit goes through the `sketch.edit` command (one undo step, re-solved by the engine),
//! so what the canvas shows is always the solved sketch from the model.

use egui::{Align2, Color32, FontId, Id, Painter, Pos2, Rect, RichText, Stroke, Ui, Vec2};
use formcraft_engine::Session;
use formcraft_engine::doc::FeatureId;
use formcraft_kernel::{DVec2, DVec3, Plane};
use formcraft_render::camera::Camera;
use formcraft_sketch::{DimKind, Eid, Entity, Sketch};
use serde_json::{Value, json};

use crate::theme::Tokens;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Select,
    Line,
    Rectangle,
    Circle,
    Arc,
    Dimension,
}

impl Tool {
    pub fn from_command(id: &str) -> Option<Tool> {
        Some(match id {
            "sketch.line" => Tool::Line,
            "sketch.rectangle" => Tool::Rectangle,
            "sketch.circle" => Tool::Circle,
            "sketch.arc" => Tool::Arc,
            "sketch.dimension" => Tool::Dimension,
            _ => return None,
        })
    }
    fn label(&self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Line => "Line",
            Tool::Rectangle => "Rectangle",
            Tool::Circle => "Circle",
            Tool::Arc => "3-Point Arc",
            Tool::Dimension => "Dimension",
        }
    }
}

/// A clicked location: an existing point (snapped) or a new position.
#[derive(Debug, Clone, Copy, PartialEq)]
struct At {
    id: Option<Eid>,
    p: DVec2,
}

/// Something under the cursor.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Hit {
    Point(Eid),
    Curve(Eid),
}

pub struct SketchMode {
    pub sketch: FeatureId,
    pub tool: Tool,
    pending: Vec<At>,
    pub selected: Vec<Eid>,
    hover: Option<Hit>,
    /// Dimension being entered: kind, text, screen position.
    dim_input: Option<(DimKind, String, Pos2)>,
    drag: Option<(Eid, DVec2)>,
    /// First point of the current line chain (clicking it again closes the chain).
    chain_origin: Option<Eid>,
    pub message: Option<String>,
}

/// What the app should do after a frame of sketch mode.
pub enum Request {
    None,
    /// Run `sketch.edit` with these ops.
    Edit(Vec<Value>),
    Finish,
}

const SNAP_PX: f32 = 9.0;
const HV_DEG: f64 = 3.0;

impl SketchMode {
    pub fn new(sketch: FeatureId) -> Self {
        SketchMode {
            sketch,
            tool: Tool::Line,
            pending: Vec::new(),
            selected: Vec::new(),
            hover: None,
            dim_input: None,
            drag: None,
            chain_origin: None,
            message: None,
        }
    }

    /// Replace the text of the open dimension field (tests).
    #[cfg(test)]
    pub fn set_pending_dimension_text(&mut self, text: &str) -> bool {
        match self.dim_input.as_mut() {
            Some((_, t, _)) => {
                *t = text.to_string();
                true
            }
            None => false,
        }
    }

    /// No tool operation in progress (right-click may open the marking menu).
    pub fn idle(&self) -> bool {
        self.pending.is_empty() && self.dim_input.is_none()
    }

    pub fn set_tool(&mut self, t: Tool) {
        self.tool = t;
        self.pending.clear();
        self.chain_origin = None;
        self.dim_input = None;
    }

    /// Feed back the ids a `sketch.edit` created, so the line chain continues from real points.
    pub fn after_edit(&mut self, result: &Value) {
        if self.tool != Tool::Line {
            return;
        }
        let created = &result["created"][0];
        let id = |i: usize| created.get(i).and_then(Value::as_u64).map(|v| Eid(v as u32));
        if self.chain_origin.is_none() {
            self.chain_origin = id(0);
        }
        if let Some(last) = self.pending.last_mut()
            && last.id.is_none()
        {
            last.id = id(1);
        }
    }

    /// Camera looking straight at the sketch plane.
    pub fn look_at(plane: &Plane, cam: &mut Camera) {
        cam.look_from(plane.normal);
        cam.target = plane.origin;
    }

    fn to_screen(plane: &Plane, cam: &Camera, rect: Rect, p: DVec2) -> Option<Pos2> {
        let size = glam::DVec2::new(rect.width() as f64, rect.height() as f64);
        cam.project(plane.to_world(p), size).map(|s| rect.min + Vec2::new(s.x as f32, s.y as f32))
    }

    /// Cursor → sketch-plane coordinates (ray/plane intersection).
    fn to_plane(plane: &Plane, cam: &Camera, rect: Rect, pos: Pos2) -> Option<DVec2> {
        let size = glam::DVec2::new(rect.width() as f64, rect.height() as f64);
        let px = glam::DVec2::new((pos.x - rect.left()) as f64, (pos.y - rect.top()) as f64);
        let (o, d): (DVec3, DVec3) = cam.ray(px, size);
        let denom = d.dot(plane.normal);
        if denom.abs() < 1e-9 {
            return None;
        }
        let t = (plane.origin - o).dot(plane.normal) / denom;
        (t.is_finite() && t > 0.0).then(|| plane.to_local(o + d * t))
    }

    fn hit(sk: &Sketch, plane: &Plane, cam: &Camera, rect: Rect, pos: Pos2) -> Option<Hit> {
        let scr = |p: (f64, f64)| Self::to_screen(plane, cam, rect, DVec2::new(p.0, p.1));
        // Points first.
        let mut best: Option<(f32, Hit)> = None;
        for (id, e) in &sk.entities {
            if let Entity::Point { x, y } = e
                && let Some(s) = scr((*x, *y))
            {
                let d = s.distance(pos);
                if d < SNAP_PX && best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, Hit::Point(*id)));
                }
            }
        }
        if best.is_some() {
            return best.map(|b| b.1);
        }
        for (id, e) in &sk.entities {
            let d = match *e {
                Entity::Line { a, b, .. } => match (sk.point(a).ok().and_then(scr), sk.point(b).ok().and_then(scr)) {
                    (Some(sa), Some(sb)) => seg_dist(pos, sa, sb),
                    _ => f32::INFINITY,
                },
                Entity::Circle { center, radius, .. } => {
                    match (sk.point(center).ok().and_then(scr), sk.point(center).ok()) {
                        (Some(sc), Some(c)) => {
                            let edge = scr((c.0 + radius, c.1)).map(|e| e.distance(sc)).unwrap_or(0.0);
                            (pos.distance(sc) - edge).abs()
                        }
                        _ => f32::INFINITY,
                    }
                }
                Entity::Arc { center, start, .. } => {
                    match (sk.point(center).ok().and_then(scr), sk.point(start).ok().and_then(scr)) {
                        (Some(sc), Some(ss)) => (pos.distance(sc) - ss.distance(sc)).abs(),
                        _ => f32::INFINITY,
                    }
                }
                Entity::Point { .. } => f32::INFINITY,
            };
            if d < 6.0 && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, Hit::Curve(*id)));
            }
        }
        best.map(|b| b.1)
    }

    fn at(sk: &Sketch, hit: Option<Hit>, p: DVec2) -> At {
        match hit {
            Some(Hit::Point(id)) => At { id: Some(id), p: sk.point(id).map(|(x, y)| DVec2::new(x, y)).unwrap_or(p) },
            _ => At { id: None, p },
        }
    }

    fn point_json(prefix: &str, a: &At, op: &mut Value) {
        match a.id {
            Some(id) => op[format!("{prefix}_point")] = json!(id.0),
            None => op[prefix] = json!([round(a.p.x), round(a.p.y)]),
        }
    }

    /// Handle input and draw the sketch overlay. `resp` is the canvas response.
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        resp: &egui::Response,
        rect: Rect,
        s: &Session,
        cam: &Camera,
        t: &Tokens,
    ) -> Request {
        let Some(r) = s.model().sketch(self.sketch) else {
            self.message = Some("this sketch failed to compute".into());
            return Request::None;
        };
        let (plane, sk) = (r.plane, &r.sketch);
        let painter = ui.painter_at(rect);
        let pos = resp.hover_pos();
        let local = pos.and_then(|p| Self::to_plane(&plane, cam, rect, p));
        self.hover = pos.and_then(|p| Self::hit(sk, &plane, cam, rect, p));
        let mut req = Request::None;

        // ---- input
        let clicked = resp.clicked_by(egui::PointerButton::Primary);
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.pending.is_empty() && self.dim_input.is_none() {
                self.tool = Tool::Select;
            }
            self.pending.clear();
            self.dim_input = None;
        }
        if resp.clicked_by(egui::PointerButton::Secondary) {
            self.pending.clear();
        }
        if let (true, Some(p)) = (clicked, local) {
            let at = Self::at(sk, self.hover, p);
            match self.tool {
                Tool::Select => {
                    let multi = ui.input(|i| i.modifiers.ctrl || i.modifiers.shift);
                    match self.hover.map(|h| match h {
                        Hit::Point(e) | Hit::Curve(e) => e,
                    }) {
                        Some(e) if multi => {
                            if let Some(i) = self.selected.iter().position(|x| *x == e) {
                                self.selected.remove(i);
                            } else {
                                self.selected.push(e);
                            }
                        }
                        Some(e) => self.selected = vec![e],
                        None => self.selected.clear(),
                    }
                }
                Tool::Line => match self.pending.last().copied() {
                    None => {
                        self.pending = vec![at];
                        self.chain_origin = at.id;
                    }
                    Some(prev) if prev.p.distance(at.p) > 1e-6 => {
                        let mut op = json!({"op": "line"});
                        Self::point_json("a", &prev, &mut op);
                        Self::point_json("b", &at, &mut op);
                        let ang = (at.p - prev.p).to_angle().to_degrees().rem_euclid(180.0);
                        if !(HV_DEG..=180.0 - HV_DEG).contains(&ang) {
                            op["horizontal"] = json!(true);
                        } else if (ang - 90.0).abs() < HV_DEG {
                            op["vertical"] = json!(true);
                        }
                        req = Request::Edit(vec![op]);
                        // The chain continues from this point unless it closed on its start.
                        let closes = at.id.is_some() && at.id == self.chain_origin;
                        self.pending = if closes { Vec::new() } else { vec![at] };
                        if closes {
                            self.chain_origin = None;
                        }
                    }
                    Some(_) => {}
                },
                Tool::Rectangle => {
                    if let Some(a) = self.pending.pop() {
                        req = Request::Edit(vec![
                            json!({"op": "rectangle", "a": [round(a.p.x), round(a.p.y)], "b": [round(at.p.x), round(at.p.y)]}),
                        ]);
                    } else {
                        self.pending.push(at);
                    }
                }
                Tool::Circle => {
                    if let Some(c) = self.pending.pop() {
                        let r = c.p.distance(at.p);
                        if r > 1e-6 {
                            let mut op = json!({"op": "circle", "radius": round(r)});
                            Self::point_json("center", &c, &mut op);
                            req = Request::Edit(vec![op]);
                        }
                    } else {
                        self.pending.push(at);
                    }
                }
                Tool::Arc => {
                    self.pending.push(at);
                    if let [c, a, b] = self.pending.as_slice() {
                        // project the end onto the radius so the arc is exact
                        let r = c.p.distance(a.p);
                        let end = At { id: b.id, p: c.p + (b.p - c.p).normalize_or_zero() * r };
                        let mut op = json!({"op": "arc"});
                        Self::point_json("center", c, &mut op);
                        Self::point_json("start", a, &mut op);
                        Self::point_json("end", &end, &mut op);
                        req = Request::Edit(vec![op]);
                        self.pending.clear();
                    }
                }
                Tool::Dimension => self.start_dimension(sk, pos.unwrap_or(rect.center())),
            }
        }
        // Drag a point (select tool) to move it; constraints are re-solved on release.
        if self.tool == Tool::Select {
            if resp.drag_started_by(egui::PointerButton::Primary)
                && let Some(Hit::Point(id)) = self.hover
                && let Some(p) = local
            {
                self.drag = Some((id, p));
            }
            if let (Some((id, _)), Some(p)) = (self.drag, local) {
                self.drag = Some((id, p));
            }
            if resp.drag_stopped()
                && let Some((id, p)) = self.drag.take()
            {
                req = Request::Edit(vec![json!({"op": "move", "point": id.0, "to": [round(p.x), round(p.y)]})]);
            }
        }
        if ui.input(|i| i.key_pressed(egui::Key::Delete))
            && !self.selected.is_empty()
            && !ui.ctx().egui_wants_keyboard_input()
        {
            req = Request::Edit(self.selected.drain(..).map(|e| json!({"op": "delete", "id": e.0})).collect());
        }

        // ---- draw
        self.draw(&painter, sk, &plane, cam, rect, t, local);
        if let Some(r2) = self.dimension_input(ui, t) {
            req = r2;
        }
        req
    }

    fn start_dimension(&mut self, sk: &Sketch, pos: Pos2) {
        let kind = match self.hover {
            Some(Hit::Curve(id)) => match sk.get(id) {
                Some(Entity::Line { .. }) => Some((DimKind::Length { line: id }, line_len(sk, id))),
                Some(Entity::Circle { radius, .. }) => Some((DimKind::Diameter { curve: id }, radius * 2.0)),
                Some(Entity::Arc { center, start, .. }) => {
                    let r = match (sk.point(*center), sk.point(*start)) {
                        (Ok(c), Ok(s)) => ((c.0 - s.0).powi(2) + (c.1 - s.1).powi(2)).sqrt(),
                        _ => 0.0,
                    };
                    Some((DimKind::Radius { curve: id }, r))
                }
                _ => None,
            },
            Some(Hit::Point(id)) => {
                // second point → distance
                match self.pending.first().and_then(|a| a.id) {
                    Some(first) if first != id => {
                        self.pending.clear();
                        let d = match (sk.point(first), sk.point(id)) {
                            (Ok(a), Ok(b)) => ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt(),
                            _ => 0.0,
                        };
                        Some((DimKind::Distance { a: first, b: id }, d))
                    }
                    _ => {
                        self.pending = vec![At { id: Some(id), p: DVec2::ZERO }];
                        None
                    }
                }
            }
            None => None,
        };
        if let Some((k, v)) = kind {
            self.dim_input = Some((k, format!("{}", round(v)), pos));
        }
    }

    fn dimension_input(&mut self, ui: &mut Ui, t: &Tokens) -> Option<Request> {
        let (kind, mut text, pos) = self.dim_input.take()?;
        let mut done = None;
        egui::Area::new(Id::new("dim_input")).fixed_pos(pos + Vec2::new(12.0, -12.0)).show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Dimension").color(t.text_secondary));
                    let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(90.0));
                    r.request_focus();
                    if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        done = Some(true);
                    }
                });
            });
        });
        match done {
            Some(true) => {
                let value = text.trim().parse::<f64>().map(|v| json!(v)).unwrap_or(json!(text.trim()));
                Some(Request::Edit(vec![json!({"op": "dimension", "dimension": kind, "value": value})]))
            }
            _ => {
                self.dim_input = Some((kind, text, pos));
                None
            }
        }
    }

    fn draw(
        &self,
        p: &Painter,
        sk: &Sketch,
        plane: &Plane,
        cam: &Camera,
        rect: Rect,
        t: &Tokens,
        cursor: Option<DVec2>,
    ) {
        let scr = |q: DVec2| Self::to_screen(plane, cam, rect, q);
        let pt = |id: Eid| sk.point(id).ok().map(|(x, y)| DVec2::new(x, y));
        let normal = t.sketch_under;
        let constr = t.sketch_construction;
        let sel = t.warning;
        let hov = t.accent_hover;
        let color_of = |id: Eid, construction: bool| {
            if self.selected.contains(&id) {
                sel
            } else if self.hover == Some(Hit::Curve(id)) || self.hover == Some(Hit::Point(id)) {
                hov
            } else if construction {
                constr
            } else {
                normal
            }
        };
        let poly = |pts: &[DVec2], c: Color32, w: f32| {
            let v: Vec<Pos2> = pts.iter().filter_map(|q| scr(*q)).collect();
            if v.len() >= 2 {
                p.add(egui::Shape::line(v, Stroke::new(w, c)));
            }
        };
        for (id, e) in &sk.entities {
            match *e {
                Entity::Line { a, b, construction } => {
                    if let (Some(pa), Some(pb)) = (pt(a), pt(b)) {
                        poly(&[pa, pb], color_of(*id, construction), 2.0);
                    }
                }
                Entity::Circle { center, radius, construction } => {
                    if let Some(c) = pt(center) {
                        let pts: Vec<DVec2> = (0..=64)
                            .map(|k| c + DVec2::from_angle(k as f64 / 64.0 * std::f64::consts::TAU) * radius)
                            .collect();
                        poly(&pts, color_of(*id, construction), 2.0);
                    }
                }
                Entity::Arc { center, start, end, construction } => {
                    if let (Some(c), Some(s), Some(e2)) = (pt(center), pt(start), pt(end)) {
                        let r = c.distance(s);
                        let a0 = (s - c).to_angle();
                        let mut sweep = (e2 - c).to_angle() - a0;
                        while sweep <= 0.0 {
                            sweep += std::f64::consts::TAU;
                        }
                        let pts: Vec<DVec2> =
                            (0..=48).map(|k| c + DVec2::from_angle(a0 + sweep * k as f64 / 48.0) * r).collect();
                        poly(&pts, color_of(*id, construction), 2.0);
                    }
                }
                Entity::Point { .. } => {}
            }
        }
        for (id, e) in &sk.entities {
            if let Entity::Point { x, y } = e
                && let Some(s) = scr(DVec2::new(*x, *y))
            {
                let c = color_of(*id, false);
                p.rect_filled(Rect::from_center_size(s, Vec2::splat(5.0)), 0.0, c);
            }
        }
        // Dimension labels at their entities.
        for d in &sk.dimensions {
            let anchor = match d.kind {
                DimKind::Length { line } | DimKind::PointLine { line, .. } => match sk.get(line) {
                    Some(Entity::Line { a, b, .. }) => pt(*a).zip(pt(*b)).map(|(a, b)| (a + b) / 2.0),
                    _ => None,
                },
                DimKind::Distance { a, b } => pt(a).zip(pt(b)).map(|(a, b)| (a + b) / 2.0),
                // Horizontal: centred under the measured span at the second point's height;
                // vertical: beside the second point, half-way up.
                DimKind::HorizontalDistance { a, b } => {
                    pt(a).zip(pt(b)).map(|(a, b)| DVec2::new((a.x + b.x) / 2.0, b.y))
                }
                DimKind::VerticalDistance { a, b } => pt(a).zip(pt(b)).map(|(a, b)| DVec2::new(b.x, (a.y + b.y) / 2.0)),
                DimKind::Radius { curve } | DimKind::Diameter { curve } => match sk.get(curve) {
                    Some(Entity::Circle { center, radius, .. }) => pt(*center).map(|c| c + DVec2::new(*radius, 0.0)),
                    Some(Entity::Arc { start, .. }) => pt(*start),
                    _ => None,
                },
                DimKind::Angle { a, .. } => match sk.get(a) {
                    Some(Entity::Line { a, .. }) => pt(*a),
                    _ => None,
                },
            };
            if let Some(s) = anchor.and_then(scr) {
                let (text, offset) = match d.kind {
                    DimKind::Diameter { .. } => (format!("Ø{}", d.value), Vec2::new(6.0, -6.0)),
                    DimKind::Radius { .. } => (format!("R{}", d.value), Vec2::new(6.0, -6.0)),
                    DimKind::Angle { .. } => (format!("{}°", d.value), Vec2::new(10.0, -10.0)),
                    DimKind::VerticalDistance { .. } => (d.value.clone(), Vec2::new(14.0, 0.0)),
                    DimKind::HorizontalDistance { .. } => (d.value.clone(), Vec2::new(0.0, 14.0)),
                    _ => (d.value.clone(), Vec2::new(0.0, -10.0)),
                };
                let at = s + offset;
                let galley = p.layout_no_wrap(text, FontId::proportional(12.0), t.text_primary);
                let r = Rect::from_center_size(at, galley.size() + Vec2::new(6.0, 2.0));
                p.rect_filled(r, 2.0, t.panel_bg.gamma_multiply(0.85));
                p.galley(r.min + Vec2::new(3.0, 1.0), galley, t.text_primary);
            }
        }
        // Rubber band for the tool in progress.
        if let (Some(c), Some(first)) = (cursor, self.pending.first()) {
            let cur = match self.hover {
                Some(Hit::Point(id)) => pt(id).unwrap_or(c),
                _ => c,
            };
            let band = t.accent_hover.gamma_multiply(0.8);
            match self.tool {
                Tool::Line => poly(&[first.p, cur], band, 1.5),
                Tool::Rectangle => poly(
                    &[first.p, DVec2::new(cur.x, first.p.y), cur, DVec2::new(first.p.x, cur.y), first.p],
                    band,
                    1.5,
                ),
                Tool::Circle => {
                    let r = first.p.distance(cur);
                    let pts: Vec<DVec2> = (0..=64)
                        .map(|k| first.p + DVec2::from_angle(k as f64 / 64.0 * std::f64::consts::TAU) * r)
                        .collect();
                    poly(&pts, band, 1.5);
                }
                Tool::Arc => {
                    for q in &self.pending {
                        poly(&[q.p, cur], band, 1.0);
                    }
                }
                _ => {}
            }
            if let Some(s) = scr(cur) {
                let d = cur - first.p;
                p.text(
                    s + Vec2::new(14.0, 14.0),
                    Align2::LEFT_TOP,
                    format!("{:.2}, {:.2}  ({:.2})", cur.x, cur.y, d.length()),
                    FontId::monospace(11.0),
                    t.text_secondary,
                );
            }
        }
        if let Some((id, to)) = self.drag
            && let (Some(a), Some(b)) = (pt(id).and_then(scr), scr(to))
        {
            p.line_segment([a, b], Stroke::new(1.0, t.accent_hover));
        }
    }

    /// Sketch palette contents: constraints for the selection, DOF, finish.
    pub fn palette(&mut self, ui: &mut Ui, s: &Session, t: &Tokens) -> Request {
        let mut req = Request::None;
        let (dof, sk) = match s.model().sketch(self.sketch) {
            Some(r) => (Some(r.dof), Some(r.sketch.clone())),
            None => (None, None),
        };
        ui.label(RichText::new("SKETCH PALETTE").size(11.0).strong().color(t.text_secondary));
        ui.separator();
        egui::Grid::new("sketch_tools").num_columns(2).show(ui, |ui| {
            for (i, tool) in [Tool::Select, Tool::Line, Tool::Rectangle, Tool::Circle, Tool::Arc, Tool::Dimension]
                .into_iter()
                .enumerate()
            {
                if ui.selectable_label(self.tool == tool, tool.label()).clicked() {
                    self.set_tool(tool);
                }
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
        ui.separator();
        ui.label(RichText::new("Constraints").strong());
        let sel = self.selected.clone();
        let kind_of = |id: Eid| sk.as_ref().and_then(|s| s.get(id).cloned());
        let is_line = |id: Eid| matches!(kind_of(id), Some(Entity::Line { .. }));
        let is_point = |id: Eid| matches!(kind_of(id), Some(Entity::Point { .. }));
        let is_round = |id: Eid| matches!(kind_of(id), Some(Entity::Circle { .. } | Entity::Arc { .. }));
        let mut c = |label: &str, ok: bool, constraint: Option<Value>| {
            if ui.add_enabled(ok, egui::Button::new(label)).clicked()
                && let Some(c) = constraint
            {
                req = Request::Edit(vec![json!({"op": "constraint", "constraint": c})]);
            }
        };
        let two = |f: &dyn Fn(Eid) -> bool, g: &dyn Fn(Eid) -> bool| matches!(sel.as_slice(), [a, b] if f(*a) && g(*b));
        let pair = |ty: &str| sel.first().zip(sel.get(1)).map(|(a, b)| json!({"type": ty, "a": a.0, "b": b.0}));
        c("Coincident", two(&is_point, &is_point), pair("coincident"));
        c(
            "Point on curve",
            matches!(sel.as_slice(), [a, b] if is_point(*a) && !is_point(*b)),
            sel.first().zip(sel.get(1)).map(|(a, b)| json!({"type": "point_on", "point": a.0, "curve": b.0})),
        );
        c(
            "Horizontal",
            matches!(sel.as_slice(), [a] if is_line(*a)),
            sel.first().map(|a| json!({"type": "horizontal", "line": a.0})),
        );
        c(
            "Vertical",
            matches!(sel.as_slice(), [a] if is_line(*a)),
            sel.first().map(|a| json!({"type": "vertical", "line": a.0})),
        );
        c("Parallel", two(&is_line, &is_line), pair("parallel"));
        c("Perpendicular", two(&is_line, &is_line), pair("perpendicular"));
        c(
            "Tangent",
            matches!(sel.as_slice(), [a, b] if (is_line(*a) || is_round(*a)) && is_round(*b)),
            pair("tangent"),
        );
        c("Equal", two(&is_line, &is_line) || two(&is_round, &is_round), pair("equal"));
        c("Concentric", two(&is_round, &is_round), pair("concentric"));
        c(
            "Midpoint",
            matches!(sel.as_slice(), [a, b] if is_point(*a) && is_line(*b)),
            sel.first().zip(sel.get(1)).map(|(a, b)| json!({"type": "midpoint", "point": a.0, "line": b.0})),
        );
        c(
            "Fix",
            matches!(sel.as_slice(), [a] if is_point(*a)),
            sel.first().map(|a| json!({"type": "fix", "point": a.0})),
        );
        ui.separator();
        match dof {
            Some(0) => ui.label(RichText::new("Fully constrained").color(t.ok)),
            Some(n) => ui.label(format!("{n} degrees of freedom")),
            None => ui.label(RichText::new("Sketch failed").color(t.error)),
        };
        if let Some(m) = &self.message {
            ui.label(RichText::new(m).color(t.warning));
        }
        ui.add_space(6.0);
        if ui.add(egui::Button::new(RichText::new("Finish Sketch").color(Color32::WHITE)).fill(t.ok)).clicked() {
            req = Request::Finish;
        }
        req
    }
}

fn round(v: f64) -> f64 {
    (v * 1e4).round() / 1e4
}

fn line_len(sk: &Sketch, id: Eid) -> f64 {
    match sk.get(id) {
        Some(Entity::Line { a, b, .. }) => match (sk.point(*a), sk.point(*b)) {
            (Ok(a), Ok(b)) => ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt(),
            _ => 0.0,
        },
        _ => 0.0,
    }
}

fn seg_dist(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let l2 = ab.length_sq();
    let t = if l2 > 0.0 { ((p - a).dot(ab) / l2).clamp(0.0, 1.0) } else { 0.0 };
    p.distance(a + ab * t)
}
