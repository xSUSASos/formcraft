//! Features: the steps of the timeline.

use serde::{Deserialize, Serialize};

use formcraft_sketch::{Eid, Sketch};

use crate::select::{EdgeSel, FaceSel};
use crate::{BodyId, Expr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FeatureId(pub u32);

/// A 3D vector of expressions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Vec3Expr(pub [Expr; 3]);

impl Vec3Expr {
    pub fn num(x: f64, y: f64, z: f64) -> Self {
        Vec3Expr([Expr::num(x), Expr::num(y), Expr::num(z)])
    }
}

/// What a feature does with its result (Fusion's "Operation").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "type", content = "target", rename_all = "snake_case")]
pub enum BodyOp {
    #[default]
    NewBody,
    Join(BodyId),
    Cut(BodyId),
    Intersect(BodyId),
}

impl BodyOp {
    pub fn target(&self) -> Option<BodyId> {
        match *self {
            BodyOp::NewBody => None,
            BodyOp::Join(b) | BodyOp::Cut(b) | BodyOp::Intersect(b) => Some(b),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FeatureKind {
    /// Axis-aligned box from a corner.
    Box {
        corner: Vec3Expr,
        size: Vec3Expr,
        #[serde(default)]
        op: BodyOp,
    },
    Cylinder {
        base: Vec3Expr,
        /// Direction of the axis (not normalised, any length).
        axis: Vec3Expr,
        radius: Expr,
        height: Expr,
        #[serde(default)]
        op: BodyOp,
    },
    Sphere {
        center: Vec3Expr,
        radius: Expr,
        #[serde(default)]
        op: BodyOp,
    },
    Sketch {
        plane: SketchPlane,
        sketch: Sketch,
    },
    Extrude {
        sketch: FeatureId,
        #[serde(default, skip_serializing_if = "ProfileSel::is_all")]
        profiles: ProfileSel,
        distance: Expr,
        #[serde(default)]
        direction: ExtrudeDirection,
        /// Cut through everything (ignores `distance`).
        #[serde(default)]
        through_all: bool,
        #[serde(default)]
        op: BodyOp,
    },
    Revolve {
        sketch: FeatureId,
        #[serde(default, skip_serializing_if = "ProfileSel::is_all")]
        profiles: ProfileSel,
        axis: RevolveAxis,
        #[serde(default = "full_turn")]
        angle: Expr,
        #[serde(default)]
        op: BodyOp,
    },
    Fillet {
        body: BodyId,
        edges: EdgeSel,
        radius: Expr,
    },
    Chamfer {
        body: BodyId,
        edges: EdgeSel,
        distance: Expr,
    },
    /// Hollow a body: remove `faces` (open sides) and keep walls of `thickness`.
    Shell {
        body: BodyId,
        #[serde(default = "no_faces")]
        faces: FaceSel,
        thickness: Expr,
    },
    /// Holes drilled into `face` at `points` (3D, projected onto the face plane).
    Hole {
        body: BodyId,
        face: FaceSel,
        points: Vec<Vec3Expr>,
        diameter: Expr,
        /// None = through all.
        #[serde(default)]
        depth: Option<Expr>,
        /// Counterbore (diameter, depth).
        #[serde(default)]
        counterbore: Option<(Expr, Expr)>,
        /// Countersink (diameter, included angle in degrees).
        #[serde(default)]
        countersink: Option<(Expr, Expr)>,
    },
    /// Push or pull a planar face along its normal (adds or removes material).
    PressPull {
        body: BodyId,
        face: FaceSel,
        distance: Expr,
    },
    /// A solid imported from a file (STEP): the base body, stored as base64 BREP.
    Import {
        source: String,
        brep: String,
    },
    /// A plugin's feature (ADR 0003): rebuilt by the plugin from `params` (numbers may be
    /// expressions). `frozen` keeps the last good body (base64 BREP) so the feature still
    /// shows its result when the plugin is missing.
    Custom {
        plugin: String,
        kind: String,
        #[serde(default)]
        params: serde_json::Value,
        #[serde(default)]
        op: BodyOp,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        frozen: Option<String>,
    },
    /// Boolean between two existing bodies; the tool body is consumed unless `keep_tool`.
    Combine {
        target: BodyId,
        tool: BodyId,
        operation: CombineOp,
        #[serde(default)]
        keep_tool: bool,
    },
}

/// Where a sketch lives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SketchPlane {
    /// An origin plane ("XY", "XZ", "YZ") moved along its normal by `offset`.
    Origin {
        name: String,
        #[serde(default = "zero")]
        offset: Expr,
    },
    /// A planar face of a body (resolved at every regeneration).
    Face {
        body: BodyId,
        face: FaceSel,
        #[serde(default = "zero")]
        offset: Expr,
    },
}

fn zero() -> Expr {
    Expr::num(0.0)
}

fn full_turn() -> Expr {
    Expr::num(360.0)
}

/// Which closed regions of a sketch a feature uses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(untagged)]
pub enum ProfileSel {
    /// Outer regions with their holes (even nesting depth).
    #[default]
    All,
    /// Regions containing these sketch-plane points (stable across edits).
    Points { points: Vec<[f64; 2]> },
    /// Regions by index in the current region list.
    Indices { indices: Vec<usize> },
}

impl ProfileSel {
    pub fn is_all(&self) -> bool {
        matches!(self, ProfileSel::All)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExtrudeDirection {
    /// Along the sketch normal (negative distance goes the other way).
    #[default]
    OneSide,
    /// Half the distance to each side.
    Symmetric,
}

/// Axis of a revolve.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RevolveAxis {
    /// "X", "Y", "Z" (world axes through the origin) or "sketch_x", "sketch_y".
    Name(String),
    /// A line of the sketch.
    Line { line: Eid },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombineOp {
    Join,
    Cut,
    Intersect,
}

impl FeatureKind {
    /// Default name prefix on the timeline.
    pub fn base_name(&self) -> &'static str {
        match self {
            FeatureKind::Box { .. } => "Box",
            FeatureKind::Cylinder { .. } => "Cylinder",
            FeatureKind::Sphere { .. } => "Sphere",
            FeatureKind::Combine { .. } => "Combine",
            FeatureKind::Sketch { .. } => "Sketch",
            FeatureKind::Extrude { .. } => "Extrude",
            FeatureKind::Revolve { .. } => "Revolve",
            FeatureKind::Fillet { .. } => "Fillet",
            FeatureKind::Chamfer { .. } => "Chamfer",
            FeatureKind::Shell { .. } => "Shell",
            FeatureKind::Hole { .. } => "Hole",
            FeatureKind::PressPull { .. } => "PressPull",
            FeatureKind::Custom { .. } => "Custom",
            FeatureKind::Import { .. } => "Import",
        }
    }

    pub fn op(&self) -> Option<BodyOp> {
        match self {
            FeatureKind::Box { op, .. }
            | FeatureKind::Cylinder { op, .. }
            | FeatureKind::Sphere { op, .. }
            | FeatureKind::Extrude { op, .. }
            | FeatureKind::Revolve { op, .. }
            | FeatureKind::Custom { op, .. } => Some(*op),
            FeatureKind::Import { .. } => Some(BodyOp::NewBody),
            _ => None,
        }
    }

    /// Whether this feature makes a new body (named after it).
    pub fn creates_body(&self) -> bool {
        self.op() == Some(BodyOp::NewBody)
    }

    /// Bodies this feature reads or modifies.
    pub fn body_refs(&self) -> Vec<BodyId> {
        match self {
            FeatureKind::Combine { target, tool, .. } => vec![*target, *tool],
            FeatureKind::Fillet { body, .. }
            | FeatureKind::Chamfer { body, .. }
            | FeatureKind::Shell { body, .. }
            | FeatureKind::Hole { body, .. }
            | FeatureKind::PressPull { body, .. } => vec![*body],
            FeatureKind::Sketch { plane: SketchPlane::Face { body, .. }, .. } => vec![*body],
            k => k.op().and_then(|o| o.target()).into_iter().collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Feature {
    pub id: FeatureId,
    pub name: String,
    #[serde(default)]
    pub suppressed: bool,
    /// Features added together (e.g. by the AI) share a group; the timeline shows them as one.
    #[serde(default)]
    pub group: Option<u32>,
    #[serde(flatten)]
    pub kind: FeatureKind,
}

fn no_faces() -> FaceSel {
    FaceSel::Many(Vec::new())
}
