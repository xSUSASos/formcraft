//! Document model: features on a timeline, user parameters with expressions, bodies.
//!
//! A document is plain data (serde): the timeline is the source of truth and bodies are
//! produced by `regen`. Documents are cheap to clone (copy-on-write undo keeps whole snapshots
//! behind `Arc`).
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod expr;
mod feature;
pub mod select;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub use expr::{Expr, ExprError};
pub use feature::{
    BodyOp, CombineOp, ExtrudeDirection, Feature, FeatureId, FeatureKind, ProfileSel, RevolveAxis, SketchPlane,
    Vec3Expr,
};
pub use select::{EdgeSel, FaceSel};

/// A body is identified by the feature that created it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BodyId(pub u32);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub expr: Expr,
    #[serde(default)]
    pub comment: String,
}

/// Per-body display state kept in the document (names and visibility survive regeneration).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BodyState {
    pub name: String,
    #[serde(default = "yes")]
    pub visible: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DocError {
    #[error("no feature with id {0}")]
    NoFeature(u32),
    #[error("invalid parameter name `{0}` (letters, digits and `_`, not starting with a digit)")]
    BadName(String),
    #[error("parameter `{0}` already exists")]
    Duplicate(String),
    #[error("parameter `{0}`: {1}")]
    Param(String, ExprError),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, DocError>;

/// Limits that keep hostile input from exhausting memory.
pub const MAX_FEATURES: usize = 10_000;
pub const MAX_PARAMETERS: usize = 2_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Document {
    pub name: String,
    /// Always "mm" in the MVP.
    pub units: String,
    pub parameters: Vec<Parameter>,
    pub features: Vec<Feature>,
    /// Timeline marker: features at index >= `marker` are rolled back (not computed).
    pub marker: Option<usize>,
    pub bodies: HashMap<BodyId, BodyState>,
    next_id: u32,
}

impl Default for Document {
    fn default() -> Self {
        Document {
            name: "Untitled".into(),
            units: "mm".into(),
            parameters: Vec::new(),
            features: Vec::new(),
            marker: None,
            bodies: HashMap::new(),
            next_id: 1,
        }
    }
}

pub fn valid_name(n: &str) -> bool {
    let mut c = n.chars();
    matches!(c.next(), Some(ch) if ch.is_ascii_alphabetic() || ch == '_')
        && c.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        && n.len() <= 64
        && !matches!(n, "pi" | "e" | "mm" | "cm" | "m" | "in" | "ft" | "deg" | "rad")
}

impl Document {
    pub fn new(name: &str) -> Self {
        Document { name: name.into(), ..Default::default() }
    }

    /// Features that take part in regeneration (before the marker).
    pub fn active_features(&self) -> &[Feature] {
        let end = self.marker.unwrap_or(self.features.len()).min(self.features.len());
        self.features.get(..end).unwrap_or_default()
    }

    pub fn feature(&self, id: FeatureId) -> Option<&Feature> {
        self.features.iter().find(|f| f.id == id)
    }

    pub fn feature_mut(&mut self, id: FeatureId) -> Option<&mut Feature> {
        self.features.iter_mut().find(|f| f.id == id)
    }

    pub fn index_of(&self, id: FeatureId) -> Option<usize> {
        self.features.iter().position(|f| f.id == id)
    }

    /// A unique default name like `Extrude3`.
    pub fn next_name(&self, base: &str) -> String {
        (1..)
            .map(|i| format!("{base}{i}"))
            .find(|n| !self.features.iter().any(|f| &f.name == n))
            .unwrap_or_else(|| base.to_string())
    }

    /// Append a feature at the marker (like Fusion: new features go where the marker is).
    pub fn add_feature(&mut self, kind: FeatureKind) -> Result<FeatureId> {
        if self.features.len() >= MAX_FEATURES {
            return Err(DocError::Invalid("too many features".into()));
        }
        let id = FeatureId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        let name = self.next_name(kind.base_name());
        if kind.creates_body() {
            let n = self.bodies.len() + 1;
            self.bodies.insert(BodyId(id.0), BodyState { name: format!("Body{n}"), visible: true });
        }
        let at = self.marker.unwrap_or(self.features.len()).min(self.features.len());
        self.features.insert(at, Feature { id, name, suppressed: false, group: None, kind });
        if let Some(m) = self.marker.as_mut() {
            *m += 1;
        }
        Ok(id)
    }

    pub fn delete_feature(&mut self, id: FeatureId) -> Result<()> {
        let i = self.index_of(id).ok_or(DocError::NoFeature(id.0))?;
        self.features.remove(i);
        self.bodies.remove(&BodyId(id.0));
        if let Some(m) = self.marker.as_mut()
            && *m > i
        {
            *m -= 1;
        }
        Ok(())
    }

    /// Move a feature to a new timeline index.
    pub fn reorder(&mut self, id: FeatureId, to: usize) -> Result<()> {
        let i = self.index_of(id).ok_or(DocError::NoFeature(id.0))?;
        let f = self.features.remove(i);
        let to = to.min(self.features.len());
        self.features.insert(to, f);
        Ok(())
    }

    pub fn set_marker(&mut self, at: Option<usize>) {
        self.marker = at.filter(|m| *m < self.features.len());
    }

    // ------------------------------------------------------------------ parameters

    pub fn parameter(&self, name: &str) -> Option<&Parameter> {
        self.parameters.iter().find(|p| p.name == name)
    }

    /// Add or update a parameter. The whole table is re-checked (unknown names, cycles).
    pub fn set_parameter(&mut self, name: &str, expr: Expr, comment: Option<&str>) -> Result<()> {
        if !valid_name(name) {
            return Err(DocError::BadName(name.into()));
        }
        let mut next = self.parameters.clone();
        match next.iter_mut().find(|p| p.name == name) {
            Some(p) => {
                p.expr = expr;
                if let Some(c) = comment {
                    p.comment = c.into();
                }
            }
            None => {
                if next.len() >= MAX_PARAMETERS {
                    return Err(DocError::Invalid("too many parameters".into()));
                }
                next.push(Parameter { name: name.into(), expr, comment: comment.unwrap_or("").into() })
            }
        }
        let table: Vec<(String, Expr)> = next.iter().map(|p| (p.name.clone(), p.expr.clone())).collect();
        expr::eval_all(&table).map_err(|(n, e)| DocError::Param(n, e))?;
        self.parameters = next;
        Ok(())
    }

    pub fn remove_parameter(&mut self, name: &str) -> Result<()> {
        let used_by_param =
            self.parameters.iter().any(|p| p.name != name && p.expr.references().iter().any(|r| r == name));
        if used_by_param {
            return Err(DocError::Invalid(format!("parameter `{name}` is used by another parameter")));
        }
        let before = self.parameters.len();
        self.parameters.retain(|p| p.name != name);
        if self.parameters.len() == before {
            return Err(DocError::Invalid(format!("no parameter `{name}`")));
        }
        Ok(())
    }

    /// Values of all parameters.
    pub fn parameter_values(&self) -> Result<HashMap<String, f64>> {
        let table: Vec<(String, Expr)> = self.parameters.iter().map(|p| (p.name.clone(), p.expr.clone())).collect();
        expr::eval_all(&table).map_err(|(n, e)| DocError::Param(n, e))
    }

    /// Evaluate an expression against the document's parameters.
    pub fn eval(&self, e: &Expr) -> std::result::Result<f64, ExprError> {
        let vals = self.parameter_values().unwrap_or_default();
        e.eval(&|n| vals.get(n).copied())
    }

    pub fn body_name(&self, b: BodyId) -> String {
        self.bodies.get(&b).map(|s| s.name.clone()).unwrap_or_else(|| format!("Body#{}", b.0))
    }

    /// JSON form (inside `.fcraft`).
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|e| DocError::Invalid(e.to_string()))
    }

    pub fn from_json(s: &str) -> Result<Self> {
        let mut d: Document = serde_json::from_str(s).map_err(|e| DocError::Invalid(e.to_string()))?;
        if d.features.len() > MAX_FEATURES || d.parameters.len() > MAX_PARAMETERS {
            return Err(DocError::Invalid("document is too large".into()));
        }
        let max = d.features.iter().map(|f| f.id.0).max().unwrap_or(0);
        d.next_id = d.next_id.max(max.saturating_add(1));
        d.set_marker(d.marker);
        Ok(d)
    }
}

#[cfg(test)]
mod tests;
