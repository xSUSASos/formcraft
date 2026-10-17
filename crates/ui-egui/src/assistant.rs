//! The Assistant (PLAN §3, UI_SPEC.md §5.12): a card on the right of the canvas with the
//! request feed. Generation runs on a background thread on a fork of the session; the result
//! is a proposal shown as a purple preview that the user accepts (one undo step, a feature
//! group on the timeline) or rejects (nothing changes).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};

use egui::{Color32, CornerRadius, RichText, Sense, Ui, Vec2};
use formcraft_ai::{AiError, AiRequest, Generator, Plan, Progress, Proposal};
use formcraft_engine::Session;
use serde_json::Value;

use crate::i18n::tr;
use crate::icons;
use crate::theme::{SIZE, Tokens};

pub enum EntryState {
    Running(String),
    Proposal(Box<Proposal>),
    Accepted(String),
    Rejected,
    Failed(String),
    /// A plain answer (Explain).
    Note(String),
}

pub struct Entry {
    pub prompt: String,
    pub state: EntryState,
}

enum JobMsg {
    Progress(Progress),
    Done(Box<Result<Proposal, AiError>>),
}

struct Job {
    rx: Receiver<JobMsg>,
    cancel: Arc<AtomicBool>,
    entry: usize,
}

/// What the app should do after a frame of the Assistant.
pub enum AssistantAction {
    None,
    Accept(usize),
    Reject(usize),
}

pub struct Assistant {
    pub generator: Option<Arc<Generator>>,
    pub input: String,
    pub use_selection: bool,
    pub entries: Vec<Entry>,
    job: Option<Job>,
    /// Server status (`ai.status`), fetched in the background when the panel opens.
    status: Option<Result<Value, String>>,
    status_rx: Option<Receiver<Result<Value, String>>>,
    pub focus_input: bool,
}

impl Assistant {
    pub fn new(generator: Option<Arc<Generator>>) -> Self {
        Assistant {
            generator,
            input: String::new(),
            use_selection: false,
            entries: Vec::new(),
            job: None,
            status: None,
            status_rx: None,
            focus_input: false,
        }
    }

    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    /// The proposal waiting for Accept/Reject (the latest one), if any.
    pub fn pending(&self) -> Option<(usize, &Proposal)> {
        self.entries.iter().enumerate().rev().find_map(|(i, e)| match &e.state {
            EntryState::Proposal(p) => Some((i, p.as_ref())),
            _ => None,
        })
    }

    pub fn note(&mut self, title: &str, text: String) {
        self.entries.push(Entry { prompt: title.to_string(), state: EntryState::Note(text) });
    }

    /// Start generating for `prompt` on a fork of `s`.
    pub fn start(&mut self, s: &Session, prompt: String, use_selection: bool) {
        let Some(generator) = self.generator.clone() else {
            self.entries.push(Entry { prompt, state: EntryState::Failed("the assistant is not configured".into()) });
            return;
        };
        if let Some(j) = &self.job {
            j.cancel.store(true, Ordering::Relaxed);
        }
        let fork = s.fork();
        let (tx, rx) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let c = cancel.clone();
        let req = AiRequest { prompt: prompt.clone(), use_selection };
        let spawned = std::thread::Builder::new().name("ai-generate".into()).spawn(move || {
            let progress_tx = tx.clone();
            let r = generator.generate(&fork, &req, &mut |p| drop(progress_tx.send(JobMsg::Progress(p.clone()))), &c);
            let _ = tx.send(JobMsg::Done(Box::new(r)));
        });
        let entry = self.entries.len();
        match spawned {
            Ok(_) => {
                self.entries.push(Entry { prompt, state: EntryState::Running("Generating…".into()) });
                self.job = Some(Job { rx, cancel, entry });
            }
            Err(e) => self.entries.push(Entry { prompt, state: EntryState::Failed(e.to_string()) }),
        }
    }

    pub fn stop(&mut self) {
        if let Some(j) = self.job.take() {
            j.cancel.store(true, Ordering::Relaxed);
            if let Some(e) = self.entries.get_mut(j.entry) {
                e.state = EntryState::Failed("stopped".into());
            }
        }
    }

    /// Take finished work from the background threads. Returns true when something changed.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        if let Some(rx) = &self.status_rx
            && let Ok(s) = rx.try_recv()
        {
            self.status = Some(s);
            self.status_rx = None;
            changed = true;
        }
        let Some(job) = &self.job else { return changed };
        let mut done = None;
        while let Ok(m) = job.rx.try_recv() {
            match m {
                JobMsg::Progress(p) => {
                    if let Some(e) = self.entries.get_mut(job.entry) {
                        e.state = EntryState::Running(p.label());
                    }
                }
                JobMsg::Done(r) => done = Some(*r),
            }
            changed = true;
        }
        if let Some(r) = done {
            let entry = job.entry;
            self.job = None;
            if let Some(e) = self.entries.get_mut(entry) {
                e.state = match r {
                    Ok(p) => EntryState::Proposal(Box::new(p)),
                    Err(AiError::Cancelled) => EntryState::Failed("stopped".into()),
                    Err(e) => EntryState::Failed(e.to_string()),
                };
            }
        }
        changed
    }

    /// Ask the backend for its status (model, VRAM) in the background.
    pub fn refresh_status(&mut self) {
        let Some(g) = self.generator.clone() else { return };
        if self.status_rx.is_some() {
            return;
        }
        let (tx, rx) = channel();
        if std::thread::Builder::new().name("ai-status".into()).spawn(move || drop(tx.send(g.backend.status()))).is_ok()
        {
            self.status_rx = Some(rx);
        }
    }

    fn header(&mut self, ui: &mut Ui, t: &Tokens, open: &mut bool) {
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(SIZE.icon_menu + SIZE.grid), Sense::hover());
            icons::paint(ui.painter(), r, "ai", t.ai, t);
            ui.label(RichText::new(tr("ASSISTANT")).font(crate::theme::heading(ui.ctx())).color(t.text_primary));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icons::button(ui, t, "close", tr("Close"), SIZE.icon_menu + SIZE.grid, true, false).clicked() {
                    *open = false;
                }
                // model + status dot (green: reachable, red: error, grey: unknown)
                let (dot, text) = match (&self.status, &self.generator) {
                    (_, None) => (t.text_disabled, tr("not configured").to_string()),
                    (None, Some(g)) => (t.text_disabled, g.backend.model()),
                    (Some(Ok(v)), Some(g)) => {
                        let vram = v["loaded"]
                            .as_array()
                            .and_then(|a| a.first())
                            .and_then(|m| m["vram"].as_f64())
                            .map(|b| {
                                if b > 0.0 { format!(" · {:.1} {}", b / 1e9, tr("GB VRAM")) } else { " · CPU".into() }
                            })
                            .unwrap_or_default();
                        let ok = v["available"].as_bool().unwrap_or(true);
                        (if ok { t.ok } else { t.warning }, format!("{}{vram}", g.backend.model()))
                    }
                    (Some(Err(e)), Some(g)) => {
                        let _ = e;
                        (t.error, format!("{} · {}", g.backend.model(), tr("offline")))
                    }
                };
                let tip = match &self.status {
                    Some(Err(e)) => e.clone(),
                    Some(Ok(v)) => v.to_string(),
                    None => tr("status not checked yet").into(),
                };
                let (r, resp) = ui.allocate_exact_size(Vec2::splat(SIZE.grid * 3.0), Sense::click());
                ui.painter().circle_filled(r.center(), SIZE.grid, dot);
                if resp.on_hover_text(tip).clicked() {
                    self.refresh_status();
                }
                ui.label(RichText::new(text).size(SIZE.font_group + 1.0).color(t.text_secondary));
            });
        });
    }

    /// Draw the panel content. `has_selection`: the "use selection" box is offered.
    pub fn ui(&mut self, ui: &mut Ui, s: &Session, t: &Tokens, open: &mut bool) -> AssistantAction {
        let mut action = AssistantAction::None;
        let g = SIZE.grid;
        ui.set_width(SIZE.assistant_w - g * 4.0);
        self.header(ui, t, open);
        let r = ui.cursor();
        ui.painter().hline(r.x_range(), r.top(), egui::Stroke::new(1.0, t.divider));
        ui.add_space(g);
        // the feed
        egui::ScrollArea::vertical().max_height(SIZE.assistant_w).stick_to_bottom(true).show(ui, |ui| {
            if self.entries.is_empty() {
                ui.label(
                    RichText::new(tr("Describe a part or a change, e.g. “flange D100, 4 holes D9 on D80, 10 thick”."))
                        .color(t.text_secondary),
                );
            }
            for (i, e) in self.entries.iter().enumerate() {
                ui.label(RichText::new(&e.prompt).strong().color(t.text_primary));
                match &e.state {
                    EntryState::Running(p) => {
                        // thin animated bar in the AI colour
                        let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), g / 2.0), Sense::hover());
                        let time = ui.input(|i| i.time) as f32;
                        let w = r.width() * 0.3;
                        let x = r.left() + (time * 0.6).fract() * (r.width() + w) - w;
                        ui.painter().rect_filled(r, CornerRadius::ZERO, t.ai.gamma_multiply(0.15));
                        let bar = egui::Rect::from_min_max(
                            egui::pos2(x.max(r.left()), r.top()),
                            egui::pos2((x + w).min(r.right()), r.bottom()),
                        );
                        ui.painter().rect_filled(bar, CornerRadius::ZERO, t.ai);
                        ui.ctx().request_repaint();
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(tr(p)).color(t.ai));
                            if ui.button(tr("Stop")).clicked()
                                && let Some(j) = self.job.as_ref()
                            {
                                j.cancel.store(true, Ordering::Relaxed);
                            }
                        });
                    }
                    EntryState::Proposal(p) => {
                        proposal_ui(ui, t, &p.plan, p.attempts.len(), p.elapsed.as_secs_f64());
                        ui.horizontal(|ui| {
                            let accept = egui::Button::new(RichText::new(tr("Accept")).strong().color(Color32::WHITE))
                                .fill(t.ai)
                                .corner_radius(CornerRadius::same(SIZE.radius_button));
                            if ui.add(accept).clicked() {
                                action = AssistantAction::Accept(i);
                            }
                            if ui.button(tr("Reject")).clicked() {
                                action = AssistantAction::Reject(i);
                            }
                        });
                    }
                    EntryState::Accepted(m) => {
                        ui.label(RichText::new(format!("✓ {m}")).color(t.ok));
                    }
                    EntryState::Rejected => {
                        ui.label(RichText::new(tr("Rejected")).color(t.text_secondary));
                    }
                    EntryState::Failed(m) => {
                        ui.label(RichText::new(tr(m)).color(t.error));
                    }
                    EntryState::Note(m) => {
                        ui.label(RichText::new(m).color(t.text_primary));
                    }
                }
                ui.add_space(g * 2.0);
            }
        });
        // the input
        let r = ui.cursor();
        ui.painter().hline(r.x_range(), r.top(), egui::Stroke::new(1.0, t.divider));
        ui.add_space(g);
        let has_sel = !s.selection().items.is_empty();
        let edit =
            ui.add(egui::TextEdit::multiline(&mut self.input).desired_rows(2).desired_width(f32::INFINITY).hint_text(
                if has_sel {
                    tr("Change the selection, e.g. “4 holes M5 in the corners”")
                } else {
                    tr("Describe a part…")
                },
            ));
        if self.focus_input {
            edit.request_focus();
            self.focus_input = false;
        }
        ui.horizontal(|ui| {
            ui.add_enabled(has_sel, egui::Checkbox::new(&mut self.use_selection, tr("Use selection")));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let ready = !self.input.trim().is_empty() && !self.busy();
                let go = egui::Button::new(RichText::new(tr("Generate")).strong().color(Color32::WHITE))
                    .fill(if ready { t.ai } else { t.text_disabled })
                    .corner_radius(CornerRadius::same(SIZE.radius_button));
                let enter = edit.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
                if (ui.add_enabled(ready, go).clicked() || (enter && ready)) && ready {
                    let prompt = self.input.trim().to_string();
                    self.input.clear();
                    let sel = self.use_selection && has_sel;
                    self.start(s, prompt, sel);
                }
            });
        });
        action
    }
}

/// The proposed features as a row of timeline-style icons plus the parameters.
fn proposal_ui(ui: &mut Ui, t: &Tokens, plan: &Plan, attempts: usize, secs: f64) {
    ui.label(RichText::new(&plan.summary).color(t.ai));
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = SIZE.timeline_gap;
        for st in &plan.steps {
            let icon = crate::timeline::icon_for(match st.command.as_str() {
                "sketch.create" => "Sketch",
                "solid.extrude" => "Extrude",
                "solid.revolve" => "Revolve",
                "solid.fillet" => "Fillet",
                "solid.chamfer" => "Chamfer",
                "solid.shell" => "Shell",
                "solid.hole" => "Hole",
                "solid.box" => "Box",
                "solid.cylinder" => "Cylinder",
                "solid.sphere" => "Sphere",
                "solid.press_pull" => "PressPull",
                "solid.combine" => "Combine",
                _ => "",
            });
            let (r, resp) = ui.allocate_exact_size(Vec2::splat(SIZE.icon_timeline), Sense::hover());
            icons::paint(ui.painter(), r, icon, t.icon_outline, t);
            resp.on_hover_text(format!("{} {}", st.command, st.params));
        }
    });
    if !plan.parameters.is_empty() {
        let p: Vec<String> = plan
            .parameters
            .iter()
            .map(|p| format!("{} = {}", p.name, crate::assistant::short(&p.expression)))
            .collect();
        ui.label(RichText::new(p.join(", ")).size(SIZE.font_group + 1.0).color(t.text_secondary));
    }
    let extra = if attempts > 1 { format!(", {attempts} {}", tr("attempts")) } else { String::new() };
    ui.label(RichText::new(format!("{secs:.0} {}{extra}", tr("s"))).size(SIZE.font_group).color(t.text_secondary));
}

pub fn short(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
