//! Geometric constraint solver.
//!
//! Unknowns: x, y of every point and the radius of every circle (arc radii are implied by their
//! points). Each constraint/dimension contributes residual equations. The solver takes damped
//! minimum-norm Gauss–Newton steps, `dx = −Jᵀ (J Jᵀ + μ I)⁻¹ r`, so an under-constrained sketch
//! moves as little as possible from where the user drew it. Degrees of freedom = unknowns −
//! rank(J) at the solution. Everything is bounded (iterations, sizes) and NaN-safe.

use std::collections::HashMap;

use crate::model::{Constraint, DimKind, Eid, Entity, Sketch};

pub const MAX_ITERATIONS: usize = 100;
pub const TOLERANCE: f64 = 1e-9;
/// Equation-count limit for the dense solver.
pub const MAX_EQUATIONS: usize = 2_000;

#[derive(Debug, Clone, PartialEq)]
pub struct SolveReport {
    pub converged: bool,
    pub iterations: usize,
    /// Largest remaining residual (mm or radians).
    pub residual: f64,
    /// Remaining degrees of freedom (0 = fully constrained).
    pub dof: usize,
    /// Equations that are redundant or conflicting (rank deficit of the equation set).
    pub redundant: usize,
}

/// Where each unknown lives.
#[derive(Clone, Copy)]
enum Var {
    X(Eid),
    Y(Eid),
    R(Eid),
}

struct System<'a> {
    sketch: &'a Sketch,
    vars: Vec<Var>,
    index: HashMap<(u32, u8), usize>,
    /// Dimension values (mm / radians) in the order of `sketch.dimensions`.
    dims: &'a [f64],
    /// Positions of fixed points at the start of the solve.
    fixed: HashMap<Eid, (f64, f64)>,
    /// Signs captured from the start geometry (signed distances/angles, tangency side).
    signs: Vec<f64>,
}

impl System<'_> {
    fn val(&self, x: &[f64], id: Eid, k: u8) -> Option<f64> {
        self.index.get(&(id.0, k)).and_then(|i| x.get(*i)).copied()
    }
    fn p(&self, x: &[f64], id: Eid) -> Option<(f64, f64)> {
        Some((self.val(x, id, 0)?, self.val(x, id, 1)?))
    }
    fn entity(&self, id: Eid) -> Option<&Entity> {
        self.sketch.get(id)
    }
    /// Line endpoints.
    fn line(&self, x: &[f64], id: Eid) -> Option<((f64, f64), (f64, f64))> {
        match self.entity(id)? {
            Entity::Line { a, b, .. } => Some((self.p(x, *a)?, self.p(x, *b)?)),
            _ => None,
        }
    }
    /// Centre and radius of a circle or arc.
    fn circle(&self, x: &[f64], id: Eid) -> Option<((f64, f64), f64)> {
        match self.entity(id)? {
            Entity::Circle { center, .. } => Some((self.p(x, *center)?, self.val(x, id, 2)?)),
            Entity::Arc { center, start, .. } => {
                let c = self.p(x, *center)?;
                let s = self.p(x, *start)?;
                Some((c, dist(c, s)))
            }
            _ => None,
        }
    }

    /// All residuals. Equations with missing/ill-typed references contribute nothing.
    fn residuals(&self, x: &[f64], out: &mut Vec<f64>) {
        out.clear();
        let mut sign_i = 0usize;
        let mut next_sign = |signs: &[f64]| {
            let s = signs.get(sign_i).copied().unwrap_or(1.0);
            sign_i += 1;
            s
        };
        // Arcs: start and end on the same radius.
        for (id, e) in &self.sketch.entities {
            if let Entity::Arc { end, .. } = e
                && let (Some((c, r)), Some(pe)) = (self.circle(x, *id), self.p(x, *end))
            {
                out.push(dist(c, pe) - r);
            }
        }
        for c in &self.sketch.constraints {
            match *c {
                Constraint::Coincident { a, b } => match (self.p(x, a), self.p(x, b)) {
                    (Some(pa), Some(pb)) => {
                        out.push(pa.0 - pb.0);
                        out.push(pa.1 - pb.1);
                    }
                    (Some(p), None) => self.point_on(x, p, b, out),
                    (None, Some(p)) => self.point_on(x, p, a, out),
                    _ => {}
                },
                Constraint::PointOn { point, curve } => {
                    if let Some(p) = self.p(x, point) {
                        self.point_on(x, p, curve, out);
                    }
                }
                Constraint::Horizontal { line } => {
                    if let Some((a, b)) = self.line(x, line) {
                        out.push(a.1 - b.1);
                    }
                }
                Constraint::Vertical { line } => {
                    if let Some((a, b)) = self.line(x, line) {
                        out.push(a.0 - b.0);
                    }
                }
                Constraint::Parallel { a, b } => {
                    if let (Some(l1), Some(l2)) = (self.line(x, a), self.line(x, b)) {
                        let (d1, d2) = (sub(l1.1, l1.0), sub(l2.1, l2.0));
                        out.push(cross(d1, d2) / (norm(d1) * norm(d2)).max(1e-12));
                    }
                }
                Constraint::Perpendicular { a, b } => {
                    if let (Some(l1), Some(l2)) = (self.line(x, a), self.line(x, b)) {
                        let (d1, d2) = (sub(l1.1, l1.0), sub(l2.1, l2.0));
                        out.push(dot(d1, d2) / (norm(d1) * norm(d2)).max(1e-12));
                    }
                }
                Constraint::Tangent { a, b } => {
                    let s = next_sign(&self.signs);
                    match (self.line(x, a), self.line(x, b), self.circle(x, a), self.circle(x, b)) {
                        (Some(l), _, _, Some((c, r))) | (_, Some(l), Some((c, r)), _) => {
                            out.push(line_dist(c, l).abs() - r);
                        }
                        (_, _, Some((c1, r1)), Some((c2, r2))) => {
                            // s > 0: external tangency, s < 0: internal.
                            let d = dist(c1, c2);
                            out.push(if s >= 0.0 { d - (r1 + r2) } else { d - (r1 - r2).abs() });
                        }
                        _ => {}
                    }
                }
                Constraint::Equal { a, b } => match (self.line(x, a), self.line(x, b)) {
                    (Some(l1), Some(l2)) => out.push(dist(l1.0, l1.1) - dist(l2.0, l2.1)),
                    _ => {
                        if let (Some((_, r1)), Some((_, r2))) = (self.circle(x, a), self.circle(x, b)) {
                            out.push(r1 - r2);
                        }
                    }
                },
                Constraint::Concentric { a, b } => {
                    if let (Some((c1, _)), Some((c2, _))) = (self.circle(x, a), self.circle(x, b)) {
                        out.push(c1.0 - c2.0);
                        out.push(c1.1 - c2.1);
                    }
                }
                Constraint::Midpoint { point, line } => {
                    if let (Some(p), Some((a, b))) = (self.p(x, point), self.line(x, line)) {
                        out.push(p.0 - (a.0 + b.0) / 2.0);
                        out.push(p.1 - (a.1 + b.1) / 2.0);
                    }
                }
                Constraint::Fix { point } => {
                    if let (Some(p), Some(f)) = (self.p(x, point), self.fixed.get(&point)) {
                        out.push(p.0 - f.0);
                        out.push(p.1 - f.1);
                    }
                }
            }
        }
        for (i, d) in self.sketch.dimensions.iter().enumerate() {
            let v = self.dims.get(i).copied().unwrap_or(0.0);
            match d.kind {
                DimKind::Distance { a, b } => match (self.p(x, a), self.p(x, b), self.line(x, b), self.line(x, a)) {
                    (Some(pa), Some(pb), _, _) => out.push(dist(pa, pb) - v),
                    (Some(p), None, Some(l), _) | (None, Some(p), _, Some(l)) => out.push(line_dist(p, l).abs() - v),
                    _ => {}
                },
                DimKind::HorizontalDistance { a, b } => {
                    let s = next_sign(&self.signs);
                    if let (Some(pa), Some(pb)) = (self.p(x, a), self.p(x, b)) {
                        out.push(s * (pb.0 - pa.0) - v);
                    }
                }
                DimKind::VerticalDistance { a, b } => {
                    let s = next_sign(&self.signs);
                    if let (Some(pa), Some(pb)) = (self.p(x, a), self.p(x, b)) {
                        out.push(s * (pb.1 - pa.1) - v);
                    }
                }
                DimKind::Length { line } => {
                    if let Some((a, b)) = self.line(x, line) {
                        out.push(dist(a, b) - v);
                    }
                }
                DimKind::PointLine { point, line } => {
                    if let (Some(p), Some(l)) = (self.p(x, point), self.line(x, line)) {
                        out.push(line_dist(p, l).abs() - v);
                    }
                }
                DimKind::Radius { curve } => {
                    if let Some((_, r)) = self.circle(x, curve) {
                        out.push(r - v);
                    }
                }
                DimKind::Diameter { curve } => {
                    if let Some((_, r)) = self.circle(x, curve) {
                        out.push(2.0 * r - v);
                    }
                }
                DimKind::Angle { a, b } => {
                    let s = next_sign(&self.signs);
                    if let (Some(l1), Some(l2)) = (self.line(x, a), self.line(x, b)) {
                        let (d1, d2) = (sub(l1.1, l1.0), sub(l2.1, l2.0));
                        let ang = cross(d1, d2).atan2(dot(d1, d2));
                        out.push(ang - s * v);
                    }
                }
            }
        }
    }

    fn point_on(&self, x: &[f64], p: (f64, f64), curve: Eid, out: &mut Vec<f64>) {
        if let Some(l) = self.line(x, curve) {
            out.push(line_dist(p, l));
        } else if let Some((c, r)) = self.circle(x, curve) {
            out.push(dist(p, c) - r);
        }
    }
}

fn sub(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0 - b.0, a.1 - b.1)
}
fn dot(a: (f64, f64), b: (f64, f64)) -> f64 {
    a.0 * b.0 + a.1 * b.1
}
fn cross(a: (f64, f64), b: (f64, f64)) -> f64 {
    a.0 * b.1 - a.1 * b.0
}
fn norm(a: (f64, f64)) -> f64 {
    dot(a, a).sqrt()
}
fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    norm(sub(a, b))
}
/// Signed distance from p to the infinite line through l.
fn line_dist(p: (f64, f64), l: ((f64, f64), (f64, f64))) -> f64 {
    let d = sub(l.1, l.0);
    cross(d, sub(p, l.0)) / norm(d).max(1e-12)
}

/// Solve `a · y = b` (square, dense) by Gaussian elimination with partial pivoting.
fn solve_dense(mut a: Vec<f64>, mut b: Vec<f64>, n: usize) -> Option<Vec<f64>> {
    for col in 0..n {
        let piv = (col..n).max_by(|&i, &j| a[i * n + col].abs().total_cmp(&a[j * n + col].abs()))?;
        if a[piv * n + col].abs() < 1e-300 {
            return None;
        }
        if piv != col {
            for k in 0..n {
                a.swap(piv * n + k, col * n + k);
            }
            b.swap(piv, col);
        }
        let d = a[col * n + col];
        for row in col + 1..n {
            let f = a[row * n + col] / d;
            if f != 0.0 {
                for k in col..n {
                    a[row * n + k] -= f * a[col * n + k];
                }
                b[row] -= f * b[col];
            }
        }
    }
    let mut y = vec![0.0; n];
    for row in (0..n).rev() {
        let mut s = b[row];
        for k in row + 1..n {
            s -= a[row * n + k] * y[k];
        }
        y[row] = s / a[row * n + row];
    }
    y.iter().all(|v| v.is_finite()).then_some(y)
}

/// Numerical rank of an m×n row-major matrix (Gaussian elimination with full pivoting).
fn rank(mut a: Vec<f64>, m: usize, n: usize) -> usize {
    let scale = a.iter().fold(0.0f64, |s, v| s.max(v.abs())).max(1e-12);
    let tol = 1e-8 * scale;
    let mut r = 0;
    let mut cols: Vec<usize> = (0..n).collect();
    while r < m.min(n) {
        // full pivot over the remaining block
        let mut best = (r, r, 0.0f64);
        for i in r..m {
            for (cj, &j) in cols.iter().enumerate().skip(r) {
                let v = a[i * n + j].abs();
                if v > best.2 {
                    best = (i, cj, v);
                }
            }
        }
        if best.2 <= tol {
            break;
        }
        let (pi, pcj, _) = best;
        for k in 0..n {
            a.swap(pi * n + k, r * n + k);
        }
        cols.swap(r, pcj);
        let pc = cols[r];
        let d = a[r * n + pc];
        for i in r + 1..m {
            let f = a[i * n + pc] / d;
            if f != 0.0 {
                for k in 0..n {
                    a[i * n + k] -= f * a[r * n + k];
                }
            }
        }
        r += 1;
    }
    r
}

/// Build the unknown vector from the sketch.
fn unknowns(s: &Sketch) -> (Vec<Var>, HashMap<(u32, u8), usize>, Vec<f64>) {
    let mut vars = Vec::new();
    let mut index = HashMap::new();
    let mut x = Vec::new();
    for (id, e) in &s.entities {
        match e {
            Entity::Point { x: px, y: py } => {
                index.insert((id.0, 0), vars.len());
                vars.push(Var::X(*id));
                x.push(*px);
                index.insert((id.0, 1), vars.len());
                vars.push(Var::Y(*id));
                x.push(*py);
            }
            Entity::Circle { radius, .. } => {
                index.insert((id.0, 2), vars.len());
                vars.push(Var::R(*id));
                x.push(*radius);
            }
            _ => {}
        }
    }
    (vars, index, x)
}

/// Signs from the starting geometry for signed dimensions and tangency side, in the order the
/// residual function consumes them.
fn capture_signs(sys: &System<'_>, x: &[f64]) -> Vec<f64> {
    let mut signs = Vec::new();
    let sgn = |v: f64| if v < 0.0 { -1.0 } else { 1.0 };
    for c in &sys.sketch.constraints {
        if let Constraint::Tangent { a, b } = *c {
            let s = match (sys.circle(x, a), sys.circle(x, b)) {
                (Some((c1, r1)), Some((c2, r2))) => {
                    let d = dist(c1, c2);
                    if d >= r1.max(r2) { 1.0 } else { -1.0 }
                }
                _ => 1.0,
            };
            signs.push(s);
        }
    }
    for d in &sys.sketch.dimensions {
        match d.kind {
            DimKind::HorizontalDistance { a, b } => {
                signs.push(sys.p(x, a).zip(sys.p(x, b)).map(|(pa, pb)| sgn(pb.0 - pa.0)).unwrap_or(1.0))
            }
            DimKind::VerticalDistance { a, b } => {
                signs.push(sys.p(x, a).zip(sys.p(x, b)).map(|(pa, pb)| sgn(pb.1 - pa.1)).unwrap_or(1.0))
            }
            DimKind::Angle { a, b } => {
                let s = sys.line(x, a).zip(sys.line(x, b)).map(|(l1, l2)| sgn(cross(sub(l1.1, l1.0), sub(l2.1, l2.0))));
                signs.push(s.unwrap_or(1.0))
            }
            _ => {}
        }
    }
    signs
}

fn jacobian(sys: &System<'_>, x: &[f64], m: usize) -> Vec<f64> {
    let n = x.len();
    let mut j = vec![0.0; m * n];
    let mut xp = x.to_vec();
    let (mut rp, mut rm) = (Vec::with_capacity(m), Vec::with_capacity(m));
    for k in 0..n {
        let h = 1e-7 * (1.0 + x[k].abs());
        xp[k] = x[k] + h;
        sys.residuals(&xp, &mut rp);
        xp[k] = x[k] - h;
        sys.residuals(&xp, &mut rm);
        xp[k] = x[k];
        for i in 0..m.min(rp.len()).min(rm.len()) {
            j[i * n + k] = (rp[i] - rm[i]) / (2.0 * h);
        }
    }
    j
}

/// Solve the sketch in place. `dims` are the evaluated dimension values (mm; angles in
/// degrees, converted here). The sketch is left unchanged if the solve diverges.
pub fn solve(sketch: &mut Sketch, dims_deg: &[f64]) -> SolveReport {
    let dims: Vec<f64> = sketch
        .dimensions
        .iter()
        .zip(dims_deg.iter().chain(std::iter::repeat(&0.0)))
        .map(|(d, v)| if matches!(d.kind, DimKind::Angle { .. }) { v.to_radians() } else { *v })
        .collect();
    let (vars, index, mut x) = unknowns(sketch);
    let fixed: HashMap<Eid, (f64, f64)> = sketch
        .constraints
        .iter()
        .filter_map(|c| match c {
            Constraint::Fix { point } => sketch.point(*point).ok().map(|p| (*point, p)),
            _ => None,
        })
        .collect();
    let mut sys = System { sketch, vars, index, dims: &dims, fixed, signs: Vec::new() };
    sys.signs = capture_signs(&sys, &x);
    let n = x.len();
    let mut r = Vec::new();
    sys.residuals(&x, &mut r);
    let m = r.len();
    let mut report = SolveReport { converged: true, iterations: 0, residual: 0.0, dof: n, redundant: 0 };
    if m == 0 || n == 0 || m > MAX_EQUATIONS || n > MAX_EQUATIONS {
        report.converged = m == 0;
        return report;
    }
    // Non-finite residuals count as infinitely bad (f64::max would silently skip NaN).
    let norm_inf = |r: &[f64]| r.iter().fold(0.0f64, |a, v| if v.is_finite() { a.max(v.abs()) } else { f64::INFINITY });
    let mut mu = 1e-6;
    let start = x.clone();
    let mut cur = norm_inf(&r);
    for it in 0..MAX_ITERATIONS {
        report.iterations = it;
        if cur.is_nan() || cur <= TOLERANCE {
            break;
        }
        let j = jacobian(&sys, &x, m);
        // A = J Jᵀ + μ I  (m×m)
        let mut a = vec![0.0; m * m];
        for p in 0..m {
            for q in p..m {
                let s: f64 = (0..n).map(|k| j[p * n + k] * j[q * n + k]).sum();
                a[p * m + q] = s;
                a[q * m + p] = s;
            }
            a[p * m + p] += mu;
        }
        let Some(y) = solve_dense(a, r.clone(), m) else {
            mu *= 10.0;
            continue;
        };
        let mut step = vec![0.0; n];
        for k in 0..n {
            step[k] = -(0..m).map(|p| j[p * n + k] * y[p]).sum::<f64>();
        }
        let trial: Vec<f64> = x.iter().zip(&step).map(|(a, b)| a + b).collect();
        let mut rt = Vec::new();
        sys.residuals(&trial, &mut rt);
        let nt = norm_inf(&rt);
        if nt.is_finite() && nt < cur {
            x = trial;
            r = rt;
            cur = nt;
            mu = (mu * 0.3).max(1e-12);
        } else {
            mu *= 10.0;
            if mu > 1e8 {
                break;
            }
        }
    }
    report.residual = cur;
    report.converged = cur <= 1e-7;
    let j = jacobian(&sys, &x, m);
    let rk = rank(j, m, n);
    report.dof = n - rk;
    report.redundant = m - rk;
    if !report.converged {
        x = start;
    }
    // Write back.
    let values: Vec<(Var, f64)> = sys.vars.iter().copied().zip(x.iter().copied()).collect();
    for (v, val) in values {
        match v {
            Var::X(id) => {
                if let Some(Entity::Point { x, .. }) = sketch.get_mut(id) {
                    *x = val;
                }
            }
            Var::Y(id) => {
                if let Some(Entity::Point { y, .. }) = sketch.get_mut(id) {
                    *y = val;
                }
            }
            Var::R(id) => {
                if let Some(Entity::Circle { radius, .. }) = sketch.get_mut(id) {
                    *radius = val.abs().max(1e-9);
                }
            }
        }
    }
    report
}
