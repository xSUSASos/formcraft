//! Preferences (PLAN Phase 7): theme, language, interface scale, animations, the assistant's
//! backend, logs. Stored with the layout; the AI fields override the environment.

use egui::{RichText, Ui};
use serde::{Deserialize, Serialize};

use crate::i18n::{Lang, tr};
use crate::theme::{SIZE, ThemeKind, Tokens};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub language: Lang,
    /// 0.8 ..= 1.5 (UI_SPEC §3).
    pub ui_scale: f32,
    pub animations: bool,
    /// `ollama` or `openai`; empty = from the environment.
    pub ai_backend: String,
    pub ai_url: String,
    pub ai_model: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: Lang::En,
            ui_scale: 1.0,
            animations: true,
            ai_backend: String::new(),
            ai_url: String::new(),
            ai_model: String::new(),
        }
    }
}

impl Settings {
    pub fn sanitize(&mut self) {
        self.ui_scale = if self.ui_scale.is_finite() { self.ui_scale.clamp(0.8, 1.5) } else { 1.0 };
        for s in [&mut self.ai_backend, &mut self.ai_url, &mut self.ai_model] {
            if s.len() > 512 {
                s.clear();
            }
        }
    }

    /// The assistant's configuration: defaults ← environment ← these settings.
    pub fn ai_config(&self) -> formcraft_ai::AiConfig {
        let mut c = formcraft_ai::AiConfig::from_env();
        if !self.ai_backend.trim().is_empty() {
            c.backend = self.ai_backend.trim().to_string();
        }
        if !self.ai_url.trim().is_empty() {
            c.url = self.ai_url.trim().to_string();
        }
        if !self.ai_model.trim().is_empty() {
            c.model = self.ai_model.trim().to_string();
        }
        c
    }
}

pub enum SettingsAction {
    None,
    ApplyAi,
    CheckAi,
    ResetLayout,
    OpenLogs,
}

/// The preferences window. `theme` and `s` are edited in place; `ai_status` is the last check.
pub fn window(
    ui: &mut Ui,
    t: &Tokens,
    open: &mut bool,
    theme: &mut ThemeKind,
    s: &mut Settings,
    ai_status: &str,
) -> SettingsAction {
    let mut action = SettingsAction::None;
    egui::Window::new(tr("Preferences"))
        .id(egui::Id::new("settings"))
        .frame(crate::theme::card(t))
        .collapsible(false)
        .resizable(false)
        .default_width(SIZE.assistant_w)
        .default_pos(ui.ctx().content_rect().center() - egui::vec2(SIZE.assistant_w * 0.6, SIZE.assistant_w * 0.6))
        .open(open)
        .show(ui.ctx(), |ui| {
            egui::Grid::new("settings_grid").num_columns(2).spacing([SIZE.grid * 3.0, SIZE.grid * 2.0]).show(
                ui,
                |ui| {
                    ui.label(tr("Theme"));
                    ui.horizontal(|ui| {
                        ui.selectable_value(theme, ThemeKind::Light, tr("Light"));
                        ui.selectable_value(theme, ThemeKind::Dark, tr("Dark"));
                    });
                    ui.end_row();
                    ui.label(tr("Language"));
                    ui.horizontal(|ui| {
                        for l in Lang::ALL {
                            ui.selectable_value(&mut s.language, l, l.name());
                        }
                    });
                    ui.end_row();
                    ui.label(tr("Interface scale"));
                    ui.add(
                        egui::Slider::new(&mut s.ui_scale, 0.8..=1.5)
                            .step_by(0.05)
                            .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                    );
                    ui.end_row();
                    ui.label(tr("Animations"));
                    ui.checkbox(&mut s.animations, "");
                    ui.end_row();
                },
            );
            ui.separator();
            ui.label(RichText::new(tr("ASSISTANT")).size(SIZE.font_group).color(t.group_label));
            let cfg = s.ai_config();
            egui::Grid::new("settings_ai").num_columns(2).spacing([SIZE.grid * 3.0, SIZE.grid * 2.0]).show(ui, |ui| {
                ui.label(tr("Assistant backend"));
                ui.horizontal(|ui| {
                    for b in ["ollama", "openai"] {
                        if ui.selectable_label(cfg.backend == b, b).clicked() {
                            s.ai_backend = b.into();
                        }
                    }
                });
                ui.end_row();
                ui.label(tr("Server URL"));
                ui.add(egui::TextEdit::singleline(&mut s.ai_url).hint_text(cfg.url.clone()));
                ui.end_row();
                ui.label(tr("Model"));
                ui.add(egui::TextEdit::singleline(&mut s.ai_model).hint_text(cfg.model.clone()));
                ui.end_row();
            });
            ui.horizontal(|ui| {
                if ui.button(tr("Check connection")).clicked() {
                    action = SettingsAction::CheckAi;
                }
                if ui.button(tr("Apply")).clicked() {
                    action = SettingsAction::ApplyAi;
                }
            });
            if !ai_status.is_empty() {
                ui.label(RichText::new(ai_status).size(SIZE.font_group + 1.0).color(t.text_secondary));
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(tr("Open logs folder")).clicked() {
                    action = SettingsAction::OpenLogs;
                }
                if ui.button(tr("Reset layout")).clicked() {
                    action = SettingsAction::ResetLayout;
                }
            });
        });
    action
}

/// `%LOCALAPPDATA%/FormCraft/logs` (or `~/.local/state/formcraft/logs`), `FORMCRAFT_LOG_DIR`
/// overrides. The app log and the AI request logs live here.
pub fn logs_dir() -> Option<std::path::PathBuf> {
    formcraft_ai::log::dir().and_then(|ai| ai.parent().map(std::path::Path::to_path_buf))
}
