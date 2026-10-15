//! Persistent references to faces and edges (topological naming).
//!
//! Face and edge indices change whenever the model regenerates, so features never store raw
//! indices. They store selectors that `regen` resolves against the current body:
//!
//! * semantic names an AI or a script can write: `"top"`, `"+X"`, `"largest_planar"`,
//!   `"vertical"`, `"circular"`, …;
//! * a point the face/edge passes near (`{"near": [x, y, z]}`);
//! * a geometric signature recorded when the user clicked something (`{"face": {...}}`);
//! * several of the above (a JSON array).

use serde::{Deserialize, Serialize};

/// Geometric signature of a face, recorded from a pick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FaceSignature {
    /// "plane", "cylinder", "cone", "sphere", "torus", "other".
    pub kind: String,
    pub center: [f64; 3],
    pub normal: [f64; 3],
    pub area: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeSignature {
    /// "line", "circle", "other".
    pub kind: String,
    pub mid: [f64; 3],
    pub length: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FaceSel {
    /// `top`, `bottom`, `+X`, `-X`, `+Y`, `-Y`, `+Z`, `-Z`, `largest_planar`,
    /// `smallest_planar`, `cylindrical`, `planar`, `all`.
    Name(String),
    Near {
        near: [f64; 3],
    },
    Signature {
        face: FaceSignature,
    },
    Many(Vec<FaceSel>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EdgeSel {
    /// `all`, `vertical`, `horizontal`, `circular`, `linear`, `top`, `bottom`,
    /// `top_outer`, `bottom_outer`.
    Name(String),
    /// Edges bounding the selected face(s).
    Of {
        edges_of: Box<FaceSel>,
    },
    Near {
        near: [f64; 3],
    },
    Signature {
        edge: EdgeSignature,
    },
    Many(Vec<EdgeSel>),
}

pub const FACE_NAMES: &[&str] = &[
    "top",
    "bottom",
    "+X",
    "-X",
    "+Y",
    "-Y",
    "+Z",
    "-Z",
    "largest_planar",
    "smallest_planar",
    "cylindrical",
    "planar",
    "all",
];
pub const EDGE_NAMES: &[&str] =
    &["all", "vertical", "horizontal", "circular", "linear", "top", "bottom", "top_outer", "bottom_outer"];

impl FaceSel {
    pub fn name(s: &str) -> Self {
        FaceSel::Name(s.into())
    }
}

impl EdgeSel {
    pub fn name(s: &str) -> Self {
        EdgeSel::Name(s.into())
    }
}
