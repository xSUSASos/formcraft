//! Golden geometry tests (the Phase 0 kernel test set) and hostile-input tests.

use std::f64::consts::{PI, TAU};

use formcraft_kernel::{DVec2, Loop2, Plane, Segment2};

use super::*;

const K: Occt = Occt;

fn close(a: f64, b: f64, rel: f64) -> bool {
    (a - b).abs() <= rel * b.abs().max(1.0)
}

fn assert_volume(s: &Shape, expected: f64) {
    let m = K.mass_props(s).unwrap();
    assert!(close(m.volume, expected, 1e-6), "volume {} != {expected}", m.volume);
    assert!(K.is_valid(s));
}

fn plate_with_hole() -> Shape {
    let p = Profile::new(Plane::XY, Loop2::rect(DVec2::ZERO, DVec2::new(60.0, 40.0)))
        .with_hole(Loop2::circle(DVec2::new(30.0, 20.0), 5.0));
    K.extrude(&p, DVec3::Z * 10.0).unwrap()
}

#[test]
fn box_with_hole_by_extrude() {
    let s = plate_with_hole();
    assert_volume(&s, 60.0 * 40.0 * 10.0 - PI * 25.0 * 10.0);
    let m = K.mass_props(&s).unwrap();
    assert!(m.bbox.min.distance(DVec3::ZERO) < 1e-6);
    assert!(m.bbox.max.distance(DVec3::new(60.0, 40.0, 10.0)) < 1e-6);
    assert_eq!(K.faces(&s).unwrap().len(), 7);
}

#[test]
fn fillet_all_edges() {
    let b = K.make_box(DVec3::ZERO, DVec3::splat(20.0)).unwrap();
    let edges: Vec<u32> = K.edges(&b).unwrap().iter().map(|e| e.index).collect();
    assert_eq!(edges.len(), 12);
    let s = K.fillet(&b, &edges, 2.0).unwrap();
    let (r, a) = (2.0, 16.0);
    let expected = a * a * a + 2.0 * r * 3.0 * a * a + PI * r * r * 3.0 * a + 4.0 / 3.0 * PI * r * r * r;
    assert_volume(&s, expected);
    assert_eq!(K.faces(&s).unwrap().len(), 26);
}

#[test]
fn cut_by_cylinder() {
    let b = K.make_box(DVec3::ZERO, DVec3::new(40.0, 40.0, 20.0)).unwrap();
    let c = K.make_cylinder(DVec3::new(20.0, 20.0, -1.0), DVec3::Z, 10.0, 22.0).unwrap();
    let s = K.boolean(BoolOp::Cut, &b, &c).unwrap();
    assert_volume(&s, 40.0 * 40.0 * 20.0 - PI * 100.0 * 20.0);
    let cyl = K.faces(&s).unwrap().into_iter().filter(|f| matches!(f.kind, SurfaceKind::Cylinder { .. })).count();
    assert!(cyl >= 1);
}

#[test]
fn revolve_ring() {
    let p = Profile::new(Plane::XZ, Loop2::rect(DVec2::new(10.0, 0.0), DVec2::new(20.0, 30.0)));
    let s = K.revolve(&p, DVec3::ZERO, DVec3::Z, TAU).unwrap();
    assert_volume(&s, PI * (400.0 - 100.0) * 30.0);
}

#[test]
fn shell_open_top() {
    let b = K.make_box(DVec3::ZERO, DVec3::new(40.0, 30.0, 20.0)).unwrap();
    let top: Vec<u32> = K.faces(&b).unwrap().iter().filter(|f| f.normal.z > 0.9).map(|f| f.index).collect();
    assert_eq!(top.len(), 1);
    let s = K.shell(&b, &top, 2.0).unwrap();
    assert_volume(&s, 40.0 * 30.0 * 20.0 - 36.0 * 26.0 * 18.0);
}

#[test]
fn union_and_intersect() {
    let a = K.make_box(DVec3::ZERO, DVec3::splat(10.0)).unwrap();
    let b = K.make_box(DVec3::splat(5.0), DVec3::splat(10.0)).unwrap();
    assert_volume(&K.boolean(BoolOp::Union, &a, &b).unwrap(), 2000.0 - 125.0);
    assert_volume(&K.boolean(BoolOp::Intersect, &a, &b).unwrap(), 125.0);
}

#[test]
fn chamfer_one_edge() {
    let b = K.make_box(DVec3::ZERO, DVec3::splat(10.0)).unwrap();
    let s = K.chamfer(&b, &[0], 1.0).unwrap();
    assert_volume(&s, 1000.0 - 0.5 * 10.0);
}

#[test]
fn arc_profile_slot() {
    // a slot: two lines and two half-circle arcs, width 10, straight length 20
    let seg = vec![
        Segment2::Line { a: DVec2::new(0.0, 0.0), b: DVec2::new(20.0, 0.0) },
        Segment2::Arc { start: DVec2::new(20.0, 0.0), mid: DVec2::new(25.0, 5.0), end: DVec2::new(20.0, 10.0) },
        Segment2::Line { a: DVec2::new(20.0, 10.0), b: DVec2::new(0.0, 10.0) },
        Segment2::Arc { start: DVec2::new(0.0, 10.0), mid: DVec2::new(-5.0, 5.0), end: DVec2::new(0.0, 0.0) },
    ];
    let s = K.extrude(&Profile::new(Plane::XY, Loop2::new(seg)), DVec3::Z * 2.0).unwrap();
    assert_volume(&s, (20.0 * 10.0 + PI * 25.0) * 2.0);
}

#[test]
fn face_normals_point_outward() {
    let b = K.make_box(DVec3::ZERO, DVec3::splat(10.0)).unwrap();
    for f in K.faces(&b).unwrap() {
        let out = f.center - DVec3::splat(5.0);
        assert!(f.normal.dot(out) > 0.0, "face {} normal {:?} at {:?}", f.index, f.normal, f.center);
    }
}

#[test]
fn edges_know_their_faces() {
    let b = K.make_box(DVec3::ZERO, DVec3::splat(10.0)).unwrap();
    for e in K.edges(&b).unwrap() {
        assert_eq!(e.faces.len(), 2);
        assert!((e.length - 10.0).abs() < 1e-9);
        assert!(matches!(e.kind, CurveKind::Line { .. }));
    }
}

#[test]
fn tessellation_matches_brep() {
    let s = plate_with_hole();
    let m = K.tessellate(&s, 0.01).unwrap();
    let exact = K.mass_props(&s).unwrap().volume;
    assert!(close(m.volume(), exact, 1e-3), "mesh volume {} vs {exact}", m.volume());
    assert_eq!(m.triangle_face.len(), m.triangle_count());
    assert_eq!(m.normals.len(), m.positions.len());
    assert_eq!(m.edges.len(), K.edges(&s).unwrap().len());
    assert!(m.edges.iter().all(|e| e.points.len() >= 2));
}

#[test]
fn brep_bytes_round_trip() {
    let s = plate_with_hole();
    let back = K.deserialize(&K.serialize(&s).unwrap()).unwrap();
    assert_volume(&back, K.mass_props(&s).unwrap().volume);
}

#[test]
fn compound_and_solids() {
    let a = K.make_box(DVec3::ZERO, DVec3::splat(1.0)).unwrap();
    let b = K.make_box(DVec3::splat(5.0), DVec3::splat(1.0)).unwrap();
    let c = K.compound(&[a, b]).unwrap();
    assert_eq!(K.solids(&c).unwrap().len(), 2);
    assert_volume(&c, 2.0);
}

#[test]
fn transform_moves_shape() {
    let a = K.make_box(DVec3::ZERO, DVec3::splat(1.0)).unwrap();
    let t = K.transform(&a, &DMat4::from_translation(DVec3::new(10.0, 0.0, 0.0))).unwrap();
    let bb = K.mass_props(&t).unwrap().bbox;
    assert!(bb.min.distance(DVec3::new(10.0, 0.0, 0.0)) < 1e-9);
    let r = K.transform(&a, &DMat4::from_rotation_z(PI / 2.0)).unwrap();
    let bb = K.mass_props(&r).unwrap().bbox;
    assert!(bb.min.distance(DVec3::new(-1.0, 0.0, 0.0)) < 1e-9, "{bb:?}");
}

/// Inputs that made OCCT throw (and opencascade-rs abort) must come back as errors.
#[test]
fn hostile_inputs_are_errors_not_aborts() {
    let b = K.make_box(DVec3::ZERO, DVec3::splat(10.0)).unwrap();
    assert!(K.make_box(DVec3::ZERO, DVec3::new(0.0, 1.0, 1.0)).is_err());
    assert!(K.make_box(DVec3::splat(f64::NAN), DVec3::ONE).is_err());
    assert!(K.make_cylinder(DVec3::ZERO, DVec3::ZERO, 1.0, 1.0).is_err());
    assert!(K.fillet(&b, &[0], 50.0).is_err(), "radius larger than the box");
    assert!(K.fillet(&b, &[999], 1.0).is_err(), "edge index out of range");
    assert!(K.fillet(&b, &[], 1.0).is_err());
    assert!(K.shell(&b, &[999], 1.0).is_err());
    let thick = K.shell(&b, &[0], 20.0);
    if let Ok(s) = &thick {
        eprintln!("thick shell result: {:?}", K.mass_props(s));
    }
    assert!(thick.is_err(), "wall thicker than the box");
    let dup = Loop2::polygon(&[DVec2::ZERO, DVec2::X, DVec2::ONE, DVec2::ZERO]);
    assert!(K.extrude(&Profile::new(Plane::XY, dup), DVec3::Z).is_err());
    let sq = Profile::new(Plane::XY, Loop2::rect(DVec2::ZERO, DVec2::ONE));
    assert!(K.extrude(&sq, DVec3::ZERO).is_err());
    assert!(K.extrude(&sq, DVec3::X).is_err() || K.extrude(&sq, DVec3::X).is_ok(), "in-plane extrude never aborts");
    assert!(K.revolve(&sq, DVec3::ZERO, DVec3::Z, 0.0).is_err());
    // STEP read errors are covered in tests/step.rs (harness = false, see there).
    assert!(K.deserialize(b"garbage").is_err());
    assert!(K.tessellate(&b, 0.0).is_err());
}

#[test]
fn parallel_use_is_serialised() {
    let handles: Vec<_> = (0..4)
        .map(|i| {
            std::thread::spawn(move || {
                let b = K.make_box(DVec3::ZERO, DVec3::splat(10.0 + i as f64)).unwrap();
                let m = K.tessellate(&b, 0.1).unwrap();
                (m.triangle_count(), K.mass_props(&b).unwrap().volume)
            })
        })
        .collect();
    for (i, h) in handles.into_iter().enumerate() {
        let (tris, vol) = h.join().unwrap();
        assert_eq!(tris, 12);
        assert!(close(vol, (10.0 + i as f64).powi(3), 1e-9));
    }
}
