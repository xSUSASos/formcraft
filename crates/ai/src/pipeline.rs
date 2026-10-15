//! Request → LLM (structured output) → plan → validation → trial on a forked session (regen)
//! → on failure the error goes back to the LLM (up to `max_attempts`) → a proposal with a
//! preview. Nothing touches the user's document until the proposal is accepted.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use formcraft_engine::doc::FeatureId;
use formcraft_engine::{Pick, Session};
use serde::Serialize;
use serde_json::json;

use crate::backend::{ChatRequest, LlmBackend, Message};
use crate::plan::{self, Plan};
use crate::{fewshot, log, prompt};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Progress {
    /// Waiting for the model (attempt `attempt` of `of`).
    Generating { attempt: usize, of: usize },
    /// The previous plan failed; asking the model to fix it.
    Repairing { attempt: usize, of: usize, error: String },
}

impl Progress {
    pub fn label(&self) -> String {
        match self {
            Progress::Generating { .. } => "Generating…".into(),
            Progress::Repairing { attempt, of, .. } => format!("Repairing ({attempt}/{of})…"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum AiError {
    #[error("{0}")]
    Backend(String),
    #[error("no valid plan after {attempts} attempt(s): {last}")]
    Failed { attempts: usize, last: String },
    #[error("cancelled")]
    Cancelled,
    #[error("{0}")]
    Invalid(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct Attempt {
    pub reply: String,
    pub error: Option<String>,
    pub seconds: f64,
}

/// A successful generation: the plan and a session where it has been applied (the preview).
pub struct Proposal {
    pub plan: Plan,
    pub preview: Session,
    pub added: Vec<FeatureId>,
    pub attempts: Vec<Attempt>,
    pub elapsed: Duration,
}

#[derive(Debug, Clone, Default)]
pub struct AiRequest {
    pub prompt: String,
    /// Mention the current selection ("Edit with AI…").
    pub use_selection: bool,
}

pub struct Generator {
    pub backend: Arc<dyn LlmBackend>,
    pub max_attempts: usize,
    /// Few-shot examples per request (closest to the prompt).
    pub fewshot: usize,
    pub temperature: f64,
    pub max_tokens: u32,
    /// Write `logs/ai` records.
    pub log: bool,
}

impl Generator {
    pub fn new(backend: Arc<dyn LlmBackend>) -> Self {
        Generator { backend, max_attempts: 3, fewshot: 3, temperature: 0.1, max_tokens: 2048, log: true }
    }

    /// The conversation for the first attempt.
    pub fn messages(&self, base: &Session, req: &AiRequest) -> Vec<Message> {
        let mut m = vec![Message::system(prompt::system_prompt(base.registry()))];
        for ex in fewshot::pick(&req.prompt, self.fewshot) {
            m.push(Message::user(ex.prompt.clone()));
            m.push(Message::assistant(serde_json::to_string(&ex.plan).unwrap_or_default()));
        }
        let ctx = context(base, req.use_selection);
        m.push(Message::user(if ctx.is_empty() {
            req.prompt.clone()
        } else {
            format!("{ctx}\n\nRequest: {}", req.prompt)
        }));
        m
    }

    pub fn generate(
        &self,
        base: &Session,
        req: &AiRequest,
        progress: &mut dyn FnMut(&Progress),
        cancel: &AtomicBool,
    ) -> Result<Proposal, AiError> {
        if req.prompt.trim().is_empty() {
            return Err(AiError::Invalid("the request is empty".into()));
        }
        let start = Instant::now();
        let mut messages = self.messages(base, req);
        let schema = prompt::output_schema(base.registry());
        let of = self.max_attempts.max(1);
        let mut attempts = Vec::new();
        let mut last = String::new();
        for attempt in 1..=of {
            if cancel.load(Ordering::Relaxed) {
                return Err(AiError::Cancelled);
            }
            progress(&if attempt == 1 {
                Progress::Generating { attempt, of }
            } else {
                Progress::Repairing { attempt, of, error: last.clone() }
            });
            let t = Instant::now();
            let chat = ChatRequest {
                messages: messages.clone(),
                schema: Some(schema.clone()),
                temperature: self.temperature,
                max_tokens: self.max_tokens,
            };
            let reply = match self.backend.chat(&chat) {
                Ok(r) => r,
                Err(e) => {
                    self.write_log(req, &attempts, None, Some(&e), start);
                    return Err(AiError::Backend(e));
                }
            };
            if cancel.load(Ordering::Relaxed) {
                return Err(AiError::Cancelled);
            }
            let tried = plan::parse(&reply.text).and_then(|p| {
                let mut preview = base.fork();
                let label = format!("AI: {}", p.summary.trim());
                plan::apply(&mut preview, &p, &label).map(|added| (p, preview, added))
            });
            let secs = t.elapsed().as_secs_f64();
            match tried {
                Ok((p, preview, added)) => {
                    attempts.push(Attempt { reply: reply.text, error: None, seconds: secs });
                    self.write_log(req, &attempts, Some(&p), None, start);
                    return Ok(Proposal { plan: p, preview, added, attempts, elapsed: start.elapsed() });
                }
                Err(e) => {
                    last = e.to_string();
                    attempts.push(Attempt { reply: reply.text.clone(), error: Some(last.clone()), seconds: secs });
                    messages.push(Message::assistant(reply.text));
                    messages.push(Message::user(format!(
                        "That plan failed: {last}\nFix the problem and answer with the complete corrected JSON plan."
                    )));
                }
            }
        }
        self.write_log(req, &attempts, None, Some(&last), start);
        Err(AiError::Failed { attempts: attempts.len(), last })
    }

    fn write_log(&self, req: &AiRequest, attempts: &[Attempt], plan: Option<&Plan>, error: Option<&str>, t: Instant) {
        if self.log {
            log::write(&json!({
                "backend": self.backend.kind(), "model": self.backend.model(), "prompt": req.prompt,
                "attempts": attempts, "plan": plan, "error": error, "seconds": t.elapsed().as_secs_f64(),
            }));
        }
    }
}

fn fmt3(v: formcraft_engine::kernel::DVec3) -> String {
    format!("[{:.1}, {:.1}, {:.1}]", v.x, v.y, v.z)
}

/// What the model needs to know about the current design (empty for an empty design).
pub fn context(s: &Session, use_selection: bool) -> String {
    let d = s.document();
    let mut out = Vec::new();
    if !d.parameters.is_empty() {
        let values = d.parameter_values().unwrap_or_default();
        let p: Vec<String> = d
            .parameters
            .iter()
            .map(|p| match values.get(&p.name) {
                Some(v) => format!("{} = {v}", p.name),
                None => p.name.clone(),
            })
            .collect();
        out.push(format!("Parameters: {}", p.join(", ")));
    }
    if !d.features.is_empty() {
        let f: Vec<&str> = d.features.iter().map(|f| f.name.as_str()).collect();
        out.push(format!("Timeline: {}", f.join(", ")));
    }
    let k = s.kernel().clone();
    for b in &s.model().bodies {
        if let Ok(m) = k.mass_props(&b.shape) {
            out.push(format!(
                "Body \"{}\" (id {}): bbox {} to {}, volume {:.1}",
                b.name,
                b.id.0,
                fmt3(m.bbox.min),
                fmt3(m.bbox.max),
                m.volume
            ));
        }
    }
    if use_selection {
        for pick in s.selection().items.iter().take(20) {
            let Some(body) = s.model().body(pick.body()) else { continue };
            let line = match *pick {
                Pick::Body { .. } => format!("Selected: body \"{}\"", body.name),
                Pick::Face { index, .. } => {
                    match k.faces(&body.shape).ok().and_then(|f| f.into_iter().find(|f| f.index == index)) {
                        Some(f) => format!(
                            "Selected: a {:?} face of \"{}\", centre {}, normal {} — selector {{\"near\": {}}}",
                            f.kind,
                            body.name,
                            fmt3(f.center),
                            fmt3(f.normal),
                            fmt3(f.center)
                        ),
                        None => continue,
                    }
                }
                Pick::Edge { index, .. } => {
                    match k.edges(&body.shape).ok().and_then(|e| e.into_iter().find(|e| e.index == index)) {
                        Some(e) => format!(
                            "Selected: a {:?} edge of \"{}\", midpoint {} — selector {{\"near\": {}}}",
                            e.kind,
                            body.name,
                            fmt3(e.mid),
                            fmt3(e.mid)
                        ),
                        None => continue,
                    }
                }
            };
            out.push(line);
        }
    }
    if out.is_empty() { String::new() } else { format!("Current design:\n{}", out.join("\n")) }
}
