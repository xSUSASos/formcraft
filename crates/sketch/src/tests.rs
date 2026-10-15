use std::f64::consts::PI;

use formcraft_kernel::DVec2;

use super::*;

fn pt(s: &Sketch, id: Eid) -> (f64, f64) {
    s.point(id).unwrap()
}

/// Points of a line.
fn ends(s: &Sketch, l: Eid) -> (Eid, Eid) {
    match s.get(l) {
        Some(Entity::Line { a, b, .. }) => (*a, *b),
        _ => panic!("not a line"),
    }
}

#[test]
fn rectangle_dimensions_drive_geometry() {
    let mut s = Sketch::new();
    let [bottom, right, ..] = s.add_rectangle(0.0, 0.0, 9.0, 3.0).unwrap();
    let (p0, _) = ends(&s, bottom);
    s.add_constraint(Constraint::Fix { point: p0 }).unwrap();
    s.add_dimension(DimKind::Length { line: bottom }, "60").unwrap();
    s.add_dimension(DimKind::Length { line: right }, "40").unwrap();
    let r = solve(&mut s, &[60.0, 40.0]);
    assert!(r.converged, "{r:?}");
    assert_eq!(r.dof, 0, "fully constrained: {r:?}");
    let (_, p2) = ends(&s, right);
    let (x, y) = pt(&s, p2);
    assert!((x - 60.0).abs() < 1e-7 && (y - 40.0).abs() < 1e-7, "({x}, {y})");
    let (x0, y0) = pt(&s, p0);
    assert!(x0.abs() < 1e-7 && y0.abs() < 1e-7);
}

#[test]
fn degrees_of_freedom_are_counted() {
    let mut s = Sketch::new();
    s.add_rectangle(0.0, 0.0, 10.0, 5.0).unwrap();
    let r = solve(&mut s, &[]);
    // 4 points = 8 unknowns; 4 H/V constraints → 4 DOF (x, y, width, height)
    assert!(r.converged);
    assert_eq!(r.dof, 4, "{r:?}");
    assert_eq!(r.redundant, 0);
}

#[test]
fn under_constrained_sketch_moves_little() {
    let mut s = Sketch::new();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(10.0, 0.5).unwrap();
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(Constraint::Horizontal { line: l }).unwrap();
    let r = solve(&mut s, &[]);
    assert!(r.converged);
    let (pa, pb) = (pt(&s, a), pt(&s, b));
    assert!((pa.1 - pb.1).abs() < 1e-9);
    // minimum-norm step: both ends moved by ~0.25 towards each other, x unchanged
    assert!((pa.1 - 0.25).abs() < 1e-6 && (pb.0 - 10.0).abs() < 1e-9, "{pa:?} {pb:?}");
}

#[test]
fn angle_parallel_perpendicular_equal() {
    let mut s = Sketch::new();
    let o = s.add_point(0.0, 0.0).unwrap();
    let a = s.add_point(10.0, 1.0).unwrap();
    let b = s.add_point(1.0, 8.0).unwrap();
    let l1 = s.add_line(o, a).unwrap();
    let l2 = s.add_line(o, b).unwrap();
    s.add_constraint(Constraint::Fix { point: o }).unwrap();
    s.add_constraint(Constraint::Horizontal { line: l1 }).unwrap();
    s.add_constraint(Constraint::Equal { a: l1, b: l2 }).unwrap();
    s.add_dimension(DimKind::Angle { a: l1, b: l2 }, "60").unwrap();
    s.add_dimension(DimKind::Length { line: l1 }, "20").unwrap();
    let r = solve(&mut s, &[60.0, 20.0]);
    assert!(r.converged && r.dof == 0, "{r:?}");
    let (bx, by) = pt(&s, b);
    assert!((bx - 10.0).abs() < 1e-6 && (by - 20.0 * (PI / 3.0).sin()).abs() < 1e-6, "({bx}, {by})");

    // perpendicular
    let c = s.add_point(25.0, 3.0).unwrap();
    let l3 = s.add_line(a, c).unwrap();
    s.add_constraint(Constraint::Perpendicular { a: l1, b: l3 }).unwrap();
    let r = solve(&mut s, &[60.0, 20.0]);
    assert!(r.converged);
    let (cx, _) = pt(&s, c);
    assert!((cx - 20.0).abs() < 1e-6, "perpendicular to a horizontal line is vertical: {cx}");
}

#[test]
fn tangent_circle_and_radius() {
    let mut s = Sketch::new();
    let [bottom, ..] = s.add_rectangle(0.0, 0.0, 50.0, 30.0).unwrap();
    let c = s.add_point(20.0, 12.0).unwrap();
    let circ = s.add_circle(c, 5.0).unwrap();
    s.add_constraint(Constraint::Tangent { a: bottom, b: circ }).unwrap();
    s.add_dimension(DimKind::Diameter { curve: circ }, "8").unwrap();
    let r = solve(&mut s, &[8.0]);
    assert!(r.converged, "{r:?}");
    let (bx, by) = ends(&s, bottom);
    let line_y = pt(&s, bx).1.max(pt(&s, by).1);
    let (_, cy) = pt(&s, c);
    assert!(((cy - line_y).abs() - 4.0).abs() < 1e-6, "centre {cy}, line {line_y}");
}

#[test]
fn conflicting_constraints_leave_the_sketch_unchanged() {
    let mut s = Sketch::new();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    let l = s.add_line(a, b).unwrap();
    s.add_dimension(DimKind::Length { line: l }, "10").unwrap();
    s.add_dimension(DimKind::Distance { a, b }, "20").unwrap();
    let before = s.clone();
    let r = solve(&mut s, &[10.0, 20.0]);
    assert!(!r.converged);
    assert_eq!(s, before);
}

#[test]
fn hostile_values_do_not_explode() {
    let mut s = Sketch::new();
    let [bottom, ..] = s.add_rectangle(0.0, 0.0, 1.0, 1.0).unwrap();
    s.add_dimension(DimKind::Length { line: bottom }, "x").unwrap();
    let r = solve(&mut s, &[f64::NAN]);
    assert!(!r.converged);
    let r = solve(&mut s, &[1e300]);
    assert!(s.entities.iter().all(|(_, e)| match e {
        Entity::Point { x, y } => x.is_finite() && y.is_finite(),
        _ => true,
    }));
    let _ = r;
    assert!(s.add_point(f64::NAN, 0.0).is_err());
    assert!(s.add_circle(Eid(9999), 1.0).is_err());
}

#[test]
fn plate_with_hole_gives_ring_and_disk() {
    let mut s = Sketch::new();
    s.add_rectangle(0.0, 0.0, 60.0, 40.0).unwrap();
    let c = s.add_point(30.0, 20.0).unwrap();
    s.add_circle(c, 5.0).unwrap();
    let rs = regions(&s);
    assert_eq!(rs.len(), 2, "{rs:?}");
    let ring = &rs[0];
    assert_eq!(ring.depth, 0);
    assert_eq!(ring.holes.len(), 1);
    assert!((ring.area - (2400.0 - PI * 25.0)).abs() < 1.0, "{}", ring.area);
    assert_eq!(rs[1].depth, 1);
    assert_eq!(default_regions(&rs), vec![0]);
    // the sample point of the ring is not inside the hole
    assert!(rs[0].sample.distance(DVec2::new(30.0, 20.0)) > 5.0);
    assert_eq!(region_at(&rs, DVec2::new(30.0, 20.0)), Some(1));
    assert_eq!(region_at(&rs, DVec2::new(5.0, 5.0)), Some(0));
}

#[test]
fn slot_with_arcs_is_one_region() {
    let mut s = Sketch::new();
    let p = [
        s.add_point(0.0, 0.0).unwrap(),
        s.add_point(20.0, 0.0).unwrap(),
        s.add_point(20.0, 10.0).unwrap(),
        s.add_point(0.0, 10.0).unwrap(),
    ];
    let c1 = s.add_point(20.0, 5.0).unwrap();
    let c2 = s.add_point(0.0, 5.0).unwrap();
    s.add_line(p[0], p[1]).unwrap();
    s.add_arc(c1, p[1], p[2]).unwrap();
    s.add_line(p[2], p[3]).unwrap();
    s.add_arc(c2, p[3], p[0]).unwrap();
    let rs = regions(&s);
    assert_eq!(rs.len(), 1, "{rs:?}");
    assert!((rs[0].area - (200.0 + PI * 25.0)).abs() < 1.0, "{}", rs[0].area);
}

#[test]
fn shared_edge_splits_regions_and_dangling_lines_are_ignored() {
    let mut s = Sketch::new();
    // two squares sharing an edge (a "window" with a divider), plus a dangling line
    let p: Vec<Eid> = [(0.0, 0.0), (10.0, 0.0), (20.0, 0.0), (20.0, 10.0), (10.0, 10.0), (0.0, 10.0)]
        .iter()
        .map(|(x, y)| s.add_point(*x, *y).unwrap())
        .collect();
    for (a, b) in [(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 0), (1, 4)] {
        s.add_line(p[a], p[b]).unwrap();
    }
    let q = s.add_point(30.0, 30.0).unwrap();
    s.add_line(p[3], q).unwrap();
    let rs = regions(&s);
    assert_eq!(rs.len(), 2, "{rs:?}");
    assert!(rs.iter().all(|r| (r.area - 100.0).abs() < 1e-6));
}

#[test]
fn island_inside_hole_is_solid_by_default() {
    let mut s = Sketch::new();
    s.add_rectangle(0.0, 0.0, 100.0, 100.0).unwrap();
    s.add_rectangle(20.0, 20.0, 80.0, 80.0).unwrap();
    let c = s.add_point(50.0, 50.0).unwrap();
    s.add_circle(c, 10.0).unwrap();
    let rs = regions(&s);
    assert_eq!(rs.len(), 3);
    let depths: Vec<usize> = rs.iter().map(|r| r.depth).collect();
    assert_eq!(depths, vec![0, 1, 2]);
    assert_eq!(default_regions(&rs), vec![0, 2]);
}

#[test]
fn construction_geometry_is_not_a_profile() {
    let mut s = Sketch::new();
    let l = s.add_rectangle(0.0, 0.0, 10.0, 10.0).unwrap();
    for id in l {
        if let Some(Entity::Line { construction, .. }) = s.get_mut(id) {
            *construction = true;
        }
    }
    assert!(regions(&s).is_empty());
}

#[test]
fn delete_removes_dependents() {
    let mut s = Sketch::new();
    let [bottom, ..] = s.add_rectangle(0.0, 0.0, 10.0, 10.0).unwrap();
    s.add_dimension(DimKind::Length { line: bottom }, "10").unwrap();
    let (a, _) = ends(&s, bottom);
    s.delete(a);
    assert!(s.get(bottom).is_none());
    assert!(s.dimensions.is_empty());
    assert_eq!(s.entities.iter().filter(|(_, e)| matches!(e, Entity::Line { .. })).count(), 2);
}
