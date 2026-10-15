//! Closed regions ("profiles") of a sketch.
//!
//! Lines and arcs that meet at their end points form a planar graph. Its bounded faces are
//! traced with half-edges (keeping the face on the left, turning as far right as possible at
//! each vertex); dangling edges are pruned first. Full circles are faces of their own. A
//! connected component lying inside a face becomes a hole of the smallest face containing it,
//! and the hole's inside is again a profile. Intersections between curves are not split (lines
//! must meet at end points), which matches how most sketches are drawn.

use std::collections::HashMap;
use std::f64::consts::TAU;

use formcraft_kernel::{DVec2, Loop2, Segment2};
use serde::Serialize;

use crate::model::{Eid, Entity, Sketch};

const MERGE_TOL: f64 = 1e-6;
const ARC_STEPS: usize = 24;

/// A closed region: outer boundary and holes, in sketch-plane coordinates.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Region {
    pub outer: Loop2,
    pub holes: Vec<Loop2>,
    /// Area of the region (outer minus holes), mm².
    pub area: f64,
    /// Nesting depth: 0 for regions not inside another region.
    pub depth: usize,
    /// A point strictly inside the region (stable selector for regeneration).
    pub sample: DVec2,
}

#[derive(Debug, Clone, Copy)]
enum Edge {
    Line {
        a: usize,
        b: usize,
    },
    /// CCW arc from `a` to `b` around `c` with radius `r`.
    Arc {
        a: usize,
        b: usize,
        c: DVec2,
        r: f64,
    },
}

impl Edge {
    fn ends(&self) -> (usize, usize) {
        match *self {
            Edge::Line { a, b } | Edge::Arc { a, b, .. } => (a, b),
        }
    }
}

/// One traversal step: edge index and direction (true = a→b).
type Half = (usize, bool);

struct Graph {
    verts: Vec<DVec2>,
    edges: Vec<Edge>,
}

impl Graph {
    fn from(s: &Sketch) -> (Graph, Vec<(DVec2, f64)>) {
        let mut verts: Vec<DVec2> = Vec::new();
        let mut vmap: HashMap<Eid, usize> = HashMap::new();
        let mut vert = |id: Eid, s: &Sketch, verts: &mut Vec<DVec2>| -> Option<usize> {
            if let Some(v) = vmap.get(&id) {
                return Some(*v);
            }
            let (x, y) = s.point(id).ok()?;
            let p = DVec2::new(x, y);
            let i = match verts.iter().position(|q| q.distance(p) < MERGE_TOL) {
                Some(i) => i,
                None => {
                    verts.push(p);
                    verts.len() - 1
                }
            };
            vmap.insert(id, i);
            Some(i)
        };
        let mut edges = Vec::new();
        let mut circles = Vec::new();
        for (_, e) in &s.entities {
            if e.is_construction() {
                continue;
            }
            match *e {
                Entity::Line { a, b, .. } => {
                    if let (Some(a), Some(b)) = (vert(a, s, &mut verts), vert(b, s, &mut verts))
                        && a != b
                    {
                        edges.push(Edge::Line { a, b });
                    }
                }
                Entity::Arc { center, start, end, .. } => {
                    let (Ok(c), Some(a), Some(b)) =
                        (s.point(center), vert(start, s, &mut verts), vert(end, s, &mut verts))
                    else {
                        continue;
                    };
                    let c = DVec2::new(c.0, c.1);
                    let r = verts.get(a).map(|p| p.distance(c)).unwrap_or(0.0);
                    if a != b && r > MERGE_TOL {
                        edges.push(Edge::Arc { a, b, c, r });
                    }
                }
                Entity::Circle { center, radius, .. } => {
                    if let Ok(c) = s.point(center)
                        && radius > MERGE_TOL
                    {
                        circles.push((DVec2::new(c.0, c.1), radius));
                    }
                }
                Entity::Point { .. } => {}
            }
        }
        (Graph { verts, edges }, circles)
    }

    fn vert(&self, i: usize) -> DVec2 {
        self.verts.get(i).copied().unwrap_or(DVec2::ZERO)
    }

    /// Direction leaving the start of a half-edge (tangent).
    fn out_dir(&self, h: Half) -> DVec2 {
        let Some(e) = self.edges.get(h.0) else { return DVec2::X };
        match (*e, h.1) {
            (Edge::Line { a, b }, true) => self.vert(b) - self.vert(a),
            (Edge::Line { a, b }, false) => self.vert(a) - self.vert(b),
            (Edge::Arc { a, c, .. }, true) => (self.vert(a) - c).perp(),
            (Edge::Arc { b, c, .. }, false) => -(self.vert(b) - c).perp(),
        }
    }

    fn start(&self, h: Half) -> usize {
        let (a, b) = self.edges.get(h.0).map(Edge::ends).unwrap_or((0, 0));
        if h.1 { a } else { b }
    }

    fn end(&self, h: Half) -> usize {
        let (a, b) = self.edges.get(h.0).map(Edge::ends).unwrap_or((0, 0));
        if h.1 { b } else { a }
    }

    /// Remove edges that hang off the graph (degree-1 vertices), repeatedly.
    fn prune(&mut self) {
        loop {
            let mut deg = vec![0usize; self.verts.len()];
            for e in &self.edges {
                let (a, b) = e.ends();
                if let Some(d) = deg.get_mut(a) {
                    *d += 1;
                }
                if let Some(d) = deg.get_mut(b) {
                    *d += 1;
                }
            }
            let before = self.edges.len();
            self.edges.retain(|e| {
                let (a, b) = e.ends();
                deg.get(a).copied().unwrap_or(0) > 1 && deg.get(b).copied().unwrap_or(0) > 1
            });
            if self.edges.len() == before {
                break;
            }
        }
    }

    /// Polyline of a half-edge (start point excluded).
    fn sample(&self, h: Half, out: &mut Vec<DVec2>) {
        let Some(e) = self.edges.get(h.0) else { return };
        match *e {
            Edge::Line { .. } => out.push(self.vert(self.end(h))),
            Edge::Arc { a, b, c, r } => {
                let a0 = (self.vert(a) - c).to_angle();
                let mut sweep = (self.vert(b) - c).to_angle() - a0;
                while sweep <= 0.0 {
                    sweep += TAU;
                }
                for k in 1..=ARC_STEPS {
                    let t = if h.1 { k as f64 / ARC_STEPS as f64 } else { 1.0 - k as f64 / ARC_STEPS as f64 };
                    out.push(c + DVec2::from_angle(a0 + sweep * t) * r);
                }
            }
        }
    }

    fn segment(&self, h: Half) -> Option<Segment2> {
        let e = self.edges.get(h.0)?;
        let (s, t) = (self.vert(self.start(h)), self.vert(self.end(h)));
        Some(match *e {
            Edge::Line { .. } => Segment2::Line { a: s, b: t },
            Edge::Arc { a, b, c, r } => {
                let a0 = (self.vert(a) - c).to_angle();
                let mut sweep = (self.vert(b) - c).to_angle() - a0;
                while sweep <= 0.0 {
                    sweep += TAU;
                }
                let mid = c + DVec2::from_angle(a0 + sweep / 2.0) * r;
                Segment2::Arc { start: s, mid, end: t }
            }
        })
    }

    /// Faces of the planar graph: (half-edges, polygon, signed area).
    fn faces(&self) -> Vec<(Vec<Half>, Vec<DVec2>, f64)> {
        // Outgoing half-edges per vertex sorted by angle.
        let mut out: Vec<Vec<(f64, Half)>> = vec![Vec::new(); self.verts.len()];
        for (i, _) in self.edges.iter().enumerate() {
            for dir in [true, false] {
                let h = (i, dir);
                if let Some(list) = out.get_mut(self.start(h)) {
                    list.push((self.out_dir(h).to_angle(), h));
                }
            }
        }
        for list in &mut out {
            list.sort_by(|a, b| a.0.total_cmp(&b.0));
        }
        let mut used: HashMap<Half, bool> = HashMap::new();
        let mut faces = Vec::new();
        let limit = self.edges.len() * 2 + 2;
        for (i, _) in self.edges.iter().enumerate() {
            for dir in [true, false] {
                let first = (i, dir);
                if used.contains_key(&first) {
                    continue;
                }
                let mut loop_h = Vec::new();
                let mut poly = vec![self.vert(self.start(first))];
                let mut h = first;
                let mut closed = false;
                for _ in 0..limit {
                    used.insert(h, true);
                    loop_h.push(h);
                    self.sample(h, &mut poly);
                    // At the end vertex, the next edge is the one just clockwise of the twin.
                    let v = self.end(h);
                    let twin = (h.0, !h.1);
                    let Some(list) = out.get(v) else { break };
                    let Some(pos) = list.iter().position(|(_, x)| *x == twin) else { break };
                    let next = list.get((pos + list.len() - 1) % list.len()).map(|(_, x)| *x);
                    let Some(next) = next else { break };
                    if next == first {
                        closed = true;
                        break;
                    }
                    if used.contains_key(&next) {
                        break;
                    }
                    h = next;
                }
                if closed {
                    poly.pop(); // last point repeats the first
                    let a = signed_area(&poly);
                    faces.push((loop_h, poly, a));
                }
            }
        }
        faces
    }
}

fn signed_area(p: &[DVec2]) -> f64 {
    let n = p.len();
    (0..n).map(|i| p[i].perp_dot(p[(i + 1) % n])).sum::<f64>() / 2.0
}

fn inside(p: DVec2, poly: &[DVec2]) -> bool {
    let mut c = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + n - 1) % n]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            c = !c;
        }
    }
    c
}

fn circle_poly(c: DVec2, r: f64) -> Vec<DVec2> {
    (0..ARC_STEPS * 2).map(|k| c + DVec2::from_angle(TAU * k as f64 / (ARC_STEPS * 2) as f64) * r).collect()
}

/// A point inside polygon `p` (and outside `holes`): try centroid-ish candidates on a
/// horizontal scanline through the middle.
fn interior_point(p: &[DVec2], holes: &[Vec<DVec2>]) -> DVec2 {
    let (mut lo, mut hi) = (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY));
    for q in p {
        lo = lo.min(*q);
        hi = hi.max(*q);
    }
    for frac in [0.5, 0.37, 0.63, 0.21, 0.79, 0.1, 0.9] {
        let y = lo.y + (hi.y - lo.y) * frac;
        // crossings of the scanline with all boundaries
        let mut xs = Vec::new();
        for poly in std::iter::once(p).chain(holes.iter().map(Vec::as_slice)) {
            let n = poly.len();
            for i in 0..n {
                let (a, b) = (poly[i], poly[(i + 1) % n]);
                if (a.y > y) != (b.y > y) {
                    xs.push(a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y));
                }
            }
        }
        xs.sort_by(f64::total_cmp);
        // widest inside interval
        let best = xs.as_chunks::<2>().0.iter().max_by(|a, b| (a[1] - a[0]).total_cmp(&(b[1] - b[0])));
        if let Some(seg) = best {
            let c = DVec2::new((seg[0] + seg[1]) / 2.0, y);
            if inside(c, p) && !holes.iter().any(|h| inside(c, h)) {
                return c;
            }
        }
    }
    (lo + hi) / 2.0
}

struct Face {
    loop2: Loop2,
    poly: Vec<DVec2>,
    area: f64,
    comp: usize,
}

/// All closed regions of a sketch.
pub fn regions(s: &Sketch) -> Vec<Region> {
    let (mut g, circles) = Graph::from(s);
    g.prune();

    // Connected components of the (pruned) graph.
    let mut comp_of = vec![usize::MAX; g.verts.len()];
    let mut ncomp = 0;
    for start in 0..g.verts.len() {
        if comp_of[start] != usize::MAX || !g.edges.iter().any(|e| e.ends().0 == start || e.ends().1 == start) {
            continue;
        }
        let mut stack = vec![start];
        comp_of[start] = ncomp;
        while let Some(v) = stack.pop() {
            for e in &g.edges {
                let (a, b) = e.ends();
                for (x, y) in [(a, b), (b, a)] {
                    if x == v && comp_of.get(y) == Some(&usize::MAX) {
                        comp_of[y] = ncomp;
                        stack.push(y);
                    }
                }
            }
        }
        ncomp += 1;
    }

    let mut faces: Vec<Face> = Vec::new();
    // Outer outline of each component (its unbounded face, reversed).
    let mut outlines: Vec<(Loop2, Vec<DVec2>, f64)> = Vec::new();
    let mut comp_outline: HashMap<usize, usize> = HashMap::new();
    for (halves, poly, area) in g.faces() {
        let comp = halves.first().map(|h| g.start(*h)).and_then(|v| comp_of.get(v).copied()).unwrap_or(0);
        let segs: Vec<Segment2> = halves.iter().filter_map(|h| g.segment(*h)).collect();
        if area > 1e-9 {
            faces.push(Face { loop2: Loop2::new(segs), poly, area, comp });
        } else if area < -1e-9 {
            // keep the biggest (most negative) per component as its outline
            let replace = comp_outline.get(&comp).and_then(|i| outlines.get(*i)).is_none_or(|o| o.2 < -area);
            if replace {
                let mut rev: Vec<Segment2> = segs
                    .iter()
                    .rev()
                    .map(|s| match *s {
                        Segment2::Line { a, b } => Segment2::Line { a: b, b: a },
                        Segment2::Arc { start, mid, end } => Segment2::Arc { start: end, mid, end: start },
                        c => c,
                    })
                    .collect();
                rev.dedup();
                let mut p = poly.clone();
                p.reverse();
                match comp_outline.get(&comp) {
                    Some(i) => {
                        if let Some(slot) = outlines.get_mut(*i) {
                            *slot = (Loop2::new(rev), p, -area);
                        }
                    }
                    None => {
                        comp_outline.insert(comp, outlines.len());
                        outlines.push((Loop2::new(rev), p, -area));
                    }
                }
            }
        }
    }
    // Circles: a face and an outline at once (their own component).
    for (k, (c, r)) in circles.iter().enumerate() {
        let comp = ncomp + k;
        let poly = circle_poly(*c, *r);
        let area = std::f64::consts::PI * r * r;
        faces.push(Face { loop2: Loop2::circle(*c, *r), poly: poly.clone(), area, comp });
        comp_outline.insert(comp, outlines.len());
        outlines.push((Loop2::circle(*c, *r), poly, area));
    }

    // Holes: each component outline goes into the smallest face of another component that
    // contains it.
    let mut holes: Vec<Vec<usize>> = vec![Vec::new(); faces.len()];
    let mut parent_of_comp: HashMap<usize, usize> = HashMap::new();
    for (&comp, &oi) in &comp_outline {
        let Some((_, opoly, _)) = outlines.get(oi) else { continue };
        let Some(probe) = opoly.first().copied() else { continue };
        let host = faces
            .iter()
            .enumerate()
            .filter(|(_, f)| f.comp != comp && inside(probe, &f.poly) && opoly.iter().all(|p| inside(*p, &f.poly)))
            .min_by(|a, b| a.1.area.total_cmp(&b.1.area))
            .map(|(i, _)| i);
        if let Some(h) = host {
            if let Some(list) = holes.get_mut(h) {
                list.push(oi);
            }
            parent_of_comp.insert(comp, h);
        }
    }
    // Nesting depth of a face = number of ancestors through containing faces.
    let depth = |mut comp: usize| {
        let mut d = 0;
        while let Some(host) = parent_of_comp.get(&comp) {
            d += 1;
            comp = faces.get(*host).map(|f| f.comp).unwrap_or(usize::MAX);
            if d > 1000 {
                break;
            }
        }
        d
    };
    let mut out = Vec::new();
    for (i, f) in faces.iter().enumerate() {
        let hole_ids = holes.get(i).cloned().unwrap_or_default();
        let hole_polys: Vec<Vec<DVec2>> =
            hole_ids.iter().filter_map(|o| outlines.get(*o).map(|x| x.1.clone())).collect();
        let hole_area: f64 = hole_ids.iter().filter_map(|o| outlines.get(*o).map(|x| x.2)).sum();
        out.push(Region {
            outer: f.loop2.clone(),
            holes: hole_ids.iter().filter_map(|o| outlines.get(*o).map(|x| x.0.clone())).collect(),
            area: f.area - hole_area,
            depth: depth(f.comp),
            sample: interior_point(&f.poly, &hole_polys),
        });
    }
    // Deterministic order: by depth, then larger first, then position.
    out.sort_by(|a, b| {
        a.depth
            .cmp(&b.depth)
            .then(b.area.total_cmp(&a.area))
            .then(a.sample.x.total_cmp(&b.sample.x))
            .then(a.sample.y.total_cmp(&b.sample.y))
    });
    out
}

/// Regions a feature uses by default: even nesting depth (outer shapes with their holes; an
/// island inside a hole is solid again).
pub fn default_regions(all: &[Region]) -> Vec<usize> {
    all.iter().enumerate().filter(|(_, r)| r.depth % 2 == 0).map(|(i, _)| i).collect()
}

/// The region containing `p` (smallest by area), for selector-based profile references.
pub fn region_at(all: &[Region], p: DVec2) -> Option<usize> {
    all.iter()
        .enumerate()
        .filter(|(_, r)| {
            let outer = loop_poly(&r.outer);
            inside(p, &outer) && !r.holes.iter().any(|h| inside(p, &loop_poly(h)))
        })
        .min_by(|a, b| a.1.area.total_cmp(&b.1.area))
        .map(|(i, _)| i)
}

/// Polygon approximation of a loop.
pub fn loop_poly(l: &Loop2) -> Vec<DVec2> {
    let mut out = Vec::new();
    for s in &l.segments {
        match *s {
            Segment2::Line { a, .. } => out.push(a),
            Segment2::Circle { center, radius } => out.extend(circle_poly(center, radius)),
            Segment2::Arc { start, mid, end } => {
                // circle through 3 points
                let (b, c) = (mid - start, end - start);
                let d = 2.0 * b.perp_dot(c);
                if d.abs() < 1e-12 {
                    out.push(start);
                    continue;
                }
                let ux = (c.y * b.length_squared() - b.y * c.length_squared()) / d;
                let uy = (b.x * c.length_squared() - c.x * b.length_squared()) / d;
                let center = start + DVec2::new(ux, uy);
                let r = (start - center).length();
                let a0 = (start - center).to_angle();
                let am = (mid - center).to_angle();
                let a1 = (end - center).to_angle();
                // sweep direction that passes through mid
                let norm = |a: f64| a.rem_euclid(TAU);
                let ccw = norm(am - a0) < norm(a1 - a0);
                let sweep = if ccw { norm(a1 - a0) } else { -norm(a0 - a1) };
                for k in 0..ARC_STEPS {
                    out.push(center + DVec2::from_angle(a0 + sweep * k as f64 / ARC_STEPS as f64) * r);
                }
            }
        }
    }
    out
}
