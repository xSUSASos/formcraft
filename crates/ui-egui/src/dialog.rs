//! Feature dialog: a floating panel generated from a command's JSON schema. Fields accept
//! expressions (`L*2`); OK runs the command (or `feature.edit` when editing), Cancel discards.
//! While open, the canvas shows a live preview of the result.

use egui::{Color32, RichText, Ui};
use formcraft_engine::doc::FeatureId;
use formcraft_engine::{Pick, Session};
use serde_json::{Map, Value, json};

use crate::theme::Tokens;

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Expr,
    Vec3,
    Bool,
    Choice(Vec<String>),
    Body,
    Sketch,
    Faces,
    Edges,
    Text,
    Json,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub key: String,
    pub label: String,
    pub help: String,
    pub kind: Kind,
    pub required: bool,
    pub text: String,
    pub vec: [String; 3],
    pub flag: bool,
}

/// What the dialog wants the app to do this frame.
pub enum Outcome {
    None,
    Ok,
    Cancel,
}

pub struct FeatureDialog {
    pub command: String,
    pub title: String,
    pub editing: Option<FeatureId>,
    pub fields: Vec<Field>,
    pub error: Option<String>,
    /// Params of the last preview (recompute when they change).
    pub previewed: Option<Value>,
}

fn label_of(key: &str) -> String {
    let mut s = key.replace('_', " ");
    if let Some(c) = s.get(0..1) {
        s = c.to_uppercase() + s.get(1..).unwrap_or("");
    }
    s
}

fn types(schema: &Value) -> Vec<String> {
    match schema.get("type") {
        Some(Value::String(t)) => vec![t.clone()],
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        _ => vec![],
    }
}

fn classify(key: &str, schema: &Value) -> Kind {
    let t = types(schema);
    if let Some(Value::Array(e)) = schema.get("enum") {
        return Kind::Choice(e.iter().filter_map(Value::as_str).map(str::to_string).collect());
    }
    match key {
        "body" | "target" | "tool" => return Kind::Body,
        "sketch" => return Kind::Sketch,
        "edges" => return Kind::Edges,
        "face" | "faces" => return Kind::Faces,
        _ => {}
    }
    if t.iter().any(|x| x == "array") && schema.get("minItems").and_then(Value::as_u64) == Some(3) {
        return Kind::Vec3;
    }
    if t.iter().any(|x| x == "boolean") {
        return Kind::Bool;
    }
    if t.iter().any(|x| x == "number") {
        return Kind::Expr;
    }
    if t == ["string"] {
        return Kind::Text;
    }
    Kind::Json
}

fn value_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

impl FeatureDialog {
    /// Dialog for running command `id` (new feature), prefilled with `initial` params.
    pub fn new(s: &Session, id: &str, initial: &Value, editing: Option<FeatureId>) -> Option<Self> {
        let spec = s.registry().get(id)?.spec.clone();
        let props = spec.params.get("properties").and_then(Value::as_object).cloned().unwrap_or_default();
        let required: Vec<String> = spec
            .params
            .get("required")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default();
        let mut fields = Vec::new();
        for (key, sch) in &props {
            let kind = classify(key, sch);
            let init = initial.get(key).cloned().or_else(|| sch.get("default").cloned()).unwrap_or(Value::Null);
            let mut f = Field {
                key: key.clone(),
                label: label_of(key),
                help: sch.get("description").and_then(Value::as_str).unwrap_or("").to_string(),
                kind: kind.clone(),
                required: required.contains(key),
                text: String::new(),
                vec: [String::new(), String::new(), String::new()],
                flag: false,
            };
            match kind {
                Kind::Vec3 => {
                    if let Some(a) = init.as_array() {
                        for (slot, v) in f.vec.iter_mut().zip(a) {
                            *slot = value_text(v);
                        }
                    }
                }
                Kind::Bool => f.flag = init.as_bool().unwrap_or(false),
                Kind::Faces | Kind::Edges | Kind::Json => {
                    f.text = match &init {
                        Value::Null => String::new(),
                        // Plain selector names are shown without JSON quotes.
                        Value::String(t) => t.clone(),
                        other => other.to_string(),
                    }
                }
                _ => f.text = value_text(&init),
            }
            fields.push(f);
        }
        // Required fields first, then the rest (stable by schema order).
        fields.sort_by_key(|f| !f.required);
        Some(FeatureDialog {
            command: id.to_string(),
            title: if editing.is_some() { format!("Edit {}", spec.label) } else { spec.label.clone() },
            editing,
            fields,
            error: None,
            previewed: None,
        })
    }

    /// Params JSON from the fields (empty optional fields are omitted).
    pub fn params(&self) -> Result<Value, String> {
        let mut m = Map::new();
        for f in &self.fields {
            let v = match &f.kind {
                Kind::Bool => Some(json!(f.flag)),
                Kind::Vec3 => {
                    if f.vec.iter().all(|x| x.trim().is_empty()) {
                        None
                    } else {
                        Some(Value::Array(f.vec.iter().map(|x| num_or_expr(x)).collect()))
                    }
                }
                Kind::Expr => (!f.text.trim().is_empty()).then(|| num_or_expr(&f.text)),
                Kind::Body | Kind::Sketch => (!f.text.trim().is_empty()).then(|| num_or_expr(&f.text)),
                Kind::Choice(_) | Kind::Text => (!f.text.trim().is_empty()).then(|| json!(f.text.trim())),
                Kind::Faces | Kind::Edges | Kind::Json => {
                    let t = f.text.trim();
                    if t.is_empty() { None } else { Some(serde_json::from_str(t).unwrap_or_else(|_| json!(t))) }
                }
            };
            match v {
                Some(v) => {
                    m.insert(f.key.clone(), v);
                }
                None if f.required => return Err(format!("{} is required", f.label)),
                None => {}
            }
        }
        Ok(Value::Object(m))
    }

    /// The command and params to execute (new feature or edit).
    pub fn invocation(&self) -> Result<(String, Value), String> {
        let p = self.params()?;
        Ok(match self.editing {
            Some(id) => ("feature.edit".into(), json!({"feature": id.0, "params": p})),
            None => (self.command.clone(), p),
        })
    }

    /// Fill selector fields from the current selection (faces/edges become signatures).
    fn use_selection(&mut self, idx: usize, s: &Session) {
        let Some(f) = self.fields.get(idx) else { return };
        let k = s.kernel();
        let mut out = Vec::new();
        let mut body = None;
        for p in &s.selection().items {
            let Some(b) = s.model().body(p.body()) else { continue };
            match (p, &f.kind) {
                (Pick::Face { index, .. }, Kind::Faces) => {
                    if let Some(fi) = k.faces(&b.shape).ok().and_then(|v| v.into_iter().find(|x| x.index == *index)) {
                        out.push(json!({"face": formcraft_engine::regen::select::face_signature(&fi)}));
                        body = Some(b.id);
                    }
                }
                (Pick::Edge { index, .. }, Kind::Edges) => {
                    if let Some(ei) = k.edges(&b.shape).ok().and_then(|v| v.into_iter().find(|x| x.index == *index)) {
                        out.push(json!({"edge": formcraft_engine::regen::select::edge_signature(&ei)}));
                        body = Some(b.id);
                    }
                }
                (Pick::Face { index, .. }, Kind::Edges) => {
                    if let Some(fi) = k.faces(&b.shape).ok().and_then(|v| v.into_iter().find(|x| x.index == *index)) {
                        out.push(json!({"edges_of": {"face": formcraft_engine::regen::select::face_signature(&fi)}}));
                        body = Some(b.id);
                    }
                }
                _ => {}
            }
        }
        if out.is_empty() {
            self.error = Some("select faces or edges on the canvas first".into());
            return;
        }
        let text = if out.len() == 1 { out.remove(0).to_string() } else { Value::Array(out).to_string() };
        if let Some(f) = self.fields.get_mut(idx) {
            f.text = text;
        }
        // The selection also decides the body.
        if let Some(b) = body
            && let Some(bf) = self.fields.iter_mut().find(|x| x.key == "body")
        {
            bf.text = b.0.to_string();
        }
        self.error = None;
    }

    pub fn ui(&mut self, ui: &mut Ui, s: &Session, t: &Tokens) -> Outcome {
        let mut outcome = Outcome::None;
        let bodies: Vec<(String, String)> =
            s.model().bodies.iter().map(|b| (b.id.0.to_string(), b.name.clone())).collect();
        let sketches: Vec<(String, String)> = s
            .document()
            .active_features()
            .iter()
            .filter(|f| matches!(f.kind, formcraft_engine::doc::FeatureKind::Sketch { .. }))
            .map(|f| (f.id.0.to_string(), f.name.clone()))
            .collect();
        let mut use_sel = None;
        egui::Grid::new("feature_dialog_grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            for (i, f) in self.fields.iter_mut().enumerate() {
                let label = if f.required { RichText::new(&f.label).strong() } else { RichText::new(&f.label) };
                ui.label(label).on_hover_text(&f.help);
                match &f.kind {
                    Kind::Bool => {
                        ui.checkbox(&mut f.flag, "");
                    }
                    Kind::Vec3 => {
                        ui.horizontal(|ui| {
                            for v in &mut f.vec {
                                ui.add(egui::TextEdit::singleline(v).desired_width(52.0));
                            }
                        });
                    }
                    Kind::Choice(opts) => {
                        egui::ComboBox::from_id_salt(("choice", i)).selected_text(&f.text).show_ui(ui, |ui| {
                            for o in opts {
                                ui.selectable_value(&mut f.text, o.clone(), o);
                            }
                        });
                    }
                    Kind::Body | Kind::Sketch => {
                        let list = if f.kind == Kind::Body { &bodies } else { &sketches };
                        let shown =
                            list.iter()
                                .find(|(id, _)| *id == f.text)
                                .map(|(_, n)| n.clone())
                                .unwrap_or_else(|| if f.text.is_empty() { "(default)".into() } else { f.text.clone() });
                        egui::ComboBox::from_id_salt(("pick", i)).selected_text(shown).show_ui(ui, |ui| {
                            if !f.required {
                                ui.selectable_value(&mut f.text, String::new(), "(default)");
                            }
                            for (id, name) in list {
                                ui.selectable_value(&mut f.text, id.clone(), name);
                            }
                        });
                    }
                    Kind::Faces | Kind::Edges => {
                        ui.horizontal(|ui| {
                            let short = if f.text.len() > 28 { "(selection)".to_string() } else { f.text.clone() };
                            let mut shown = short.clone();
                            let r = ui
                                .add(egui::TextEdit::singleline(&mut shown).desired_width(130.0).hint_text("e.g. top"));
                            if r.changed() {
                                f.text = shown;
                            }
                            if ui
                                .small_button("Use")
                                .on_hover_text("Use the faces/edges selected on the canvas")
                                .clicked()
                            {
                                use_sel = Some(i);
                            }
                        });
                    }
                    Kind::Expr | Kind::Text | Kind::Json => {
                        ui.add(egui::TextEdit::singleline(&mut f.text).desired_width(170.0));
                    }
                }
                ui.end_row();
            }
        });
        if let Some(i) = use_sel {
            self.use_selection(i, s);
        }
        if let Some(e) = &self.error {
            ui.add_space(4.0);
            ui.label(RichText::new(e).color(t.error));
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let ok = ui.add(
                egui::Button::new(RichText::new("OK").color(Color32::WHITE))
                    .fill(t.accent)
                    .min_size(egui::vec2(70.0, 24.0)),
            );
            if ok.clicked() {
                outcome = Outcome::Ok;
            }
            if ui.add(egui::Button::new("Cancel").min_size(egui::vec2(70.0, 24.0))).clicked() {
                outcome = Outcome::Cancel;
            }
        });
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            outcome = Outcome::Cancel;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Enter)) && !ui.ctx().egui_wants_keyboard_input() {
            outcome = Outcome::Ok;
        }
        outcome
    }
}

/// Numbers stay numbers; anything else is an expression string.
fn num_or_expr(s: &str) -> Value {
    let t = s.trim();
    match t.parse::<f64>() {
        Ok(v) if v.is_finite() => json!(v),
        _ => json!(t),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_from_schema_round_trips_params() {
        let mut s = Session::new();
        s.execute("solid.box", &json!({"size": [10, 10, 10]})).unwrap();
        let d = FeatureDialog::new(&s, "solid.fillet", &json!({"edges": "vertical", "radius": "2"}), None).unwrap();
        assert_eq!(d.fields[0].key, "radius", "required first");
        let (cmd, p) = d.invocation().unwrap();
        assert_eq!(cmd, "solid.fillet");
        assert_eq!(p["radius"], 2.0);
        assert_eq!(p["edges"], "vertical");
        assert!(s.execute(&cmd, &p).is_ok());

        let ex = FeatureDialog::new(&s, "solid.box", &json!({}), None).unwrap();
        assert!(ex.params().is_err(), "size is required");
        let kinds: Vec<(String, Kind)> = ex.fields.iter().map(|f| (f.key.clone(), f.kind.clone())).collect();
        assert!(kinds.contains(&("size".into(), Kind::Vec3)));
        assert!(kinds.iter().any(|(k, v)| k == "operation" && matches!(v, Kind::Choice(_))));
        assert!(kinds.contains(&("target".into(), Kind::Body)));
    }

    #[test]
    fn edit_invocation_wraps_feature_edit() {
        let mut s = Session::new();
        let r = s.execute("solid.box", &json!({"size": [10, 10, 10]})).unwrap();
        let id = FeatureId(r["feature"].as_u64().unwrap() as u32);
        let p = s.execute("feature.params", &json!({"feature": id.0})).unwrap();
        let mut d = FeatureDialog::new(&s, "solid.box", &p["params"], Some(id)).unwrap();
        if let Some(f) = d.fields.iter_mut().find(|f| f.key == "size") {
            f.vec[2] = "L/2".into();
        }
        let (cmd, p) = d.invocation().unwrap();
        assert_eq!(cmd, "feature.edit");
        assert_eq!(p["params"]["size"], json!([10.0, 10.0, "L/2"]));
    }
}
