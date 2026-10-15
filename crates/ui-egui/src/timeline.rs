//! Timeline: the feature history as icons. Double-click edits, right-click opens a menu,
//! dragging a feature reorders it, dragging the marker rolls the model back.

use egui::{Align2, FontId, Id, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2};
use formcraft_engine::Session;
use formcraft_engine::doc::FeatureId;
use formcraft_engine::regen::Status;

use crate::icons;
use crate::theme::Tokens;

pub enum Action {
    Edit(FeatureId),
    Rename(FeatureId),
    Suppress(FeatureId, bool),
    Delete(FeatureId),
    Reorder(FeatureId, usize),
    Marker(Option<usize>),
    Explain(FeatureId),
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

const TILE: f32 = 26.0;
const GAP: f32 = 4.0;

/// Draw the timeline row; returns what the user asked for.
pub fn ui(ui: &mut Ui, t: &Tokens, s: &Session) -> Option<Action> {
    let mut action = None;
    let d = s.document();
    let n = d.features.len();
    let active = d.active_features().len();
    let marker = d.marker;
    ui.horizontal_centered(|ui| {
        // Playback: beginning, step back, step forward, end.
        let at = marker.unwrap_or(n);
        let can_back = at > 0;
        let can_fwd = at < n;
        if icons::button(ui, t, "tl_start", "Move to beginning", 18.0, can_back, false).clicked() {
            action = Some(Action::Marker(Some(0)));
        }
        if icons::button(ui, t, "tl_back", "Step back", 18.0, can_back, false).clicked() {
            action = Some(Action::Marker(Some(at.saturating_sub(1))));
        }
        if icons::button(ui, t, "tl_fwd", "Step forward", 18.0, can_fwd, false).clicked() {
            action = Some(Action::Marker(if at + 1 >= n { None } else { Some(at + 1) }));
        }
        if icons::button(ui, t, "tl_end", "Move to end", 18.0, can_fwd, false).clicked() {
            action = Some(Action::Marker(None));
        }
        ui.separator();
        let width = ui.available_width() - 8.0;
        let (strip, _) = ui.allocate_exact_size(Vec2::new(width.max(40.0), TILE + 6.0), Sense::hover());
        ui.painter().rect_filled(strip, 3.0, t.panel_header);
        if n == 0 {
            ui.painter().text(
                strip.left_center() + Vec2::new(10.0, 0.0),
                Align2::LEFT_CENTER,
                "Timeline: no features yet",
                FontId::proportional(12.0),
                t.text_secondary,
            );
            return;
        }
        let x0 = strip.left() + 6.0;
        let tile_rect =
            |i: usize| Rect::from_min_size(pos2(x0 + i as f32 * (TILE + GAP), strip.top() + 3.0), Vec2::splat(TILE));
        let gap_x = |i: usize| x0 + i as f32 * (TILE + GAP) - GAP / 2.0;
        for (i, f) in d.features.iter().enumerate() {
            let r = tile_rect(i);
            if r.right() > strip.right() {
                break;
            }
            let id = Id::new(("tl_feature", f.id.0));
            let resp = ui.interact(r, id, Sense::click_and_drag());
            let status = s.model().status.iter().find(|x| x.id == f.id).map(|x| &x.status);
            let rolled = i >= active;
            let err = matches!(status, Some(Status::Error(_)));
            if resp.hovered() {
                ui.painter().rect_filled(r, 3.0, t.accent_hover.gamma_multiply(0.25));
            }
            let color = if rolled || f.suppressed { t.text_secondary.gamma_multiply(0.45) } else { t.text_primary };
            icons::paint(ui.painter(), r.shrink(4.0), icon_for(f.kind.base_name()), color, t);
            if err {
                ui.painter().rect_stroke(r, 3.0, Stroke::new(2.0, t.error), StrokeKind::Inside);
            }
            if f.suppressed {
                ui.painter().line_segment([r.left_bottom(), r.right_top()], Stroke::new(1.5, t.warning));
            }
            if f.group.is_some() {
                ui.painter().hline(r.x_range(), r.bottom() + 1.5, Stroke::new(2.0, t.ai));
            }
            let tip = match status {
                Some(Status::Error(e)) => format!("{}\nError: {e}", f.name),
                Some(Status::Suppressed) => format!("{} (suppressed)", f.name),
                Some(Status::RolledBack) => format!("{} (rolled back)", f.name),
                _ => f.name.clone(),
            };
            let resp = resp.on_hover_text(tip);
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
            // Drag a feature to a new position.
            if resp.drag_stopped()
                && let Some(p) = ui.input(|inp| inp.pointer.interact_pos())
            {
                let to = (((p.x - x0) / (TILE + GAP)).round().max(0.0) as usize).min(n.saturating_sub(1));
                if to != i {
                    action = Some(Action::Reorder(f.id, to));
                }
            }
            if resp.dragged()
                && let Some(p) = ui.input(|inp| inp.pointer.interact_pos())
            {
                let to = ((p.x - x0) / (TILE + GAP)).round().clamp(0.0, n as f32) as usize;
                ui.painter().vline(gap_x(to), strip.y_range(), Stroke::new(2.0, t.accent));
            }
        }
        // History marker.
        let mx = gap_x(active.min(n)) + if active >= n { GAP } else { 0.0 };
        let handle = Rect::from_center_size(pos2(mx, strip.center().y), Vec2::new(8.0, strip.height()));
        let hr =
            ui.interact(handle, Id::new("tl_marker"), Sense::drag()).on_hover_text("History marker: drag to roll back");
        let x = if hr.dragged() { ui.input(|i| i.pointer.interact_pos()).map(|p| p.x).unwrap_or(mx) } else { mx };
        ui.painter().vline(x, strip.y_range(), Stroke::new(3.0, t.accent));
        ui.painter().circle_filled(pos2(x, strip.top() + 2.0), 3.5, t.accent);
        if hr.drag_stopped() {
            let gap = ((x - x0 + GAP / 2.0) / (TILE + GAP)).round().clamp(0.0, n as f32) as usize;
            action = Some(Action::Marker(if gap >= n { None } else { Some(gap) }));
        }
    });
    action
}
