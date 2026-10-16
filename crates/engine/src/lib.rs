//! Session, command registry (every action is a command with a JSON schema), selection, undo.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod builtin;
pub mod edit;
mod features3;
mod modeling;
pub mod params;
mod registry;
pub mod script;
pub mod selection;
mod sketch_edit;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;

pub use formcraft_doc as doc;
pub use formcraft_kernel as kernel;
pub use formcraft_regen as regen;
pub use registry::{Command, CommandSpec, Handler, Placement, Registry, Source, valid_id};
pub use selection::{Pick, Selection};

pub type Result<T> = std::result::Result<T, EngineError>;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EngineError {
    #[error("unknown command `{0}`")]
    UnknownCommand(String),
    #[error("invalid parameters: {0}")]
    InvalidParams(String),
    #[error("{0}")]
    Failed(String),
    #[error("not available yet: {0}")]
    NotAvailable(String),
    #[error("internal error in `{0}`: {1}")]
    Internal(String, String),
}

impl From<kernel::KernelError> for EngineError {
    fn from(e: kernel::KernelError) -> Self {
        EngineError::Failed(e.to_string())
    }
}

impl From<doc::DocError> for EngineError {
    fn from(e: doc::DocError) -> Self {
        EngineError::InvalidParams(e.to_string())
    }
}

impl From<formcraft_io::IoError> for EngineError {
    fn from(e: formcraft_io::IoError) -> Self {
        EngineError::Failed(e.to_string())
    }
}

/// One line of the Text Commands log.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LogEntry {
    pub command: String,
    pub params: Value,
    pub ok: bool,
    pub message: String,
}

/// An editing session: kernel, command registry, the document with its undo history, the
/// regenerated model and the selection.
pub struct Session {
    kernel: Arc<dyn kernel::Kernel>,
    registry: Registry,
    log: Vec<LogEntry>,
    doc: Arc<doc::Document>,
    undo: Vec<(String, Arc<doc::Document>)>,
    redo: Vec<(String, Arc<doc::Document>)>,
    model: regen::Model,
    selection: Selection,
    /// Bumped on every document change (views re-tessellate when it changes).
    revision: u64,
    /// File the document was last opened from / saved to.
    file: Option<std::path::PathBuf>,
    /// Revision at the last save/open (unsaved changes when it differs).
    saved_revision: u64,
    /// Builds plugin features (set by the plugin host).
    custom: Option<Arc<dyn regen::CustomFeatures>>,
}

const LOG_LIMIT: usize = 1000;
const UNDO_LIMIT: usize = 200;

impl Session {
    /// Session on the OpenCascade kernel with the built-in commands.
    pub fn new() -> Self {
        Self::with_kernel(Arc::new(formcraft_occt::Occt))
    }

    pub fn with_kernel(kernel: Arc<dyn kernel::Kernel>) -> Self {
        let mut s = Session {
            kernel,
            registry: Registry::default(),
            log: Vec::new(),
            doc: Arc::new(doc::Document::default()),
            undo: Vec::new(),
            redo: Vec::new(),
            model: regen::Model::default(),
            selection: Selection::default(),
            revision: 0,
            file: None,
            saved_revision: 0,
            custom: None,
        };
        builtin::register(&mut s.registry);
        modeling::register(&mut s.registry);
        features3::register(&mut s.registry);
        edit::register(&mut s.registry);
        sketch_edit::register(&mut s.registry);
        s
    }

    pub fn document(&self) -> &doc::Document {
        &self.doc
    }

    pub fn model(&self) -> &regen::Model {
        &self.model
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn file(&self) -> Option<&std::path::Path> {
        self.file.as_deref()
    }

    pub fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision
    }

    pub(crate) fn mark_saved(&mut self, path: std::path::PathBuf) {
        self.file = Some(path);
        self.saved_revision = self.revision;
    }

    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    pub fn selection_mut(&mut self) -> &mut Selection {
        &mut self.selection
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Label of the step that undo would revert.
    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|(l, _)| l.as_str())
    }

    /// Replace the document (new / open) and clear history.
    pub fn load(&mut self, d: doc::Document) {
        self.doc = Arc::new(d);
        self.undo.clear();
        self.redo.clear();
        self.selection.clear();
        self.file = None;
        self.regenerate();
        self.saved_revision = self.revision;
    }

    fn regenerate(&mut self) {
        self.model = regen::regen_with(&self.doc, self.kernel.as_ref(), self.custom.as_deref());
        self.selection.retain_bodies(&self.model);
        self.revision = self.revision.wrapping_add(1);
    }

    /// One undoable edit: the closure works on a copy of the document; the copy replaces the
    /// document only if the closure succeeds. `check` may inspect the regenerated model and
    /// refuse the edit (it is then rolled back).
    pub fn edit<T>(
        &mut self,
        label: &str,
        f: impl FnOnce(&mut doc::Document) -> Result<T>,
        check: impl FnOnce(&T, &regen::Model) -> Result<()>,
    ) -> Result<T> {
        let mut next = (*self.doc).clone();
        let out = f(&mut next)?;
        let model = regen::regen_with(&next, self.kernel.as_ref(), self.custom.as_deref());
        check(&out, &model)?;
        let prev = std::mem::replace(&mut self.doc, Arc::new(next));
        self.undo.push((label.to_string(), prev));
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.model = model;
        self.selection.retain_bodies(&self.model);
        self.revision = self.revision.wrapping_add(1);
        Ok(out)
    }

    /// A copy of this session without history: same kernel, commands (plugins included),
    /// document, model and selection. Used to try out changes off the UI thread (AI preview).
    pub fn fork(&self) -> Session {
        Session {
            kernel: self.kernel.clone(),
            registry: self.registry.clone(),
            log: Vec::new(),
            doc: self.doc.clone(),
            undo: Vec::new(),
            redo: Vec::new(),
            model: self.model.clone(),
            selection: self.selection.clone(),
            revision: self.revision,
            file: None,
            saved_revision: self.revision,
            custom: self.custom.clone(),
        }
    }

    /// Run several commands as one undo step. On error everything is rolled back. With
    /// `group`, the features the transaction adds become one named group on the timeline.
    /// Returns the closure's value and the ids of the added features.
    pub fn transaction<T>(
        &mut self,
        label: &str,
        group: Option<&str>,
        f: impl FnOnce(&mut Session) -> Result<T>,
    ) -> Result<(T, Vec<doc::FeatureId>)> {
        let start = self.doc.clone();
        let depth = self.undo.len();
        let redo = std::mem::take(&mut self.redo);
        let out = f(self);
        let changed = !Arc::ptr_eq(&start, &self.doc);
        self.undo.truncate(depth);
        let out = match out {
            Err(e) => {
                self.redo = redo;
                if changed {
                    self.doc = start;
                    self.regenerate();
                }
                return Err(e);
            }
            Ok(v) => v,
        };
        if !changed {
            self.redo = redo;
            return Ok((out, Vec::new()));
        }
        let added: Vec<doc::FeatureId> =
            self.doc.features.iter().map(|f| f.id).filter(|id| start.feature(*id).is_none()).collect();
        if let Some(name) = group.filter(|_| !added.is_empty()) {
            let d = Arc::make_mut(&mut self.doc);
            let gid = d.groups.keys().next_back().map_or(1, |k| k.saturating_add(1));
            d.groups.insert(gid, name.to_string());
            for f in d.features.iter_mut().filter(|f| added.contains(&f.id)) {
                f.group = Some(gid);
            }
        }
        self.undo.push((label.to_string(), start));
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.revision = self.revision.wrapping_add(1);
        Ok((out, added))
    }

    pub fn undo(&mut self) -> Result<String> {
        let (label, prev) = self.undo.pop().ok_or_else(|| EngineError::Failed("nothing to undo".into()))?;
        let cur = std::mem::replace(&mut self.doc, prev);
        self.redo.push((label.clone(), cur));
        self.regenerate();
        Ok(label)
    }

    pub fn redo(&mut self) -> Result<String> {
        let (label, next) = self.redo.pop().ok_or_else(|| EngineError::Failed("nothing to redo".into()))?;
        let cur = std::mem::replace(&mut self.doc, next);
        self.undo.push((label.clone(), cur));
        self.regenerate();
        Ok(label)
    }

    /// Install (or remove) the builder of plugin features and rebuild the model with it.
    pub fn set_custom_features(&mut self, custom: Option<Arc<dyn regen::CustomFeatures>>) {
        self.custom = custom;
        self.regenerate();
    }

    pub fn custom_features(&self) -> Option<&Arc<dyn regen::CustomFeatures>> {
        self.custom.as_ref()
    }

    /// Add a plugin feature (one undo step). The body is built once here to keep a frozen copy
    /// in the document (shown if the plugin is missing later). `params` may hold expressions.
    pub fn add_custom_feature(
        &mut self,
        plugin: &str,
        kind: &str,
        label: &str,
        params: Value,
        op: doc::BodyOp,
    ) -> Result<Value> {
        let custom =
            self.custom.clone().ok_or_else(|| EngineError::Failed(format!("plugin `{plugin}` is not loaded")))?;
        let evaluated = self.eval_params(&params);
        let solid = custom.regenerate(plugin, kind, &evaluated).map_err(EngineError::Failed)?;
        let frozen = self.kernel.serialize(&solid).ok().map(|b| kernel::b64::encode(&b));
        let feature = doc::FeatureKind::Custom { plugin: plugin.into(), kind: kind.into(), params, op, frozen };
        let base = label.to_string();
        let id = self.edit(
            &format!("Add {label}"),
            |d| {
                let id = d.add_feature(feature)?;
                let name = (1..).map(|i| format!("{base}{i}")).find(|n| !d.features.iter().any(|f| &f.name == n));
                if let (Some(f), Some(n)) = (d.feature_mut(id), name) {
                    f.name = n;
                }
                Ok(id)
            },
            |id, m| match m.status.iter().find(|s| s.id == *id).map(|s| &s.status) {
                Some(regen::Status::Error(e)) => Err(EngineError::Failed(e.clone())),
                _ => Ok(()),
            },
        )?;
        let name = self.doc.feature(id).map(|f| f.name.clone()).unwrap_or_default();
        Ok(serde_json::json!({"feature": id.0, "message": format!("{name} added")}))
    }

    /// Parameter expressions inside a JSON value evaluated to numbers (strings that are not
    /// expressions stay strings).
    pub fn eval_params(&self, v: &Value) -> Value {
        let vals = self.doc.parameter_values().unwrap_or_default();
        fn walk(v: &Value, vals: &std::collections::HashMap<String, f64>) -> Value {
            match v {
                Value::String(s) => match doc::Expr(s.clone()).eval(&|n| vals.get(n).copied()) {
                    Ok(x) if x.is_finite() => serde_json::json!(x),
                    _ => v.clone(),
                },
                Value::Array(a) => Value::Array(a.iter().map(|x| walk(x, vals)).collect()),
                Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), walk(x, vals))).collect()),
                other => other.clone(),
            }
        }
        walk(v, &vals)
    }

    pub fn kernel(&self) -> &Arc<dyn kernel::Kernel> {
        &self.kernel
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn registry_mut(&mut self) -> &mut Registry {
        &mut self.registry
    }

    pub fn log(&self) -> &[LogEntry] {
        &self.log
    }

    /// Run a command with JSON params. A panicking handler is caught and reported as an
    /// internal error (last-resort guard: the app never goes down with a command).
    pub fn execute(&mut self, id: &str, params: &Value) -> Result<Value> {
        let handler = self.registry.get(id).map(|c| c.handler.clone());
        let result = match handler {
            None => Err(EngineError::UnknownCommand(id.into())),
            Some(h) => {
                let params = if params.is_null() { Value::Object(Default::default()) } else { params.clone() };
                if !params.is_object() {
                    Err(EngineError::InvalidParams("params must be a JSON object".into()))
                } else {
                    catch_unwind(AssertUnwindSafe(|| h(self, &params)))
                        .unwrap_or_else(|p| Err(EngineError::Internal(id.into(), panic_message(&*p))))
                }
            }
        };
        let message = match &result {
            Ok(v) => v.get("message").and_then(Value::as_str).unwrap_or("ok").to_string(),
            Err(e) => e.to_string(),
        };
        self.push_log(LogEntry { command: id.into(), params: params.clone(), ok: result.is_ok(), message });
        result
    }

    pub fn push_log(&mut self, e: LogEntry) {
        if self.log.len() >= LOG_LIMIT {
            self.log.remove(0);
        }
        self.log.push(e);
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

fn panic_message(p: &(dyn std::any::Any + Send)) -> String {
    p.downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| p.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panic".into())
}

#[cfg(test)]
mod tests;
