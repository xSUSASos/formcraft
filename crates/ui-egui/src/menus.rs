//! Marking menu (right-click radial menu, UI_SPEC.md §5.10), shortcut box (S, §5.11) and the
//! parameters table.

use egui::{Align2, Color32, CornerRadius, FontId, Id, Pos2, Rect, RichText, Sense, Shape, Stroke, Ui, Vec2};
use formcraft_engine::{CommandSpec, Session};
use serde_json::{Value, json};

use crate::icons;
use crate::theme::{SIZE, Tokens};

/// Radial sectors, clockwise from the top. Ids that are not registered are left empty.
const RADIAL: [&str; 8] = [
    "solid.press_pull",
    "solid.extrude",
    "solid.hole",
    "solid.fillet",
    "doc.undo",
    "sketch.create",
    "solid.chamfer",
    "solid.shell",
];
/// The list under the ring (context-dependent entries are added in front).
const LIST: [&str; 5] = ["solid.combine", "doc.parameters", "model.inspect", "doc.redo", "ai.generate"];

pub enum MenuAction {
    Run(String),
    /// Open the command's dialog with these initial parameters.
    Dialog(String, Value),
    Close,
}

fn label_for(s: &Session, id: &str) -> Option<(String, String)> {
    let spec = s.registry().get(id)?;
    Some((spec.spec.label.clone(), spec.spec.icon.clone().unwrap_or_else(|| "plugin".into())))
}

/// Sector index under `p` (0 = top, clockwise), `None` inside the centre circle.
pub fn sector_at(c: Pos2, p: Pos2, inner: f32) -> Option<usize> {
    let d = p - c;
    if d.length() < inner {
        return None;
    }
    // angle from the top, clockwise, in eighths with the top sector centred on 0
    let a = d.x.atan2(-d.y).rem_euclid(std::f32::consts::TAU);
    Some(((a / (std::f32::consts::TAU / 8.0) + 0.5).floor() as usize) % 8)
}

fn ring_sector(c: Pos2, r0: f32, r1: f32, k: usize) -> Vec<Pos2> {
    let step = std::f32::consts::TAU / 8.0;
    let (a0, a1) = (k as f32 * step - step / 2.0, k as f32 * step + step / 2.0);
    let n = 10;
    let at = |r: f32, a: f32| c + Vec2::new(a.sin(), -a.cos()) * r;
    let mut v: Vec<Pos2> = (0..=n).map(|i| at(r1, a0 + (a1 - a0) * i as f32 / n as f32)).collect();
    v.extend((0..=n).rev().map(|i| at(r0, a0 + (a1 - a0) * i as f32 / n as f32)));
    v
}

/// Draw the marking menu centred at `at`. Returns an action when the user picks something.
pub fn marking_menu(ui: &mut Ui, t: &Tokens, s: &Session, at: Pos2, last: Option<&str>) -> Option<MenuAction> {
    let mut out = None;
    let r1 = SIZE.marking_radius;
    let r0 = r1 * 0.32;
    let pad = r1 + SIZE.grid * 30.0;
    let has_face = s.selection().items.iter().any(|p| matches!(p, formcraft_engine::Pick::Face { .. }));
    egui::Area::new(Id::new("marking_menu")).fixed_pos(at - Vec2::splat(pad)).order(egui::Order::Foreground).show(
        ui.ctx(),
        |ui| {
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(pad * 2.0, pad * 2.0 + SIZE.tree_row_h * 9.0), Sense::hover());
            let c = rect.min + Vec2::splat(pad);
            let hover = ui.input(|i| i.pointer.hover_pos());
            let hovered = hover.and_then(|h| sector_at(c, h, r0));
            let p = ui.painter();
            let font = FontId::proportional(SIZE.font_panel);
            // the ring
            for k in 0..8 {
                let on = hovered == Some(k) && RADIAL.get(k).and_then(|id| label_for(s, id)).is_some();
                let fill = if on { t.accent } else { t.panel_bg };
                p.add(Shape::convex_polygon(ring_sector(c, r0, r1, k), fill, Stroke::new(1.0, t.divider)));
            }
            for (k, id) in RADIAL.iter().enumerate() {
                let Some((label, icon)) = label_for(s, id) else { continue };
                let on = hovered == Some(k);
                let a = k as f32 * std::f32::consts::TAU / 8.0;
                let dir = Vec2::new(a.sin(), -a.cos());
                let ic = c + dir * (r0 + r1) / 2.0;
                let color = if on { Color32::WHITE } else { t.icon_outline };
                icons::paint(p, Rect::from_center_size(ic, Vec2::splat(SIZE.icon_nav)), &icon, color, t);
                // the caption sits outside the ring, pointing away from the centre
                let anchor = c + dir * (r1 + SIZE.grid * 2.0);
                let align = if dir.x > 0.3 {
                    Align2::LEFT_CENTER
                } else if dir.x < -0.3 {
                    Align2::RIGHT_CENTER
                } else if dir.y < 0.0 {
                    Align2::CENTER_BOTTOM
                } else {
                    Align2::CENTER_TOP
                };
                let galley = p.layout_no_wrap(label, font.clone(), if on { Color32::WHITE } else { t.text_primary });
                let tr = align.anchor_size(anchor, galley.size()).expand2(Vec2::new(SIZE.grid * 2.0, SIZE.grid));
                p.rect_filled(tr, CornerRadius::same(SIZE.radius_card), if on { t.accent } else { t.panel_bg });
                p.rect_stroke(
                    tr,
                    CornerRadius::same(SIZE.radius_card),
                    Stroke::new(1.0, t.divider),
                    egui::StrokeKind::Inside,
                );
                p.galley(tr.min + Vec2::new(SIZE.grid * 2.0, SIZE.grid), galley, t.text_primary);
                let clicked_ring = on && ui.input(|i| i.pointer.primary_clicked());
                let clicked_label = ui.interact(tr, Id::new(("mm", k)), Sense::click()).clicked();
                if clicked_ring || clicked_label {
                    out = Some(MenuAction::Run(id.to_string()));
                }
            }
            // centre: repeat the last command
            let centre = Rect::from_center_size(c, Vec2::splat(r0 * 2.0));
            let cr = ui.interact(centre, Id::new("mm_repeat"), Sense::click());
            let last_spec = last.and_then(|l| s.registry().get(l)).map(|c| c.spec.clone());
            p.circle_filled(
                c,
                r0 - 2.0,
                if cr.hovered() && last_spec.is_some() { t.toolbar_hover } else { t.panel_bg },
            );
            p.circle_stroke(c, r0 - 2.0, Stroke::new(1.0, t.divider));
            match &last_spec {
                Some(spec) => {
                    let icon = spec.icon.clone().unwrap_or_else(|| "plugin".into());
                    icons::paint(p, Rect::from_center_size(c, Vec2::splat(SIZE.icon_nav)), &icon, t.icon_outline, t);
                    if cr.on_hover_text(format!("Repeat {}", spec.label)).clicked() {
                        out = Some(MenuAction::Run(spec.id.clone()));
                    }
                }
                None => {
                    p.circle_filled(c, SIZE.grid, t.text_disabled);
                }
            }
            // the list under the ring
            let w = SIZE.grid * 48.0;
            let mut y = c.y + r1 + SIZE.grid * 9.0;
            let x = c.x - w / 2.0;
            let top = y;
            let mut rows: Vec<(String, String, String, Option<Value>)> = Vec::new();
            if has_face {
                if s.registry().get("ai.generate").is_some() {
                    rows.push((
                        "ai.generate".into(),
                        "Edit with AI…".into(),
                        "ai".into(),
                        Some(json!({"use_selection": true})),
                    ));
                }
                if s.registry().get("sketch.create").is_some() {
                    rows.push(("sketch.create".into(), "Create Sketch on Face".into(), "sketch".into(), None));
                }
            }
            for id in LIST {
                if let Some((label, icon)) = label_for(s, id) {
                    rows.push((id.to_string(), label, icon, None));
                }
            }
            let card_bottom = top + rows.len() as f32 * SIZE.tree_row_h;
            if !rows.is_empty() {
                let card = Rect::from_min_max(Pos2::new(x, top), Pos2::new(x + w, card_bottom)).expand(SIZE.grid);
                p.rect_filled(card, CornerRadius::same(SIZE.radius_card), t.panel_bg);
                p.rect_stroke(
                    card,
                    CornerRadius::same(SIZE.radius_card),
                    Stroke::new(1.0, t.divider),
                    egui::StrokeKind::Outside,
                );
            }
            for (id, label, icon, params) in rows {
                let r = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, SIZE.tree_row_h));
                let resp = ui.interact(r, Id::new(("ml", &label)), Sense::click());
                if resp.hovered() {
                    p.rect_filled(r, CornerRadius::same(SIZE.radius_button), t.toolbar_hover);
                }
                let ai = id.starts_with("ai.");
                icons::paint(
                    p,
                    Rect::from_center_size(
                        r.left_center() + Vec2::new(SIZE.grid * 3.0, 0.0),
                        Vec2::splat(SIZE.icon_menu),
                    ),
                    &icon,
                    t.icon_outline,
                    t,
                );
                p.text(
                    r.left_center() + Vec2::new(SIZE.grid * 7.0, 0.0),
                    Align2::LEFT_CENTER,
                    label,
                    font.clone(),
                    if ai { t.ai } else { t.text_primary },
                );
                if resp.clicked() {
                    out = Some(match params {
                        Some(v) => MenuAction::Dialog(id, v),
                        None => MenuAction::Run(id),
                    });
                }
                y += SIZE.tree_row_h;
            }
        },
    );
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
    /// "Solid › Create".
    pub path: String,
}

fn hit(c: &CommandSpec) -> Hit {
    Hit {
        id: c.id.clone(),
        label: c.label.clone(),
        icon: c.icon.clone().unwrap_or_else(|| "plugin".into()),
        shortcut: c.shortcut.clone(),
        path: c.placement.as_ref().map(|p| format!("{} › {}", p.tab, p.group)).unwrap_or_default(),
    }
}

pub fn search(s: &Session, text: &str) -> Vec<Hit> {
    s.registry()
        .search(text)
        .into_iter()
        .filter(|c| c.placement.is_some() || c.undoable || c.id.starts_with("doc.") || c.id.starts_with("view."))
        .take(9)
        .map(hit)
        .collect()
}

pub enum BoxAction {
    Run(String),
    /// No command matched: ask the AI to do it.
    Ai(String),
    Close,
}

/// The shortcut box (S): a 360px card at the cursor with the search field, a 6×2 grid of
/// favourites, results (icon, name, path, shortcut, pin) and the AI line in purple.
pub fn shortcut_box(
    ui: &mut Ui,
    t: &Tokens,
    s: &Session,
    text: &mut String,
    at: Pos2,
    favorites: &mut Vec<String>,
) -> Option<BoxAction> {
    let mut out = None;
    let g = SIZE.grid;
    egui::Area::new(Id::new("shortcut_box")).fixed_pos(at).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        crate::theme::card(t).show(ui, |ui| {
            ui.set_width(SIZE.sbox_w - g * 4.0);
            let r = ui.add(egui::TextEdit::singleline(text).hint_text("Search commands…").desired_width(f32::INFINITY));
            r.request_focus();
            ui.add_space(g);
            let hits = search(s, text);
            let ai = s.registry().get("ai.generate").is_some() && !text.trim().is_empty();
            if text.trim().is_empty() {
                // favourites grid
                let cell = (SIZE.sbox_w - g * 4.0) / 6.0;
                for row in favorites.chunks(6).take(2) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        for id in row {
                            let Some(spec) = s.registry().get(id).map(|c| c.spec.clone()) else { continue };
                            let (rr, resp) = ui.allocate_exact_size(Vec2::new(cell, cell * 0.8), Sense::click());
                            if resp.hovered() {
                                ui.painter().rect_filled(rr, CornerRadius::same(SIZE.radius_button), t.toolbar_hover);
                            }
                            let icon = spec.icon.clone().unwrap_or_else(|| "plugin".into());
                            icons::paint(
                                ui.painter(),
                                Rect::from_center_size(rr.center(), Vec2::splat(SIZE.icon_timeline)),
                                &icon,
                                t.icon_outline,
                                t,
                            );
                            if resp.on_hover_text(&spec.label).clicked() {
                                out = Some(BoxAction::Run(id.clone()));
                            }
                        }
                    });
                }
            }
            let mut toggle_pin = None;
            for (i, h) in hits.iter().enumerate() {
                let (rr, resp) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), SIZE.tree_row_h + g), Sense::click());
                if resp.hovered() || i == 0 {
                    ui.painter().rect_filled(rr, CornerRadius::same(SIZE.radius_button), t.toolbar_hover);
                }
                let p = ui.painter();
                let ir =
                    Rect::from_center_size(rr.left_center() + Vec2::new(g * 3.0, 0.0), Vec2::splat(SIZE.icon_menu));
                icons::paint(p, ir, &h.icon, t.icon_outline, t);
                let font = FontId::proportional(SIZE.font_panel);
                let name = p.layout_no_wrap(h.label.clone(), font.clone(), t.text_primary);
                let name_w = name.size().x;
                p.galley(rr.left_center() + Vec2::new(g * 7.0, -name.size().y / 2.0), name, t.text_primary);
                p.text(
                    rr.left_center() + Vec2::new(g * 9.0 + name_w, 0.0),
                    Align2::LEFT_CENTER,
                    &h.path,
                    FontId::proportional(SIZE.font_group),
                    t.text_secondary,
                );
                // pin (right) and shortcut
                let pin =
                    Rect::from_center_size(rr.right_center() - Vec2::new(g * 3.0, 0.0), Vec2::splat(SIZE.icon_menu));
                let pinned = favorites.contains(&h.id);
                let pr = ui.interact(pin, Id::new(("pin", &h.id)), Sense::click());
                if pinned || pr.hovered() || resp.hovered() {
                    p.circle_stroke(
                        pin.center(),
                        g * 1.25,
                        Stroke::new(1.0, if pinned { t.accent } else { t.text_secondary }),
                    );
                    if pinned {
                        p.circle_filled(pin.center(), g * 0.75, t.accent);
                    }
                }
                if pr.on_hover_text(if pinned { "Unpin" } else { "Pin to favourites" }).clicked() {
                    toggle_pin = Some(h.id.clone());
                } else if resp.clicked() {
                    out = Some(BoxAction::Run(h.id.clone()));
                }
                if let Some(k) = &h.shortcut {
                    p.text(
                        pin.left_center() - Vec2::new(g * 2.0, 0.0),
                        Align2::RIGHT_CENTER,
                        k,
                        font,
                        t.text_secondary,
                    );
                }
            }
            if let Some(id) = toggle_pin {
                if let Some(i) = favorites.iter().position(|f| *f == id) {
                    favorites.remove(i);
                } else if favorites.len() < 12 {
                    favorites.push(id);
                }
            }
            let exact = hits.first().is_some_and(|h| h.label.eq_ignore_ascii_case(text.trim()));
            if ai && !exact {
                let (rr, resp) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), SIZE.tree_row_h + g), Sense::click());
                if resp.hovered() || hits.is_empty() {
                    ui.painter().rect_filled(rr, CornerRadius::same(SIZE.radius_button), t.ai.gamma_multiply(0.12));
                }
                let ir =
                    Rect::from_center_size(rr.left_center() + Vec2::new(g * 3.0, 0.0), Vec2::splat(SIZE.icon_menu));
                icons::paint(ui.painter(), ir, "ai", t.ai, t);
                ui.painter().text(
                    rr.left_center() + Vec2::new(g * 7.0, 0.0),
                    Align2::LEFT_CENTER,
                    format!("Сделать с ИИ: «{}»", text.trim()),
                    FontId::proportional(SIZE.font_panel),
                    t.ai,
                );
                if resp.clicked() {
                    out = Some(BoxAction::Ai(text.trim().to_string()));
                }
            }
            if hits.is_empty() && !ai && !text.trim().is_empty() {
                ui.label(RichText::new("No matching command").color(t.text_secondary));
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
                .color(t.text_secondary)
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
    fn sectors_go_clockwise_from_the_top() {
        let c = Pos2::new(100.0, 100.0);
        assert_eq!(sector_at(c, Pos2::new(100.0, 10.0), 20.0), Some(0));
        assert_eq!(sector_at(c, Pos2::new(190.0, 100.0), 20.0), Some(2));
        assert_eq!(sector_at(c, Pos2::new(100.0, 190.0), 20.0), Some(4));
        assert_eq!(sector_at(c, Pos2::new(10.0, 100.0), 20.0), Some(6));
        assert_eq!(sector_at(c, Pos2::new(105.0, 95.0), 20.0), None);
    }

    #[test]
    fn search_finds_modelling_commands() {
        let s = Session::new();
        let hits = search(&s, "fil");
        assert_eq!(hits.first().map(|h| h.id.as_str()), Some("solid.fillet"));
        assert!(search(&s, "zzzz").is_empty());
    }
}
