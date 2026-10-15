//! Marking menu (right-click radial menu), shortcut box (S) and the parameters table.

use egui::{Align2, FontId, Id, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};
use formcraft_engine::{CommandSpec, Session};
use serde_json::{Value, json};

use crate::icons;
use crate::theme::Tokens;

/// Radial slots, clockwise from the top. Ids that are not registered are skipped.
const RADIAL: [&str; 8] = [
    "repeat",
    "solid.press_pull",
    "solid.extrude",
    "solid.hole",
    "solid.fillet",
    "sketch.create",
    "doc.undo",
    "ai.modify",
];
/// The list under the radial menu.
const LIST: [&str; 7] =
    ["solid.chamfer", "solid.shell", "solid.combine", "doc.parameters", "model.inspect", "doc.redo", "ai.generate"];

pub enum MenuAction {
    Run(String),
    Close,
}

fn label_for(s: &Session, id: &str, last: Option<&str>) -> Option<(String, String)> {
    if id == "repeat" {
        let last = last?;
        let spec = s.registry().get(last)?;
        return Some((
            format!("Repeat {}", spec.spec.label),
            spec.spec.icon.clone().unwrap_or_else(|| "plugin".into()),
        ));
    }
    let spec = s.registry().get(id)?;
    Some((spec.spec.label.clone(), spec.spec.icon.clone().unwrap_or_else(|| "plugin".into())))
}

/// Draw the marking menu centred at `at`. Returns an action when the user picks something.
pub fn marking_menu(ui: &mut Ui, t: &Tokens, s: &Session, at: Pos2, last: Option<&str>) -> Option<MenuAction> {
    let mut out = None;
    let radius = 78.0;
    egui::Area::new(Id::new("marking_menu"))
        .fixed_pos(at - Vec2::splat(radius + 70.0))
        .order(egui::Order::Foreground)
        .show(ui.ctx(), |ui| {
            let size = Vec2::splat((radius + 70.0) * 2.0);
            let (rect, _) = ui.allocate_exact_size(size + Vec2::new(0.0, 170.0), Sense::hover());
            let c = rect.min + Vec2::splat(radius + 70.0);
            let p = ui.painter();
            p.circle_filled(c, 7.0, t.accent.gamma_multiply(0.6));
            p.circle_stroke(c, radius * 0.45, Stroke::new(1.0, t.separator));
            for (i, id) in RADIAL.iter().enumerate() {
                let Some((label, icon)) = label_for(s, id, last) else { continue };
                // Compass slots: N, NE, E, SE, S, SW, W, NW. Right-side pills grow to the right,
                // left-side pills to the left, so neighbours never overlap.
                let (dx, dy) = [
                    (0.0, -1.0),
                    (0.75, -0.6),
                    (0.9, 0.0),
                    (0.75, 0.6),
                    (0.0, 1.0),
                    (-0.75, 0.6),
                    (-0.9, 0.0),
                    (-0.75, -0.6),
                ][i];
                let anchor = c + Vec2::new(dx, dy) * radius;
                let w = 128.0;
                let min_x = if dx > 0.1 {
                    anchor.x
                } else if dx < -0.1 {
                    anchor.x - w
                } else {
                    anchor.x - w / 2.0
                };
                let r = Rect::from_min_size(Pos2::new(min_x, anchor.y - 13.0), Vec2::new(w, 26.0));
                let resp = ui.interact(r, Id::new(("mm", i)), Sense::click());
                let fill = if resp.hovered() { t.selection } else { t.panel };
                p.rect_filled(r, 13.0, fill);
                p.rect_stroke(r, 13.0, Stroke::new(1.0, t.separator), egui::StrokeKind::Inside);
                icons::paint(
                    p,
                    Rect::from_center_size(r.left_center() + Vec2::new(16.0, 0.0), Vec2::splat(16.0)),
                    &icon,
                    t.text,
                    t,
                );
                p.text(
                    r.left_center() + Vec2::new(30.0, 0.0),
                    Align2::LEFT_CENTER,
                    label,
                    FontId::proportional(12.0),
                    t.text,
                );
                if resp.clicked() {
                    out = Some(MenuAction::Run(if *id == "repeat" {
                        last.unwrap_or_default().to_string()
                    } else {
                        id.to_string()
                    }));
                }
            }
            // Overflow list below.
            let mut y = c.y + radius + 30.0;
            let x = c.x - 80.0;
            let mut any = false;
            for id in LIST {
                let Some((label, icon)) = label_for(s, id, last) else { continue };
                any = true;
                let r = Rect::from_min_size(Pos2::new(x, y), Vec2::new(160.0, 22.0));
                let resp = ui.interact(r, Id::new(("ml", id)), Sense::click());
                p.rect_filled(r, 0.0, if resp.hovered() { t.selection } else { t.panel });
                icons::paint(
                    p,
                    Rect::from_center_size(r.left_center() + Vec2::new(12.0, 0.0), Vec2::splat(14.0)),
                    &icon,
                    t.text,
                    t,
                );
                p.text(
                    r.left_center() + Vec2::new(26.0, 0.0),
                    Align2::LEFT_CENTER,
                    label,
                    FontId::proportional(12.0),
                    t.text,
                );
                if resp.clicked() {
                    out = Some(MenuAction::Run(id.to_string()));
                }
                y += 22.0;
            }
            if any {
                p.rect_stroke(
                    Rect::from_min_max(Pos2::new(x, c.y + radius + 30.0), Pos2::new(x + 160.0, y)),
                    0.0,
                    Stroke::new(1.0, t.separator),
                    egui::StrokeKind::Outside,
                );
            }
        });
    let (esc, clicked_outside) = ui.input(|i| (i.key_pressed(egui::Key::Escape), i.pointer.any_click()));
    if out.is_none() && (esc || clicked_outside) {
        // A click that did not hit an item closes the menu (the frame after it opened).
        return Some(MenuAction::Close);
    }
    out
}

/// One row of the shortcut box.
pub struct Hit {
    pub id: String,
    pub label: String,
    pub icon: String,
    pub shortcut: Option<String>,
}

pub fn search(s: &Session, text: &str) -> Vec<Hit> {
    s.registry()
        .search(text)
        .into_iter()
        .filter(|c| c.placement.is_some() || c.undoable || c.id.starts_with("doc.") || c.id.starts_with("view."))
        .take(9)
        .map(|c: &CommandSpec| Hit {
            id: c.id.clone(),
            label: c.label.clone(),
            icon: c.icon.clone().unwrap_or_else(|| "plugin".into()),
            shortcut: c.shortcut.clone(),
        })
        .collect()
}

pub enum BoxAction {
    Run(String),
    /// No command matched: ask the AI to do it (Phase 5).
    Ai(String),
    Close,
}

/// The shortcut box (S): type to find a command.
pub fn shortcut_box(ui: &mut Ui, t: &Tokens, s: &Session, text: &mut String, at: Pos2) -> Option<BoxAction> {
    let mut out = None;
    egui::Area::new(Id::new("shortcut_box")).fixed_pos(at).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        egui::Frame::popup(ui.style()).inner_margin(6).show(ui, |ui| {
            ui.set_width(300.0);
            let r = ui.add(egui::TextEdit::singleline(text).hint_text("Search commands…").desired_width(f32::INFINITY));
            r.request_focus();
            let hits = search(s, text);
            let ai = s.registry().get("ai.generate").is_some() && !text.trim().is_empty();
            for (i, h) in hits.iter().enumerate() {
                let row = ui.horizontal(|ui| {
                    let (ir, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                    icons::paint(ui.painter(), ir, &h.icon, t.text, t);
                    let lr = ui.selectable_label(i == 0, &h.label);
                    if let Some(k) = &h.shortcut {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(k).color(t.text_dim).monospace());
                        });
                    }
                    lr
                });
                if row.inner.clicked() {
                    out = Some(BoxAction::Run(h.id.clone()));
                }
            }
            let exact = hits.first().is_some_and(|h| h.label.eq_ignore_ascii_case(text.trim()));
            if ai && !exact {
                ui.separator();
                let lr = ui.selectable_label(
                    hits.is_empty(),
                    RichText::new(format!("AI  Сделать с ИИ: {}", text.trim())).color(t.ai),
                );
                if lr.clicked() {
                    out = Some(BoxAction::Ai(text.trim().to_string()));
                }
            }
            if hits.is_empty() && !ai && !text.trim().is_empty() {
                ui.label(RichText::new("No matching command").color(t.text_dim));
            }
            if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                out = Some(match hits.first() {
                    Some(h) if exact || !ai => BoxAction::Run(h.id.clone()),
                    _ if ai => BoxAction::Ai(text.trim().to_string()),
                    Some(h) => BoxAction::Run(h.id.clone()),
                    None => BoxAction::Close,
                });
            }
        });
    });
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        out = Some(BoxAction::Close);
    }
    out
}

/// Editable parameter table (Change Parameters). Returns commands to run.
pub struct ParamRow {
    pub name: String,
    pub expr: String,
    pub comment: String,
    pub value: Option<f64>,
    pub existing: bool,
}

pub fn parameters_window(ui: &mut Ui, t: &Tokens, rows: &mut Vec<ParamRow>, open: &mut bool) -> Vec<(String, Value)> {
    let mut cmds = Vec::new();
    egui::Window::new("Parameters").id(Id::new("parameters")).open(open).default_width(520.0).show(ui.ctx(), |ui| {
        egui::Grid::new("param_grid").num_columns(5).striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
            for h in ["Name", "Expression", "Value", "Comment", ""] {
                ui.label(RichText::new(h).strong());
            }
            ui.end_row();
            let mut delete = None;
            for (i, r) in rows.iter_mut().enumerate() {
                if r.existing {
                    ui.label(&r.name);
                } else {
                    ui.add(egui::TextEdit::singleline(&mut r.name).desired_width(110.0).hint_text("name"));
                }
                let e = ui.add(egui::TextEdit::singleline(&mut r.expr).desired_width(130.0));
                ui.label(r.value.map(|v| format!("{v:.4}")).unwrap_or_else(|| "—".into()));
                let c = ui.add(egui::TextEdit::singleline(&mut r.comment).desired_width(140.0));
                if ui.small_button("x").on_hover_text("Delete").clicked() {
                    delete = Some(i);
                }
                if (e.lost_focus() || c.lost_focus()) && !r.name.trim().is_empty() && !r.expr.trim().is_empty() {
                    cmds.push((
                        "doc.parameter".to_string(),
                        json!({"name": r.name.trim(), "expression": r.expr.trim(), "comment": r.comment}),
                    ));
                }
                ui.end_row();
            }
            if let Some(i) = delete
                && let Some(r) = rows.get(i)
            {
                if r.existing {
                    cmds.push(("doc.delete_parameter".to_string(), json!({"name": r.name})));
                } else {
                    rows.remove(i);
                }
            }
        });
        ui.add_space(6.0);
        if ui.button("+ Add user parameter").clicked() {
            rows.push(ParamRow {
                name: String::new(),
                expr: "0".into(),
                comment: String::new(),
                value: None,
                existing: false,
            });
        }
        ui.label(
            RichText::new("Expressions may use other parameters, units (mm, cm, in, deg) and functions.")
                .color(t.text_dim)
                .size(11.0),
        );
    });
    cmds
}

pub fn rows_from(s: &Session) -> Vec<ParamRow> {
    let d = s.document();
    let vals = d.parameter_values().unwrap_or_default();
    d.parameters
        .iter()
        .map(|p| ParamRow {
            name: p.name.clone(),
            expr: p.expr.as_str().to_string(),
            comment: p.comment.clone(),
            value: vals.get(&p.name).copied(),
            existing: true,
        })
        .collect()
}

/// Parse "Ctrl+Shift+S" / "E" into (modifiers, key).
pub fn parse_shortcut(s: &str) -> Option<(egui::Modifiers, egui::Key)> {
    let mut m = egui::Modifiers::NONE;
    let mut key = None;
    for part in s.split('+') {
        match part.trim().to_ascii_lowercase().as_str() {
            "ctrl" | "cmd" => m |= egui::Modifiers::COMMAND,
            "shift" => m |= egui::Modifiers::SHIFT,
            "alt" => m |= egui::Modifiers::ALT,
            k => key = egui::Key::from_name(&k.to_ascii_uppercase()).or_else(|| egui::Key::from_name(k)),
        }
    }
    key.map(|k| (m, k))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_parse() {
        assert_eq!(parse_shortcut("E"), Some((egui::Modifiers::NONE, egui::Key::E)));
        assert_eq!(parse_shortcut("Ctrl+S"), Some((egui::Modifiers::COMMAND, egui::Key::S)));
        assert_eq!(parse_shortcut("Ctrl+Shift+Z").map(|x| x.1), Some(egui::Key::Z));
        assert_eq!(parse_shortcut("Nope+"), None);
    }

    #[test]
    fn search_finds_modelling_commands() {
        let s = Session::new();
        let hits = search(&s, "fil");
        assert_eq!(hits.first().map(|h| h.id.as_str()), Some("solid.fillet"));
        assert!(search(&s, "zzzz").is_empty());
    }
}
