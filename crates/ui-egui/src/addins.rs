//! Scripts and Add-Ins (PLAN §2, §3А; Shift+S): installed plugins with their state, commands
//! and log (install from .zip, reload, unload, open folder), and scripts (Python files run
//! against this app through the control channel).

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};

use egui::{RichText, Ui};
use formcraft_engine::Session;
use formcraft_plugin::Plugins;

use crate::i18n::tr;
use crate::theme::{SIZE, Tokens};

#[derive(Default, PartialEq, Clone, Copy)]
pub enum Tab {
    #[default]
    AddIns,
    Scripts,
}

pub struct AddIns {
    pub open: bool,
    pub tab: Tab,
    pub plugins: Option<Arc<Plugins>>,
    /// Where plugins are installed (`<config>/plugins`).
    pub plugins_dir: Option<PathBuf>,
    /// Where scripts are looked up (`<config>/scripts`, plus extra folders).
    pub script_dirs: Vec<PathBuf>,
    /// Output of the running/last script.
    pub script_out: Arc<Mutex<String>>,
    script_done: Option<Receiver<i32>>,
    pub message: Option<String>,
    expanded_log: Option<String>,
}

/// What the app should do (needs the session or the control channel).
pub enum AddInAction {
    None,
    Install(PathBuf),
    Unload(String),
    Reload(PathBuf),
    RunScript(PathBuf),
}

impl AddIns {
    pub fn new() -> Self {
        AddIns {
            open: false,
            tab: Tab::AddIns,
            plugins: None,
            plugins_dir: formcraft_plugin::install::default_dir(),
            script_dirs: Vec::new(),
            script_out: Arc::new(Mutex::new(String::new())),
            script_done: None,
            message: None,
            expanded_log: None,
        }
    }

    pub fn running(&self) -> bool {
        self.script_done.is_some()
    }

    /// Start `python <script>` with `FORMCRAFT_PORT=port`; output collected in `script_out`.
    pub fn start_script(&mut self, script: &Path, port: u16) {
        let python = if cfg!(windows) { "python" } else { "python3" };
        let mut cmd = std::process::Command::new(python);
        cmd.arg(script)
            .env("FORMCRAFT_PORT", port.to_string())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        if let Some(dir) = script.parent() {
            cmd.current_dir(dir);
        }
        let out = self.script_out.clone();
        if let Ok(mut o) = out.lock() {
            *o = format!("$ {python} {}\n", script.display());
        }
        match cmd.spawn() {
            Err(e) => self.message = Some(format!("cannot start {python}: {e}")),
            Ok(child) => {
                let (tx, rx) = channel();
                self.script_done = Some(rx);
                let spawned = std::thread::Builder::new().name("script".into()).spawn(move || {
                    let r = child.wait_with_output();
                    let code = match r {
                        Ok(o) => {
                            if let Ok(mut s) = out.lock() {
                                s.push_str(&String::from_utf8_lossy(&o.stdout));
                                s.push_str(&String::from_utf8_lossy(&o.stderr));
                            }
                            o.status.code().unwrap_or(-1)
                        }
                        Err(e) => {
                            if let Ok(mut s) = out.lock() {
                                s.push_str(&e.to_string());
                            }
                            -1
                        }
                    };
                    let _ = tx.send(code);
                });
                if let Err(e) = spawned {
                    self.message = Some(e.to_string());
                }
            }
        }
    }

    fn poll_script(&mut self) {
        if let Some(rx) = &self.script_done
            && let Ok(code) = rx.try_recv()
        {
            self.script_done = None;
            self.message =
                Some(if code == 0 { "Script finished".into() } else { format!("Script failed (exit {code})") });
        }
    }

    fn scripts(&self) -> Vec<PathBuf> {
        let mut dirs = self.script_dirs.clone();
        if let Some(p) = &self.plugins_dir
            && let Some(cfg) = p.parent()
        {
            dirs.insert(0, cfg.join("scripts"));
        }
        let mut out: Vec<PathBuf> = dirs
            .iter()
            .flat_map(|d| std::fs::read_dir(d).into_iter().flatten().flatten())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "py"))
            .collect();
        out.sort();
        out.dedup();
        out
    }

    pub fn ui(&mut self, ui: &mut Ui, t: &Tokens, _s: &Session) -> AddInAction {
        self.poll_script();
        if self.running() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
        }
        let mut action = AddInAction::None;
        if !self.open {
            return action;
        }
        let mut open = true;
        egui::Window::new(tr("Scripts and Add-Ins"))
            .id(egui::Id::new("addins"))
            .frame(crate::theme::card(t))
            .default_width(SIZE.assistant_w * 1.6)
            .default_pos(ui.ctx().content_rect().center() - egui::vec2(SIZE.assistant_w * 0.8, SIZE.assistant_w * 0.6))
            .collapsible(false)
            .open(&mut open)
            .show(ui.ctx(), |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.tab, Tab::AddIns, tr("ADD-INS"));
                    ui.selectable_value(&mut self.tab, Tab::Scripts, tr("SCRIPTS"));
                });
                ui.separator();
                match self.tab {
                    Tab::AddIns => action = self.addins_tab(ui, t),
                    Tab::Scripts => action = self.scripts_tab(ui, t),
                }
                if let Some(m) = &self.message {
                    ui.separator();
                    ui.label(RichText::new(m).color(t.text_secondary));
                }
            });
        self.open = open;
        action
    }

    fn addins_tab(&mut self, ui: &mut Ui, t: &Tokens) -> AddInAction {
        let mut action = AddInAction::None;
        let list = self.plugins.as_ref().map(|p| p.list()).unwrap_or_default();
        if list.is_empty() {
            ui.label(RichText::new(tr("No add-ins installed.")).color(t.text_secondary));
        }
        egui::ScrollArea::vertical().max_height(SIZE.assistant_w).show(ui, |ui| {
            for p in &list {
                let m = &p.manifest;
                ui.horizontal(|ui| {
                    let dead = p.is_dead();
                    let (r, _) = ui.allocate_exact_size(egui::Vec2::splat(SIZE.grid * 3.0), egui::Sense::hover());
                    ui.painter().circle_filled(r.center(), SIZE.grid, if dead { t.error } else { t.ok });
                    ui.label(RichText::new(format!("{} {}", m.name, m.version)).strong());
                    ui.label(RichText::new(&m.id).color(t.text_secondary));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !dead && ui.button(tr("Unload")).clicked() {
                            action = AddInAction::Unload(m.id.clone());
                        }
                        if ui.button(tr("Reload")).clicked() {
                            action = AddInAction::Reload(p.dir.clone());
                        }
                        if ui.button(tr("Log")).clicked() {
                            self.expanded_log =
                                if self.expanded_log.as_deref() == Some(&m.id) { None } else { Some(m.id.clone()) };
                        }
                    });
                });
                if !m.description.is_empty() {
                    ui.label(RichText::new(&m.description).size(SIZE.font_group + 1.0).color(t.text_secondary));
                }
                let reg = p.registered();
                let cmds: Vec<String> =
                    reg.commands.iter().cloned().chain(reg.features.iter().map(|k| format!("{}.{k}", m.id))).collect();
                if !cmds.is_empty() {
                    ui.label(
                        RichText::new(format!("{} {}", tr("Commands:"), cmds.join(", "))).size(SIZE.font_group + 1.0),
                    );
                }
                if let Some(f) = p.failure.lock().ok().and_then(|f| f.clone()) {
                    ui.label(RichText::new(format!("{} {f}", tr("Unloaded:"))).color(t.error));
                }
                if self.expanded_log.as_deref() == Some(&m.id) {
                    let log = p.log().join("\n");
                    ui.label(RichText::new(if log.is_empty() { tr("(empty log)").into() } else { log }).monospace());
                }
                ui.add_space(SIZE.grid * 2.0);
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button(tr("Install from .zip…")).clicked()
                && let Some(zip) = rfd::FileDialog::new().add_filter("FormCraft add-in", &["zip"]).pick_file()
            {
                action = AddInAction::Install(zip);
            }
            if let Some(dir) = &self.plugins_dir
                && ui.button(tr("Open folder")).clicked()
            {
                let _ = std::fs::create_dir_all(dir);
                open_folder(dir);
            }
        });
        action
    }

    fn scripts_tab(&mut self, ui: &mut Ui, t: &Tokens) -> AddInAction {
        let mut action = AddInAction::None;
        let scripts = self.scripts();
        if scripts.is_empty() {
            ui.label(RichText::new(tr("No scripts. Put .py files into the scripts folder.")).color(t.text_secondary));
        }
        for s in &scripts {
            ui.horizontal(|ui| {
                ui.label(s.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add_enabled(!self.running(), egui::Button::new(tr("Run"))).clicked() {
                        action = AddInAction::RunScript(s.clone());
                    }
                });
            });
        }
        ui.horizontal(|ui| {
            if ui.button(tr("Run file…")).clicked()
                && let Some(f) = rfd::FileDialog::new().add_filter("Python", &["py"]).pick_file()
            {
                action = AddInAction::RunScript(f);
            }
            if let Some(dir) = self.plugins_dir.as_ref().and_then(|p| p.parent()).map(|c| c.join("scripts"))
                && ui.button(tr("Open folder")).clicked()
            {
                let _ = std::fs::create_dir_all(&dir);
                open_folder(&dir);
            }
        });
        let out = self.script_out.lock().map(|o| o.clone()).unwrap_or_default();
        if !out.is_empty() {
            ui.separator();
            egui::ScrollArea::vertical().max_height(SIZE.text_commands_h).stick_to_bottom(true).show(ui, |ui| {
                ui.label(RichText::new(out).monospace().size(SIZE.font_panel - 1.0));
            });
        }
        action
    }
}

impl Default for AddIns {
    fn default() -> Self {
        Self::new()
    }
}

fn open_folder(dir: &Path) {
    let cmd = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    if let Err(e) = std::process::Command::new(cmd).arg(dir).spawn() {
        log::warn!("open {}: {e}", dir.display());
    }
}
