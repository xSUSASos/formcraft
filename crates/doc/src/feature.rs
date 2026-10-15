//! Features: the steps of the timeline.

use serde::{Deserialize, Serialize};

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
    /// Boolean between two existing bodies; the tool body is consumed unless `keep_tool`.
    Combine {
        target: BodyId,
        tool: BodyId,
        operation: CombineOp,
        #[serde(default)]
        keep_tool: bool,
    },
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
        }
    }

    pub fn op(&self) -> Option<BodyOp> {
        match self {
            FeatureKind::Box { op, .. } | FeatureKind::Cylinder { op, .. } | FeatureKind::Sphere { op, .. } => {
                Some(*op)
            }
            FeatureKind::Combine { .. } => None,
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
