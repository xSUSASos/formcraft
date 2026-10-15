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
mod registry;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;

pub use formcraft_kernel as kernel;
pub use registry::{Command, CommandSpec, Handler, Placement, Registry, Source, valid_id};

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

/// One line of the Text Commands log.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LogEntry {
    pub command: String,
    pub params: Value,
    pub ok: bool,
    pub message: String,
}

/// An editing session: kernel, command registry, log. (Document, selection and undo arrive in
/// Phases 2–3.)
pub struct Session {
    kernel: Arc<dyn kernel::Kernel>,
    registry: Registry,
    log: Vec<LogEntry>,
}

const LOG_LIMIT: usize = 1000;

impl Session {
    /// Session on the OpenCascade kernel with the built-in commands.
    pub fn new() -> Self {
        Self::with_kernel(Arc::new(formcraft_occt::Occt))
    }

    pub fn with_kernel(kernel: Arc<dyn kernel::Kernel>) -> Self {
        let mut s = Session { kernel, registry: Registry::default(), log: Vec::new() };
        builtin::register(&mut s.registry);
        s
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
