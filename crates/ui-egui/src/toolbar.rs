//! Toolbar: workspace switcher, tabs (Solid / Sketch / Inspect / Tools) and command groups with
//! drop-downs. The layout lists the built-in commands in Fusion's order; commands that are not
//! registered (yet) are shown disabled. Commands registered at runtime with a matching
//! placement (plugins) are appended to their group.

use egui::{Pos2, Rect, Sense, Ui, Vec2};
use formcraft_engine::{CommandSpec, Registry};

use crate::icons;
use crate::theme::{SIZE, Tokens};

pub const TABS: [&str; 4] = ["Solid", "Sketch", "Inspect", "Tools"];

/// (id, label, icon, pinned on the toolbar)
type Item = (&'static str, &'static str, &'static str, bool);

pub struct Group {
    pub name: &'static str,
    pub items: &'static [Item],
}

const SOLID: &[Group] = &[
    Group {
        name: "Create",
        items: &[
            ("sketch.create", "Create Sketch", "sketch", true),
            ("solid.extrude", "Extrude", "extrude", true),
            ("solid.revolve", "Revolve", "revolve", true),
            ("solid.hole", "Hole", "hole", true),
            ("solid.box", "Box", "box", false),
            ("solid.cylinder", "Cylinder", "cylinder", false),
            ("solid.sphere", "Sphere", "sphere", false),
            ("ai.generate", "Generate", "ai", true),
        ],
    },
    Group {
        name: "Modify",
        items: &[
            ("solid.press_pull", "Press Pull", "press_pull", true),
            ("solid.fillet", "Fillet", "fillet", true),
            ("solid.chamfer", "Chamfer", "chamfer", true),
            ("solid.shell", "Shell", "shell", false),
            ("solid.combine", "Combine", "combine", false),
            ("solid.move", "Move/Copy", "move", false),
            ("doc.parameters", "Change Parameters", "parameters", false),
        ],
    },
    Group {
        name: "Construct",
        items: &[
            ("construct.offset_plane", "Offset Plane", "plane", true),
            ("construct.midplane", "Midplane", "plane", false),
            ("construct.axis", "Axis Through Two Points", "axis", false),
        ],
    },
    Group {
        name: "Inspect",
        items: &[
            ("inspect.measure", "Measure", "measure", true),
            ("inspect.section", "Section Analysis", "section", false),
            ("inspect.properties", "Properties", "properties", false),
        ],
    },
];

const SKETCH: &[Group] = &[
    Group {
        name: "Create",
        items: &[
            ("sketch.line", "Line", "line", true),
            ("sketch.rectangle", "2-Point Rectangle", "rectangle", true),
            ("sketch.circle", "Center Diameter Circle", "circle", true),
            ("sketch.arc", "3-Point Arc", "arc", true),
            ("sketch.dimension", "Sketch Dimension", "dimension", true),
        ],
    },
    Group {
        name: "Constraints",
        items: &[
            ("sketch.coincident", "Coincident", "c_coincident", true),
            ("sketch.horizontal", "Horizontal/Vertical", "c_horizontal", true),
            ("sketch.parallel", "Parallel", "c_parallel", true),
            ("sketch.perpendicular", "Perpendicular", "c_perpendicular", true),
            ("sketch.tangent", "Tangent", "c_tangent", false),
            ("sketch.equal", "Equal", "c_equal", false),
            ("sketch.fix", "Fix/Unfix", "c_fix", false),
        ],
    },
    Group { name: "Finish Sketch", items: &[("sketch.finish", "Finish Sketch", "finish", true)] },
];

const INSPECT: &[Group] = &[Group {
    name: "Inspect",
    items: &[
        ("inspect.measure", "Measure", "measure", true),
        ("inspect.section", "Section Analysis", "section", true),
        ("inspect.properties", "Properties", "properties", true),
    ],
}];

const TOOLS: &[Group] = &[
    Group { name: "Add-Ins", items: &[("tools.scripts", "Scripts and Add-Ins", "scripts", true)] },
    Group {
        name: "Utility",
        items: &[
            ("tools.text_commands", "Text Commands", "console", true),
            ("tools.settings", "Preferences", "settings", true),
        ],
    },
];

pub fn groups(tab: &str) -> &'static [Group] {
    match tab {
        "Sketch" => SKETCH,
        "Inspect" => INSPECT,
        "Tools" => TOOLS,
        _ => SOLID,
    }
}

/// What the user clicked on the toolbar.
pub enum Action {
    Run(String),
}

/// UI-level commands handled by the shell itself (not engine commands).
pub fn is_ui_command(id: &str) -> bool {
    matches!(id, "tools.text_commands" | "tools.settings" | "tools.scripts")
        || (id.starts_with("sketch.") && id != "sketch.edit" && id != "sketch.get")
}

fn enabled(reg: &Registry, id: &str) -> bool {
    is_ui_command(id) || reg.get(id).is_some()
}

/// Plugin commands placed in this tab/group that the static layout does not list.
fn extra<'a>(reg: &'a Registry, tab: &str, group: &Group) -> Vec<&'a CommandSpec> {
    reg.specs()
        .filter(|s| {
            s.placement.as_ref().is_some_and(|p| p.tab == tab && p.group == group.name)
                && !group.items.iter().any(|i| i.0 == s.id)
        })
        .collect()
}

/// Draw the tab strip (caps; the active tab in accent with a 2px underline).
pub fn tabs(ui: &mut Ui, t: &Tokens, active: &mut String, sketch_mode: bool) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = SIZE.grid;
        for tab in TABS {
            let on = active == tab;
            // In sketch mode the contextual SKETCH tab is tinted with the sketch colour.
            let color = match (on, sketch_mode && tab == "Sketch") {
                (_, true) => t.icon_sketch,
                (true, _) => t.accent,
                _ => t.toolbar_fg,
            };
            let text = egui::RichText::new(tab.to_uppercase())
                .size(SIZE.font_toolbar)
                .extra_letter_spacing(SIZE.group_letter_spacing)
                .color(color);
            let r = ui.add(
                egui::Button::new(text).frame(false).min_size(Vec2::new(SIZE.grid * 16.0, SIZE.tabs_h - SIZE.grid)),
            );
            if on {
                let rr = r.rect;
                ui.painter().hline(rr.x_range(), rr.bottom(), egui::Stroke::new(SIZE.tab_underline, color));
            }
            if r.clicked() {
                *active = tab.to_string();
            }
        }
    });
}

fn shortcut_of(reg: &Registry, id: &str) -> Option<String> {
    reg.get(id).and_then(|c| c.spec.shortcut.clone())
}

/// Draw the groups of the active tab: pinned commands as 32px buttons with captions, a group
/// caption in caps with ▾ underneath that opens the full list.
pub fn groups_ui(ui: &mut Ui, t: &Tokens, reg: &Registry, tab: &str) -> Option<Action> {
    let mut action = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = SIZE.grid;
        for (gi, g) in groups(tab).iter().enumerate() {
            if gi > 0 {
                let x = ui.cursor().left();
                let y = ui.max_rect().y_range();
                ui.painter().vline(x, (y.min + SIZE.grid)..=(y.max - SIZE.grid), egui::Stroke::new(1.0, t.divider));
                ui.add_space(SIZE.grid * 2.0);
            }
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = SIZE.grid / 2.0;
                    for &(id, label, icon, pinned) in g.items {
                        if pinned
                            && let Some(a) = icon_button(ui, t, id, label, icon, enabled(reg, id), shortcut_of(reg, id))
                        {
                            action = Some(a);
                        }
                    }
                    for s in extra(reg, tab, g).into_iter().filter(|s| s.placement.as_ref().is_some_and(|p| p.pinned)) {
                        let icon = s.icon.as_deref().unwrap_or("plugin");
                        if let Some(a) = icon_button(ui, t, &s.id, &s.label, icon, true, s.shortcut.clone()) {
                            action = Some(a);
                        }
                    }
                });
                let title = egui::RichText::new(format!("{}   ", g.name.to_uppercase()))
                    .size(SIZE.font_group)
                    .extra_letter_spacing(SIZE.group_letter_spacing)
                    .color(t.group_label);
                let r = ui.add(egui::Button::new(title).frame(false).min_size(Vec2::new(0.0, SIZE.font_group + 2.0)));
                // ▾ is drawn (the UI font has no such glyph)
                let tri = Rect::from_center_size(
                    r.rect.right_center() - Vec2::new(SIZE.grid * 1.5, 0.0),
                    Vec2::splat(SIZE.font_group),
                );
                icons::paint(ui.painter(), tri, "dropdown", t.group_label, t);
                egui::Popup::menu(&r).show(|ui| {
                    for &(id, label, icon, _) in g.items {
                        if menu_item(ui, t, label, icon, enabled(reg, id), shortcut_of(reg, id).as_deref()) {
                            action = Some(Action::Run(id.into()));
                        }
                    }
                    for s in extra(reg, tab, g) {
                        let icon = s.icon.as_deref().unwrap_or("plugin");
                        if menu_item(ui, t, &s.label, icon, true, s.shortcut.as_deref()) {
                            action = Some(Action::Run(s.id.clone()));
                        }
                    }
                });
            });
        }
    });
    action
}

/// A toolbar button: 32px icon with its caption below; the tooltip has the name and shortcut.
fn icon_button(
    ui: &mut Ui,
    t: &Tokens,
    id: &str,
    label: &str,
    icon: &str,
    on: bool,
    shortcut: Option<String>,
) -> Option<Action> {
    let g = SIZE.grid;
    let font = egui::FontId::proportional(SIZE.font_toolbar);
    let caption = ui.painter().layout_no_wrap(label.to_string(), font, t.toolbar_fg);
    let w = (caption.size().x + g * 2.0).max(SIZE.icon_toolbar + g * 2.0);
    let h = SIZE.icon_toolbar + caption.size().y + g * 0.75;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), if on { Sense::click() } else { Sense::hover() });
    // screen readers and UI tests see the command name (UI_SPEC §7)
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, on, label));
    let radius = egui::CornerRadius::same(SIZE.radius_button);
    if on && resp.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, radius, t.toolbar_pressed);
    } else if on && resp.hovered() {
        ui.painter().rect_filled(rect, radius, t.toolbar_hover);
    }
    let icon_rect = Rect::from_min_size(
        rect.center_top() + Vec2::new(-SIZE.icon_toolbar / 2.0, g / 4.0),
        Vec2::splat(SIZE.icon_toolbar),
    );
    let color = if on { t.icon_outline } else { t.text_disabled };
    icons::paint(ui.painter(), icon_rect, icon, color, t);
    let text_color = if on { t.toolbar_fg } else { t.text_disabled };
    let pos = Pos2::new(rect.center().x - caption.size().x / 2.0, icon_rect.bottom() + g / 4.0);
    ui.painter().galley(pos, caption, text_color);
    let mut tip = label.to_string();
    if let Some(s) = shortcut {
        tip.push_str(&format!("  ({s})"));
    }
    if !on {
        tip.push_str("\nNot available yet");
    }
    let resp = resp.on_hover_text(tip);
    (on && resp.clicked()).then(|| Action::Run(id.into()))
}

/// A drop-down row: 16px icon, label, shortcut right-aligned in grey.
fn menu_item(ui: &mut Ui, t: &Tokens, label: &str, icon: &str, on: bool, shortcut: Option<&str>) -> bool {
    let g = SIZE.grid;
    let row = Vec2::new(ui.available_width().max(g * 56.0), SIZE.tree_row_h);
    let (rect, resp) = ui.allocate_exact_size(row, if on { Sense::click() } else { Sense::hover() });
    if resp.hovered() && on {
        ui.painter().rect_filled(rect, egui::CornerRadius::same(SIZE.radius_button), t.toolbar_hover);
    }
    let color = if on { t.text_primary } else { t.text_disabled };
    let icon_rect =
        Rect::from_min_size(rect.left_center() + Vec2::new(g, -SIZE.icon_menu / 2.0), Vec2::splat(SIZE.icon_menu));
    icons::paint(ui.painter(), icon_rect, icon, if on { t.icon_outline } else { t.text_disabled }, t);
    let font = egui::FontId::proportional(SIZE.font_panel);
    ui.painter().text(
        rect.left_center() + Vec2::new(g * 7.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        font.clone(),
        color,
    );
    if let Some(s) = shortcut {
        ui.painter().text(
            rect.right_center() - Vec2::new(g * 2.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            s,
            font,
            t.text_secondary,
        );
    }
    let clicked = on && resp.clicked();
    if clicked {
        ui.close();
    }
    clicked
}
