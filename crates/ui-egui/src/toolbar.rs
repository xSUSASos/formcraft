//! Toolbar: workspace switcher, tabs (Solid / Sketch / Inspect / Tools) and command groups with
//! drop-downs. The layout lists the built-in commands in Fusion's order; commands that are not
//! registered (yet) are shown disabled. Commands registered at runtime with a matching
//! placement (plugins) are appended to their group.

use egui::{Rect, Sense, Ui, Vec2};
use formcraft_engine::{CommandSpec, Registry};

use crate::icons;
use crate::theme::Tokens;

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
    matches!(id, "tools.text_commands" | "tools.settings")
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

/// Draw the tab strip; returns the new active tab.
pub fn tabs(ui: &mut Ui, t: &Tokens, active: &mut String) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for tab in TABS {
            let on = active == tab;
            let text = egui::RichText::new(tab.to_uppercase()).size(11.5).strong().color(if on {
                t.accent
            } else {
                t.text_secondary
            });
            let r = ui.add(egui::Button::new(text).frame(false).min_size(Vec2::new(64.0, 20.0)));
            if on {
                let rr = r.rect;
                ui.painter().hline(rr.x_range(), rr.bottom(), egui::Stroke::new(2.0, t.accent));
            }
            if r.clicked() {
                *active = tab.to_string();
            }
        }
    });
}

/// Draw the groups of the active tab. Pinned commands are large icon buttons; every group has
/// a drop-down with all its commands.
pub fn groups_ui(ui: &mut Ui, t: &Tokens, reg: &Registry, tab: &str) -> Option<Action> {
    let mut action = None;
    ui.horizontal(|ui| {
        for (gi, g) in groups(tab).iter().enumerate() {
            if gi > 0 {
                ui.separator();
            }
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    for &(id, label, icon, pinned) in g.items {
                        if pinned && let Some(a) = icon_button(ui, t, id, label, icon, enabled(reg, id)) {
                            action = Some(a);
                        }
                    }
                    for s in extra(reg, tab, g).into_iter().filter(|s| s.placement.as_ref().is_some_and(|p| p.pinned)) {
                        let icon = s.icon.as_deref().unwrap_or("plugin");
                        if let Some(a) = icon_button(ui, t, &s.id, &s.label, icon, true) {
                            action = Some(a);
                        }
                    }
                });
                let title =
                    egui::RichText::new(format!("{}    ", g.name.to_uppercase())).size(10.5).color(t.text_secondary);
                let r = ui.menu_button(title, |ui| {
                    for &(id, label, icon, _) in g.items {
                        if menu_item(ui, t, label, icon, enabled(reg, id)) {
                            action = Some(Action::Run(id.into()));
                        }
                    }
                    for s in extra(reg, tab, g) {
                        if menu_item(ui, t, &s.label, s.icon.as_deref().unwrap_or("plugin"), true) {
                            action = Some(Action::Run(s.id.clone()));
                        }
                    }
                });
                // Drop-down marker after the group title.
                let rr = r.response.rect;
                let tri = Rect::from_min_size(rr.right_center() + Vec2::new(-12.0, -4.0), Vec2::splat(8.0));
                icons::paint(ui.painter(), tri, "dropdown", t.text_secondary, t);
            });
        }
    });
    action
}

fn icon_button(ui: &mut Ui, t: &Tokens, id: &str, label: &str, icon: &str, on: bool) -> Option<Action> {
    let size = Vec2::splat(30.0);
    let (rect, resp) = ui.allocate_exact_size(size, if on { Sense::click() } else { Sense::hover() });
    if resp.hovered() && on {
        ui.painter().rect_filled(rect, 4.0, t.panel_header);
    }
    let color = if on { t.text_primary } else { t.text_secondary.gamma_multiply(0.5) };
    icons::paint(ui.painter(), rect.shrink(4.0), icon, color, t);
    let tip = if on { label.to_string() } else { format!("{label} (not available yet)") };
    let resp = resp.on_hover_text(tip);
    (on && resp.clicked()).then(|| Action::Run(id.into()))
}

fn menu_item(ui: &mut Ui, t: &Tokens, label: &str, icon: &str, on: bool) -> bool {
    let row = Vec2::new(ui.available_width().max(200.0), 22.0);
    let (rect, resp) = ui.allocate_exact_size(row, if on { Sense::click() } else { Sense::hover() });
    if resp.hovered() && on {
        ui.painter().rect_filled(rect, 3.0, t.accent.gamma_multiply(0.6));
    }
    let color = if on { t.text_primary } else { t.text_secondary.gamma_multiply(0.6) };
    let icon_rect = Rect::from_min_size(rect.min + Vec2::new(4.0, 3.0), Vec2::splat(16.0));
    icons::paint(ui.painter(), icon_rect, icon, color, t);
    ui.painter().text(
        rect.left_center() + Vec2::new(26.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.0),
        color,
    );
    let clicked = on && resp.clicked();
    if clicked {
        ui.close();
    }
    clicked
}
