//! Planar profiles: closed loops of lines, arcs and circles on a plane.

use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};

use crate::{KernelError, Result, check_dir, check_finite, check_positive};

/// Tolerance for loop closure in sketch units (mm).
pub const CLOSE_TOL: f64 = 1e-6;

/// A plane with an in-plane coordinate system: `x_dir` × `y_dir()` = `normal`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Plane {
    pub origin: DVec3,
    pub x_dir: DVec3,
    pub normal: DVec3,
}

impl Plane {
    /// Plane with orthonormalised axes; errors on degenerate input.
    pub fn new(origin: DVec3, x_dir: DVec3, normal: DVec3) -> Result<Self> {
        let n = check_dir("plane normal", normal)?;
        let x = check_dir("plane x direction", x_dir)?;
        let x = x - n * x.dot(n);
        let x = check_dir("plane x direction (parallel to normal)", x)?;
        Ok(Plane { origin: check_finite("plane origin", origin)?, x_dir: x, normal: n })
    }
    pub const XY: Plane = Plane { origin: DVec3::ZERO, x_dir: DVec3::X, normal: DVec3::Z };
    /// XZ plane as Fusion shows it (front view): x → X, y → Z, normal → −Y.
    pub const XZ: Plane = Plane { origin: DVec3::ZERO, x_dir: DVec3::X, normal: DVec3::NEG_Y };
    pub const YZ: Plane = Plane { origin: DVec3::ZERO, x_dir: DVec3::Y, normal: DVec3::X };

    pub fn y_dir(&self) -> DVec3 {
        self.normal.cross(self.x_dir)
    }
    pub fn to_world(&self, p: DVec2) -> DVec3 {
        self.origin + self.x_dir * p.x + self.y_dir() * p.y
    }
    pub fn to_local(&self, p: DVec3) -> DVec2 {
        let d = p - self.origin;
        DVec2::new(d.dot(self.x_dir), d.dot(self.y_dir()))
    }
    pub fn offset(&self, distance: f64) -> Plane {
        Plane { origin: self.origin + self.normal * distance, ..*self }
    }
}

/// A 2D curve segment in plane coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Segment2 {
    Line {
        a: DVec2,
        b: DVec2,
    },
    /// Circular arc through three points.
    Arc {
        start: DVec2,
        mid: DVec2,
        end: DVec2,
    },
    /// Full circle (a loop on its own).
    Circle {
        center: DVec2,
        radius: f64,
    },
}

impl Segment2 {
    pub fn start(&self) -> DVec2 {
        match *self {
            Segment2::Line { a, .. } => a,
            Segment2::Arc { start, .. } => start,
            Segment2::Circle { center, radius } => center + DVec2::new(radius, 0.0),
        }
    }
    pub fn end(&self) -> DVec2 {
        match *self {
            Segment2::Line { b, .. } => b,
            Segment2::Arc { end, .. } => end,
            Segment2::Circle { center, radius } => center + DVec2::new(radius, 0.0),
        }
    }
    fn validate(&self) -> Result<()> {
        let fin = |p: DVec2| p.is_finite();
        match *self {
            Segment2::Line { a, b } => {
                if !fin(a) || !fin(b) || a.distance(b) < CLOSE_TOL {
                    return Err(KernelError::InvalidInput("degenerate line segment".into()));
                }
            }
            Segment2::Arc { start, mid, end } => {
                let cross = (mid - start).perp_dot(end - start);
                if !fin(start) || !fin(mid) || !fin(end) || cross.abs() < CLOSE_TOL * CLOSE_TOL {
                    return Err(KernelError::InvalidInput("degenerate arc (collinear points)".into()));
                }
            }
            Segment2::Circle { center, radius } => {
                if !fin(center) {
                    return Err(KernelError::InvalidInput("circle center is not finite".into()));
                }
                check_positive("circle radius", radius)?;
            }
        }
        Ok(())
    }
}

/// A closed loop of segments, in order, end of each = start of the next.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Loop2 {
    pub segments: Vec<Segment2>,
}

impl Loop2 {
    pub fn new(segments: Vec<Segment2>) -> Self {
        Loop2 { segments }
    }
    /// Closed polygon through `points` (the closing edge is added).
    pub fn polygon(points: &[DVec2]) -> Self {
        let n = points.len();
        let segments =
            (0..n).filter_map(|i| Some(Segment2::Line { a: *points.get(i)?, b: *points.get((i + 1) % n)? })).collect();
        Loop2 { segments }
    }
    pub fn rect(min: DVec2, max: DVec2) -> Self {
        Self::polygon(&[min, DVec2::new(max.x, min.y), max, DVec2::new(min.x, max.y)])
    }
    pub fn circle(center: DVec2, radius: f64) -> Self {
        Loop2 { segments: vec![Segment2::Circle { center, radius }] }
    }
    pub fn validate(&self) -> Result<()> {
        if self.segments.is_empty() {
            return Err(KernelError::InvalidInput("empty loop".into()));
        }
        let has_circle = self.segments.iter().any(|s| matches!(s, Segment2::Circle { .. }));
        if has_circle && self.segments.len() > 1 {
            return Err(KernelError::InvalidInput("a circle must be a loop on its own".into()));
        }
        for s in &self.segments {
            s.validate()?;
        }
        let n = self.segments.len();
        for i in 0..n {
            let (Some(a), Some(b)) = (self.segments.get(i), self.segments.get((i + 1) % n)) else { continue };
            if a.end().distance(b.start()) > CLOSE_TOL {
                return Err(KernelError::InvalidInput(format!("loop is not closed after segment {i}")));
            }
        }
        Ok(())
    }
}

/// One closed region: an outer loop and optional holes.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Region {
    pub outer: Loop2,
    #[serde(default)]
    pub holes: Vec<Loop2>,
}

/// Planar profile: one or more regions on a plane.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub plane: Plane,
    pub regions: Vec<Region>,
}

impl Profile {
    pub fn new(plane: Plane, outer: Loop2) -> Self {
        Profile { plane, regions: vec![Region { outer, holes: Vec::new() }] }
    }
    pub fn with_hole(mut self, hole: Loop2) -> Self {
        if let Some(r) = self.regions.last_mut() {
            r.holes.push(hole);
        }
        self
    }
    pub fn validate(&self) -> Result<()> {
        if self.regions.is_empty() {
            return Err(KernelError::InvalidInput("profile has no regions".into()));
        }
        for r in &self.regions {
            r.outer.validate()?;
            for h in &r.holes {
                h.validate()?;
            }
        }
        Ok(())
    }
}
