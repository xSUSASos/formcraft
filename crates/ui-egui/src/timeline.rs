//! Timeline (UI_SPEC.md §5.7): the feature history as 24px icons. Double-click edits,
//! right-click opens a menu, dragging a feature reorders it, dragging the marker rolls the model
//! back. A feature group (e.g. one AI generation) is a folder icon with ✦ that expands into its
//! features under a bracket.

use egui::{Align2, CornerRadius, FontId, Id, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2};
use formcraft_engine::Session;
use formcraft_engine::doc::FeatureId;
use formcraft_engine::regen::Status;

use crate::icons;
use crate::theme::{SIZE, Tokens};

pub enum Action {
    Edit(FeatureId),
    Rename(FeatureId),
    Suppress(FeatureId, bool),
    Delete(FeatureId),
    Reorder(FeatureId, usize),
    Marker(Option<usize>),
    Explain(FeatureId),
    Settings,
}

pub fn icon_for(base: &str) -> &'static str {
    match base {
        "Box" => "box",
        "Cylinder" => "cylinder",
        "Sphere" => "sphere",
        "Combine" => "combine",
        "Sketch" => "sketch",
        "Extrude" => "extrude",
        "Revolve" => "revolve",
        "Fillet" => "fillet",
        "Chamfer" => "chamfer",
        "Shell" => "shell",
        "Hole" => "hole",
        "PressPull" => "press_pull",
        _ => "plugin",
    }
}

/// One position on the strip: a feature, or a collapsed group of consecutive features.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Slot {
    Feature(usize),
    Group { id: u32, first: usize, last: usize },
}

impl Slot {
    fn first(&self) -> usize {
        match *self {
            Slot::Feature(i) => i,
            Slot::Group { first, .. } => first,
        }
    }
}

fn expanded_id(g: u32) -> Id {
    Id::new(("tl_group_open", g))
}

/// Slots for the features; groups are collapsed unless the user opened them.
fn slots(ui: &Ui, s: &Session) -> Vec<Slot> {
    let d = s.document();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(f) = d.features.get(i) {
        match f.group {
            Some(g) if !ui.data(|m| m.get_temp::<bool>(expanded_id(g)).unwrap_or(false)) => {
                let mut last = i;
                while d.features.get(last + 1).is_some_and(|n| n.group == Some(g)) {
                    last += 1;
                }
                out.push(Slot::Group { id: g, first: i, last });
                i = last + 1;
            }
            _ => {
                out.push(Slot::Feature(i));
                i += 1;
            }
        }
    }
    out
}

/// Draw the timeline row; returns what the user asked for.
pub fn ui(ui: &mut Ui, t: &Tokens, s: &Session) -> Option<Action> {
    let mut action = None;
    let d = s.document();
    let n = d.features.len();
    let active = d.active_features().len();
    let marker = d.marker;
    let tile = SIZE.icon_timeline + SIZE.grid;
    let gap = SIZE.timeline_gap;
    let nav = SIZE.icon_menu + SIZE.grid;
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = SIZE.grid / 2.0;
        // Playback: beginning, step back, step forward, end.
        let at = marker.unwrap_or(n);
        let can_back = at > 0;
        let can_fwd = at < n;
        if icons::button(ui, t, "tl_start", "Move to beginning", nav, can_back, false).clicked() {
            action = Some(Action::Marker(Some(0)));
        }
        if icons::button(ui, t, "tl_back", "Step back", nav, can_back, false).clicked() {
            action = Some(Action::Marker(Some(at.saturating_sub(1))));
        }
        if icons::button(ui, t, "tl_fwd", "Step forward", nav, can_fwd, false).clicked() {
            action = Some(Action::Marker(if at + 1 >= n { None } else { Some(at + 1) }));
        }
        if icons::button(ui, t, "tl_end", "Move to end", nav, can_fwd, false).clicked() {
            action = Some(Action::Marker(None));
        }
        let x = ui.cursor().left() + SIZE.grid;
        ui.painter().vline(x, ui.max_rect().y_range(), Stroke::new(1.0, t.divider));
        ui.add_space(SIZE.grid * 3.0);
        let width = ui.available_width() - nav - SIZE.grid * 3.0;
        let (strip, _) = ui.allocate_exact_size(Vec2::new(width.max(tile * 2.0), tile + SIZE.grid), Sense::hover());
        let gear = ui.allocate_exact_size(Vec2::splat(nav), Sense::click()).1;
        let gear_rect = gear.rect;
        if gear.hovered() {
            ui.painter().rect_filled(gear_rect, CornerRadius::same(SIZE.radius_button), t.toolbar_hover);
        }
        icons::paint(ui.painter(), gear_rect.shrink(SIZE.grid / 2.0), "settings", t.icon_outline, t);
        if gear.on_hover_text("Timeline settings").clicked() {
            action = Some(Action::Settings);
        }
        if n == 0 {
            ui.painter().text(
                strip.left_center() + Vec2::new(SIZE.grid * 2.0, 0.0),
                Align2::LEFT_CENTER,
                "Timeline: no features yet",
                FontId::proportional(SIZE.font_panel),
                t.text_secondary,
            );
            return;
        }
        let slots = slots(ui, s);
        let x0 = strip.left() + SIZE.grid;
        let y0 = strip.center().y - tile / 2.0;
        let slot_rect = |k: usize| Rect::from_min_size(pos2(x0 + k as f32 * (tile + gap), y0), Vec2::splat(tile));
        let gap_x = |k: usize| x0 + k as f32 * (tile + gap) - gap / 2.0;
        // feature index ↔ slot index (for the marker and for drops)
        let slot_of = |fi: usize| slots.iter().position(|sl| sl.first() >= fi).unwrap_or(slots.len());
        let feature_at = |k: usize| slots.get(k).map_or(n, Slot::first);
        let radius = CornerRadius::same(SIZE.radius_button);
        // Brackets over expanded groups.
        let mut k = 0;
        while k < slots.len() {
            let g = match slots.get(k) {
                Some(Slot::Feature(i)) => d.features.get(*i).and_then(|f| f.group),
                _ => None,
            };
            let Some(g) = g else {
                k += 1;
                continue;
            };
            let mut end = k;
            while matches!(slots.get(end + 1), Some(Slot::Feature(i)) if d.features.get(*i).and_then(|f| f.group) == Some(g))
            {
                end += 1;
            }
            let (a, b) = (slot_rect(k), slot_rect(end));
            let y = a.top() - 1.0;
            let p = ui.painter();
            let st = Stroke::new(1.5, t.ai);
            p.hline(a.left()..=b.right(), y, st);
            p.vline(a.left(), y..=y + SIZE.grid, st);
            p.vline(b.right(), y..=y + SIZE.grid, st);
            let close = Rect::from_center_size(pos2(a.left(), y), Vec2::splat(SIZE.grid * 2.5));
            let cr = ui.interact(close, Id::new(("tl_group_close", g)), Sense::click());
            p.circle_filled(close.center(), SIZE.grid, t.ai);
            if cr.on_hover_text(d.groups.get(&g).cloned().unwrap_or_default() + " — collapse").clicked() {
                ui.data_mut(|m| m.insert_temp(expanded_id(g), false));
            }
            k = end + 1;
        }
        for (k, slot) in slots.iter().enumerate() {
            let r = slot_rect(k);
            if r.right() > strip.right() {
                break;
            }
            match *slot {
                Slot::Group { id: g, first, last } => {
                    let resp = ui.interact(r, Id::new(("tl_group", g)), Sense::click());
                    if resp.hovered() {
                        ui.painter().rect_filled(r, radius, t.toolbar_hover);
                    }
                    let rolled = first >= active;
                    let color = if rolled { t.text_disabled } else { t.icon_outline };
                    icons::paint(ui.painter(), r.shrink(SIZE.grid / 2.0), "folder", color, t);
                    let badge = Rect::from_min_size(r.right_top() - Vec2::new(SIZE.grid * 3.0, 0.0), Vec2::splat(SIZE.grid * 3.0));
                    icons::paint(ui.painter(), badge, "ai", t.ai, t);
                    ui.painter().hline(r.x_range(), r.bottom() + 1.0, Stroke::new(2.0, t.ai));
                    let failed = (first..=last).any(|i| {
                        d.features.get(i).is_some_and(|f| {
                            s.model().status.iter().any(|x| x.id == f.id && matches!(x.status, Status::Error(_)))
                        })
                    });
                    if failed {
                        ui.painter().rect_stroke(r, radius, Stroke::new(2.0, t.error), StrokeKind::Inside);
                    }
                    let name = d.groups.get(&g).cloned().unwrap_or_else(|| "Group".into());
                    let tip = format!("{name}\n{} features — click to expand", last + 1 - first);
                    if resp.on_hover_text(tip).clicked() {
                        ui.data_mut(|m| m.insert_temp(expanded_id(g), true));
                    }
                }
                Slot::Feature(i) => {
                    let Some(f) = d.features.get(i) else { continue };
                    let (a, resp) = feature_tile(ui, t, s, f, i, r, active, n);
                    if a.is_some() {
                        action = a;
                    }
                    // Drag a feature to a new position.
                    if resp.drag_stopped()
                        && let Some(p) = ui.input(|inp| inp.pointer.interact_pos())
                    {
                        let to_slot = (((p.x - x0) / (tile + gap)).round().max(0.0) as usize).min(slots.len());
                        let to = feature_at(to_slot).min(n.saturating_sub(1));
                        if to != i {
                            action = Some(Action::Reorder(f.id, to));
                        }
                    }
                    if resp.dragged()
                        && let Some(p) = ui.input(|inp| inp.pointer.interact_pos())
                    {
                        let to = ((p.x - x0) / (tile + gap)).round().clamp(0.0, slots.len() as f32) as usize;
                        ui.painter().vline(gap_x(to), strip.y_range(), Stroke::new(2.0, t.accent));
                    }
                }
            }
        }
        // History marker: a bar with a triangle at the bottom, draggable.
        let mk = slot_of(active.min(n));
        let mx = gap_x(mk) + if mk >= slots.len() { gap } else { 0.0 };
        let handle = Rect::from_center_size(pos2(mx, strip.center().y), Vec2::new(SIZE.grid * 2.0, strip.height()));
        let hr =
            ui.interact(handle, Id::new("tl_marker"), Sense::drag()).on_hover_text("History marker: drag to roll back");
        let x = if hr.dragged() { ui.input(|i| i.pointer.interact_pos()).map(|p| p.x).unwrap_or(mx) } else { mx };
        let p = ui.painter();
        p.vline(x, strip.y_range(), Stroke::new(2.0, t.accent));
        let b = strip.bottom();
        let w = SIZE.grid * 1.25;
        p.add(egui::Shape::convex_polygon(
            vec![pos2(x - w, b), pos2(x + w, b), pos2(x, b - w * 1.5)],
            t.accent,
            Stroke::NONE,
        ));
        if hr.drag_stopped() {
            let k = ((x - x0 + gap / 2.0) / (tile + gap)).round().clamp(0.0, slots.len() as f32) as usize;
            let fi = feature_at(k);
            action = Some(Action::Marker(if fi >= n { None } else { Some(fi) }));
        }
    });
    action
}

/// One feature icon with its state (rolled back, suppressed, error) and context menu.
#[allow(clippy::too_many_arguments)]
fn feature_tile(
    ui: &mut Ui,
    t: &Tokens,
    s: &Session,
    f: &formcraft_engine::doc::Feature,
    i: usize,
    r: Rect,
    active: usize,
    n: usize,
) -> (Option<Action>, egui::Response) {
    let mut action = None;
    let radius = CornerRadius::same(SIZE.radius_button);
    let resp = ui.interact(r, Id::new(("tl_feature", f.id.0)), Sense::click_and_drag());
    let status = s.model().status.iter().find(|x| x.id == f.id).map(|x| &x.status);
    let rolled = i >= active;
    let err = matches!(status, Some(Status::Error(_)));
    let selected = s.selection().items.iter().any(|p| p.body().0 == f.id.0);
    if err {
        ui.painter().rect_filled(r, radius, t.error.gamma_multiply(0.15));
    } else if resp.hovered() {
        ui.painter().rect_filled(r, radius, t.toolbar_hover);
    }
    let color = if rolled || f.suppressed { t.text_disabled } else { t.icon_outline };
    icons::paint(ui.painter(), r.shrink(SIZE.grid / 2.0), icon_for(f.kind.base_name()), color, t);
    if err {
        ui.painter().rect_stroke(r, radius, Stroke::new(2.0, t.error), StrokeKind::Inside);
    } else if selected {
        ui.painter().rect_stroke(r, radius, Stroke::new(1.5, t.accent), StrokeKind::Inside);
    }
    if f.suppressed {
        ui.painter().rect_stroke(r, radius, Stroke::new(1.5, t.warning), StrokeKind::Inside);
        ui.painter().line_segment([r.left_bottom(), r.right_top()], Stroke::new(1.5, t.warning));
    }
    if f.group.is_some() {
        ui.painter().hline(r.x_range(), r.bottom() + 1.0, Stroke::new(2.0, t.ai));
    }
    let tip = match status {
        Some(Status::Error(e)) => format!("{}\nError: {e}", f.name),
        Some(Status::Suppressed) => format!("{} (suppressed)", f.name),
        Some(Status::RolledBack) => format!("{} (rolled back)", f.name),
        _ => f.name.clone(),
    };
    let resp = resp.on_hover_text(tip);
    let out = resp.clone();
    if resp.double_clicked() {
        action = Some(Action::Edit(f.id));
    }
    resp.context_menu(|ui| {
        if ui.button("Edit Feature").clicked() {
            action = Some(Action::Edit(f.id));
            ui.close();
        }
        if ui.button("Rename…").clicked() {
            action = Some(Action::Rename(f.id));
            ui.close();
        }
        let label = if f.suppressed { "Unsuppress Features" } else { "Suppress Features" };
        if ui.button(label).clicked() {
            action = Some(Action::Suppress(f.id, !f.suppressed));
            ui.close();
        }
        if ui.button("Roll History Marker Here").clicked() {
            action = Some(Action::Marker(Some(i + 1).filter(|m| *m < n)));
            ui.close();
        }
        if ui.button("Explain").on_hover_text("Describe this feature in words").clicked() {
            action = Some(Action::Explain(f.id));
            ui.close();
        }
        ui.separator();
        if ui.button("Delete").clicked() {
            action = Some(Action::Delete(f.id));
            ui.close();
        }
    });
    (action, out)
}
