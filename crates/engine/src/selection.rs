//! Selection of bodies, faces and edges.

use formcraft_doc::BodyId;
use formcraft_regen::Model;
use serde::{Deserialize, Serialize};

/// Something the user picked on the canvas or in the browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Pick {
    Body { body: BodyId },
    Face { body: BodyId, index: u32 },
    Edge { body: BodyId, index: u32 },
}

impl Pick {
    pub fn body(&self) -> BodyId {
        match *self {
            Pick::Body { body } | Pick::Face { body, .. } | Pick::Edge { body, .. } => body,
        }
    }
}

pub const MAX_SELECTION: usize = 10_000;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Selection {
    pub items: Vec<Pick>,
    /// Pre-selection under the cursor (highlight only).
    pub hover: Option<Pick>,
}

impl Selection {
    pub fn clear(&mut self) {
        self.items.clear();
    }
    pub fn contains(&self, p: &Pick) -> bool {
        self.items.contains(p)
    }
    pub fn set(&mut self, p: Pick) {
        self.items.clear();
        self.items.push(p);
    }
    /// Add or remove (Ctrl/Shift-click).
    pub fn toggle(&mut self, p: Pick) {
        if let Some(i) = self.items.iter().position(|x| *x == p) {
            self.items.remove(i);
        } else if self.items.len() < MAX_SELECTION {
            self.items.push(p);
        }
    }
    pub fn bodies(&self) -> Vec<BodyId> {
        let mut out: Vec<BodyId> = Vec::new();
        for p in &self.items {
            if let Pick::Body { body } = p
                && !out.contains(body)
            {
                out.push(*body);
            }
        }
        out
    }
    /// After regeneration: keep only picks whose body still exists. Face and edge indices are
    /// not stable across regenerations, so sub-body picks are dropped.
    pub fn retain_bodies(&mut self, m: &Model) {
        self.items.retain(|p| matches!(p, Pick::Body { .. }) && m.body(p.body()).is_some());
        self.hover = None;
    }
}
