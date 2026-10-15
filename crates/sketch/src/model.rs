//! Sketch geometry, constraints and dimensions.

use serde::{Deserialize, Serialize};

/// Id of a sketch entity (point, line, circle, arc). Unique within a sketch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Eid(pub u32);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Entity {
    Point {
        x: f64,
        y: f64,
    },
    Line {
        a: Eid,
        b: Eid,
        #[serde(default)]
        construction: bool,
    },
    Circle {
        center: Eid,
        radius: f64,
        #[serde(default)]
        construction: bool,
    },
    /// Counter-clockwise arc from `start` to `end` around `center`.
    Arc {
        center: Eid,
        start: Eid,
        end: Eid,
        #[serde(default)]
        construction: bool,
    },
}

impl Entity {
    pub fn is_construction(&self) -> bool {
        matches!(
            self,
            Entity::Line { construction: true, .. }
                | Entity::Circle { construction: true, .. }
                | Entity::Arc { construction: true, .. }
        )
    }
    /// Point ids this entity depends on.
    pub fn points(&self) -> Vec<Eid> {
        match *self {
            Entity::Point { .. } => vec![],
            Entity::Line { a, b, .. } => vec![a, b],
            Entity::Circle { center, .. } => vec![center],
            Entity::Arc { center, start, end, .. } => vec![center, start, end],
        }
    }
}

/// Geometric constraints (no value).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Constraint {
    Coincident {
        a: Eid,
        b: Eid,
    },
    /// Point on a line (infinite) or on a circle/arc.
    PointOn {
        point: Eid,
        curve: Eid,
    },
    Horizontal {
        line: Eid,
    },
    Vertical {
        line: Eid,
    },
    Parallel {
        a: Eid,
        b: Eid,
    },
    Perpendicular {
        a: Eid,
        b: Eid,
    },
    /// Line–circle/arc or circle–circle tangency.
    Tangent {
        a: Eid,
        b: Eid,
    },
    /// Equal length (lines) or equal radius (circles/arcs).
    Equal {
        a: Eid,
        b: Eid,
    },
    Concentric {
        a: Eid,
        b: Eid,
    },
    Midpoint {
        point: Eid,
        line: Eid,
    },
    Fix {
        point: Eid,
    },
}

impl Constraint {
    pub fn entities(&self) -> Vec<Eid> {
        match *self {
            Constraint::Coincident { a, b }
            | Constraint::Parallel { a, b }
            | Constraint::Perpendicular { a, b }
            | Constraint::Tangent { a, b }
            | Constraint::Equal { a, b }
            | Constraint::Concentric { a, b } => vec![a, b],
            Constraint::PointOn { point, curve } => vec![point, curve],
            Constraint::Horizontal { line } | Constraint::Vertical { line } => vec![line],
            Constraint::Midpoint { point, line } => vec![point, line],
            Constraint::Fix { point } => vec![point],
        }
    }
}

/// Driving dimensions. `value` is an expression in the document's parameters (mm / degrees).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DimKind {
    /// Point–point distance.
    Distance {
        a: Eid,
        b: Eid,
    },
    /// Horizontal (x) distance b.x − a.x, signed by the current geometry.
    HorizontalDistance {
        a: Eid,
        b: Eid,
    },
    VerticalDistance {
        a: Eid,
        b: Eid,
    },
    /// Length of a line.
    Length {
        line: Eid,
    },
    /// Distance from a point to a line.
    PointLine {
        point: Eid,
        line: Eid,
    },
    Radius {
        curve: Eid,
    },
    Diameter {
        curve: Eid,
    },
    /// Angle between two lines in degrees.
    Angle {
        a: Eid,
        b: Eid,
    },
}

impl DimKind {
    pub fn entities(&self) -> Vec<Eid> {
        match *self {
            DimKind::Distance { a, b }
            | DimKind::HorizontalDistance { a, b }
            | DimKind::VerticalDistance { a, b }
            | DimKind::Angle { a, b } => vec![a, b],
            DimKind::Length { line } => vec![line],
            DimKind::PointLine { point, line } => vec![point, line],
            DimKind::Radius { curve } | DimKind::Diameter { curve } => vec![curve],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dimension {
    #[serde(flatten)]
    pub kind: DimKind,
    /// Expression text (e.g. "L", "40", "hole_d/2").
    pub value: String,
}

/// A 2D sketch in plane coordinates (mm).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Sketch {
    pub entities: Vec<(Eid, Entity)>,
    #[serde(default)]
    pub constraints: Vec<Constraint>,
    #[serde(default)]
    pub dimensions: Vec<Dimension>,
    #[serde(default)]
    next: u32,
}

/// Limits for hostile input.
pub const MAX_ENTITIES: usize = 20_000;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SketchError {
    #[error("sketch entity {0} does not exist")]
    Missing(u32),
    #[error("entity {0} has the wrong type for this operation")]
    WrongType(u32),
    #[error("{0}")]
    Invalid(String),
}

impl Sketch {
    pub fn new() -> Self {
        Sketch { next: 1, ..Default::default() }
    }

    fn alloc(&mut self) -> Result<Eid, SketchError> {
        if self.entities.len() >= MAX_ENTITIES {
            return Err(SketchError::Invalid("too many sketch entities".into()));
        }
        let max = self.entities.iter().map(|(id, _)| id.0).max().unwrap_or(0);
        self.next = self.next.max(max + 1);
        let id = Eid(self.next);
        self.next += 1;
        Ok(id)
    }

    pub fn get(&self, id: Eid) -> Option<&Entity> {
        self.entities.iter().find(|(e, _)| *e == id).map(|(_, x)| x)
    }

    pub fn get_mut(&mut self, id: Eid) -> Option<&mut Entity> {
        self.entities.iter_mut().find(|(e, _)| *e == id).map(|(_, x)| x)
    }

    pub fn point(&self, id: Eid) -> Result<(f64, f64), SketchError> {
        match self.get(id) {
            Some(Entity::Point { x, y }) => Ok((*x, *y)),
            Some(_) => Err(SketchError::WrongType(id.0)),
            None => Err(SketchError::Missing(id.0)),
        }
    }

    fn require_point(&self, id: Eid) -> Result<(), SketchError> {
        self.point(id).map(|_| ())
    }

    pub fn add_point(&mut self, x: f64, y: f64) -> Result<Eid, SketchError> {
        if !(x.is_finite() && y.is_finite()) {
            return Err(SketchError::Invalid("point coordinates must be finite".into()));
        }
        let id = self.alloc()?;
        self.entities.push((id, Entity::Point { x, y }));
        Ok(id)
    }

    pub fn add_line(&mut self, a: Eid, b: Eid) -> Result<Eid, SketchError> {
        self.require_point(a)?;
        self.require_point(b)?;
        if a == b {
            return Err(SketchError::Invalid("a line needs two different points".into()));
        }
        let id = self.alloc()?;
        self.entities.push((id, Entity::Line { a, b, construction: false }));
        Ok(id)
    }

    pub fn add_circle(&mut self, center: Eid, radius: f64) -> Result<Eid, SketchError> {
        self.require_point(center)?;
        if !(radius.is_finite() && radius > 0.0) {
            return Err(SketchError::Invalid("circle radius must be > 0".into()));
        }
        let id = self.alloc()?;
        self.entities.push((id, Entity::Circle { center, radius, construction: false }));
        Ok(id)
    }

    pub fn add_arc(&mut self, center: Eid, start: Eid, end: Eid) -> Result<Eid, SketchError> {
        for p in [center, start, end] {
            self.require_point(p)?;
        }
        if start == end || start == center || end == center {
            return Err(SketchError::Invalid("an arc needs distinct centre, start and end".into()));
        }
        let id = self.alloc()?;
        self.entities.push((id, Entity::Arc { center, start, end, construction: false }));
        Ok(id)
    }

    /// Convenience: a closed axis-aligned rectangle (4 points, 4 lines, H/V constraints).
    /// Returns the line ids bottom, right, top, left.
    pub fn add_rectangle(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) -> Result<[Eid; 4], SketchError> {
        let p = [self.add_point(x0, y0)?, self.add_point(x1, y0)?, self.add_point(x1, y1)?, self.add_point(x0, y1)?];
        let l = [
            self.add_line(p[0], p[1])?,
            self.add_line(p[1], p[2])?,
            self.add_line(p[2], p[3])?,
            self.add_line(p[3], p[0])?,
        ];
        self.constraints.push(Constraint::Horizontal { line: l[0] });
        self.constraints.push(Constraint::Vertical { line: l[1] });
        self.constraints.push(Constraint::Horizontal { line: l[2] });
        self.constraints.push(Constraint::Vertical { line: l[3] });
        Ok(l)
    }

    /// Check that a constraint or dimension refers to entities of fitting types.
    pub fn check_refs(&self, ids: &[Eid]) -> Result<(), SketchError> {
        for id in ids {
            if self.get(*id).is_none() {
                return Err(SketchError::Missing(id.0));
            }
        }
        Ok(())
    }

    pub fn add_constraint(&mut self, c: Constraint) -> Result<(), SketchError> {
        self.check_refs(&c.entities())?;
        self.constraints.push(c);
        Ok(())
    }

    pub fn add_dimension(&mut self, kind: DimKind, value: &str) -> Result<(), SketchError> {
        self.check_refs(&kind.entities())?;
        if value.trim().is_empty() {
            return Err(SketchError::Invalid("dimension value is empty".into()));
        }
        self.dimensions.push(Dimension { kind, value: value.trim().to_string() });
        Ok(())
    }

    /// Delete an entity together with everything that depends on it.
    pub fn delete(&mut self, id: Eid) {
        let mut doomed = vec![id];
        loop {
            let more: Vec<Eid> = self
                .entities
                .iter()
                .filter(|(e, x)| !doomed.contains(e) && x.points().iter().any(|p| doomed.contains(p)))
                .map(|(e, _)| *e)
                .collect();
            if more.is_empty() {
                break;
            }
            doomed.extend(more);
        }
        self.entities.retain(|(e, _)| !doomed.contains(e));
        self.constraints.retain(|c| !c.entities().iter().any(|e| doomed.contains(e)));
        self.dimensions.retain(|d| !d.kind.entities().iter().any(|e| doomed.contains(e)));
    }
}
