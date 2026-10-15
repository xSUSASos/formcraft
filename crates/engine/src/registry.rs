//! Runtime command registry. Every user-visible action is a command with a JSON schema;
//! commands are registered at runtime (built-ins at startup, plugins when they load) so the UI,
//! shortcut box, AI prompt, MCP, CLI and scripts all see the same set.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{EngineError, Result, Session};

/// Who provided a command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "name", rename_all = "snake_case")]
pub enum Source {
    Builtin,
    Plugin(String),
}

/// Where a command sits in the toolbar (Fusion-style tab → group).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placement {
    /// "Solid", "Sketch", "Inspect", "Tools".
    pub tab: String,
    /// "Create", "Modify", "Construct", "Inspect", …
    pub group: String,
    /// Shown directly on the toolbar (otherwise only in the group's drop-down).
    #[serde(default)]
    pub pinned: bool,
}

/// Static description of a command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandSpec {
    /// Dotted id, e.g. `solid.extrude`, `sketch.line`, `view.fit`.
    pub id: String,
    pub label: String,
    /// One or two sentences for tooltips, the AI prompt and MCP.
    pub description: String,
    /// JSON Schema of the parameters object.
    pub params: Value,
    #[serde(default)]
    pub placement: Option<Placement>,
    #[serde(default)]
    pub shortcut: Option<String>,
    /// Icon id (icons are drawn in code; plugins may supply SVG).
    #[serde(default)]
    pub icon: Option<String>,
    /// Offered to the AI as a tool for building models.
    #[serde(default)]
    pub ai: bool,
    /// Changes the document (one undo step).
    #[serde(default)]
    pub undoable: bool,
    pub source: Source,
}

impl CommandSpec {
    pub fn new(id: &str, label: &str, description: &str) -> Self {
        CommandSpec {
            id: id.into(),
            label: label.into(),
            description: description.into(),
            params: serde_json::json!({"type": "object", "properties": {}}),
            placement: None,
            shortcut: None,
            icon: None,
            ai: false,
            undoable: false,
            source: Source::Builtin,
        }
    }
    pub fn params(mut self, schema: Value) -> Self {
        self.params = schema;
        self
    }
    pub fn place(mut self, tab: &str, group: &str, pinned: bool) -> Self {
        self.placement = Some(Placement { tab: tab.into(), group: group.into(), pinned });
        self
    }
    pub fn shortcut(mut self, s: &str) -> Self {
        self.shortcut = Some(s.into());
        self
    }
    pub fn icon(mut self, s: &str) -> Self {
        self.icon = Some(s.into());
        self
    }
    pub fn ai(mut self) -> Self {
        self.ai = true;
        self
    }
    pub fn undoable(mut self) -> Self {
        self.undoable = true;
        self
    }
    pub fn source(mut self, s: Source) -> Self {
        self.source = s;
        self
    }
}

/// Command implementation: JSON params in, JSON result out. Programmatic calls never open
/// dialogs; the UI opens the feature dialog itself and then calls the command.
pub type Handler = Arc<dyn Fn(&mut Session, &Value) -> Result<Value> + Send + Sync>;

#[derive(Clone)]
pub struct Command {
    pub spec: CommandSpec,
    pub handler: Handler,
}

/// Commands by id (sorted, so listings are stable).
#[derive(Clone, Default)]
pub struct Registry {
    commands: BTreeMap<String, Command>,
}

/// Command ids: lower-case ASCII letters, digits, `_`, `-`, dot-separated, 1..=64 bytes.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.split('.').all(|p| {
            !p.is_empty() && p.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
        })
}

impl Registry {
    /// Register a command. A duplicate id is an error (plugins cannot replace built-ins).
    pub fn register(&mut self, spec: CommandSpec, handler: Handler) -> Result<()> {
        if !valid_id(&spec.id) {
            return Err(EngineError::InvalidParams(format!("invalid command id `{}`", spec.id)));
        }
        if !spec.params.is_object() {
            return Err(EngineError::InvalidParams(format!("command `{}`: params schema must be an object", spec.id)));
        }
        if self.commands.contains_key(&spec.id) {
            return Err(EngineError::InvalidParams(format!("command `{}` is already registered", spec.id)));
        }
        self.commands.insert(spec.id.clone(), Command { spec, handler });
        Ok(())
    }

    /// Remove every command from a source (a plugin being unloaded). Returns the removed ids.
    pub fn unregister_source(&mut self, source: &Source) -> Vec<String> {
        let ids: Vec<String> =
            self.commands.values().filter(|c| &c.spec.source == source).map(|c| c.spec.id.clone()).collect();
        for id in &ids {
            self.commands.remove(id);
        }
        ids
    }

    pub fn get(&self, id: &str) -> Option<&Command> {
        self.commands.get(id)
    }

    pub fn specs(&self) -> impl Iterator<Item = &CommandSpec> {
        self.commands.values().map(|c| &c.spec)
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Commands matching a search text (id, label, description), best first: used by the
    /// shortcut box (S).
    pub fn search(&self, text: &str) -> Vec<&CommandSpec> {
        let q = text.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let mut hits: Vec<(u8, &CommandSpec)> = self
            .specs()
            .filter_map(|s| {
                let label = s.label.to_lowercase();
                let rank = if label == q {
                    0
                } else if label.starts_with(&q) {
                    1
                } else if label.split_whitespace().any(|w| w.starts_with(&q)) {
                    2
                } else if s.id.contains(&q) {
                    3
                } else if s.description.to_lowercase().contains(&q) {
                    4
                } else {
                    return None;
                };
                Some((rank, s))
            })
            .collect();
        hits.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.label.cmp(&b.1.label)));
        hits.into_iter().map(|(_, s)| s).collect()
    }
}
