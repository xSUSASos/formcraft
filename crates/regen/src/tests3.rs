//! Phase 3 golden tests: sketch-based and dress-up features with semantic selectors.

use std::f64::consts::PI;

use formcraft_doc::{EdgeSel, ExtrudeDirection, FaceSel, ProfileSel, RevolveAxis, SketchPlane, Vec3Expr};
use formcraft_occt::Occt;
use formcraft_sketch::{Constraint, DimKind, Entity, Sketch};

use super::*;

fn xy() -> SketchPlane {
    SketchPlane::Origin { name: "XY".into(), offset: "0".into() }
}

fn vol(m: &Model, b: BodyId) -> f64 {
    Occt.mass_props(&m.body(b).unwrap().shape).unwrap().volume
}

fn bbox(m: &Model, b: BodyId) -> formcraft_kernel::Aabb {
    Occt.mass_props(&m.body(b).unwrap().shape).unwrap().bbox
}

fn ok(m: &Model) {
    assert_eq!(m.error_count(), 0, "{:?}", m.status);
}

/// Plate sketch: rectangle w×h at the origin with dimensions driven by parameters, and a hole.
fn plate_sketch(with_hole: bool) -> Sketch {
    let mut s = Sketch::new();
    let [bottom, right, ..] = s.add_rectangle(0.0, 0.0, 10.0, 10.0).unwrap();
    let a = match s.get(bottom) {
        Some(Entity::Line { a, .. }) => *a,
        _ => panic!("rectangle has no bottom line"),
    };
    s.add_constraint(Constraint::Fix { point: a }).unwrap();
    s.add_dimension(DimKind::Length { line: bottom }, "L").unwrap();
    s.add_dimension(DimKind::Length { line: right }, "W").unwrap();
    if with_hole {
        let c = s.add_point(30.0, 20.0).unwrap();
        let circ = s.add_circle(c, 3.0).unwrap();
        s.add_dimension(DimKind::Diameter { curve: circ }, "hole_d").unwrap();
    }
    s
}

fn params(d: &mut Document) {
    d.set_parameter("L", "60".into(), None).unwrap();
    d.set_parameter("W", "40".into(), None).unwrap();
    d.set_parameter("T", "5".into(), None).unwrap();
    d.set_parameter("hole_d", "8".into(), None).unwrap();
}

fn extrude(d: &mut Document, sketch: FeatureId, dist: &str, op: BodyOp) -> FeatureId {
    d.add_feature(FeatureKind::Extrude {
        sketch,
        profiles: ProfileSel::All,
        distance: dist.into(),
        direction: ExtrudeDirection::OneSide,
        through_all: false,
        op,
    })
    .unwrap()
}

#[test]
fn sketch_extrude_plate_with_hole_follows_parameters() {
    let mut d = Document::default();
    params(&mut d);
    let sk = d.add_feature(FeatureKind::Sketch { plane: xy(), sketch: plate_sketch(true) }).unwrap();
    let ex = extrude(&mut d, sk, "T", BodyOp::NewBody);
    let m = regen(&d, &Occt);
    ok(&m);
    let b = BodyId(ex.0);
    assert!((vol(&m, b) - (60.0 * 40.0 * 5.0 - PI * 16.0 * 5.0)).abs() < 1e-6, "{}", vol(&m, b));
    assert_eq!(m.sketch(sk).unwrap().dof, 2, "the hole centre is free");
    d.set_parameter("L", "80".into(), None).unwrap();
    d.set_parameter("hole_d", "10".into(), None).unwrap();
    let m = regen(&d, &Occt);
    ok(&m);
    assert!((vol(&m, b) - (80.0 * 40.0 * 5.0 - PI * 25.0 * 5.0)).abs() < 1e-6);
}

#[test]
fn symmetric_and_through_all() {
    let mut d = Document::default();
    params(&mut d);
    let sk = d.add_feature(FeatureKind::Sketch { plane: xy(), sketch: plate_sketch(false) }).unwrap();
    let ex = d
        .add_feature(FeatureKind::Extrude {
            sketch: sk,
            profiles: ProfileSel::All,
            distance: "10".into(),
            direction: ExtrudeDirection::Symmetric,
            through_all: false,
            op: BodyOp::NewBody,
        })
        .unwrap();
    let b = BodyId(ex.0);
    // A circle sketch on XY cut through all.
    let mut cs = Sketch::new();
    let c = cs.add_point(30.0, 20.0).unwrap();
    cs.add_circle(c, 5.0).unwrap();
    let cut_sk = d.add_feature(FeatureKind::Sketch { plane: xy(), sketch: cs }).unwrap();
    d.add_feature(FeatureKind::Extrude {
        sketch: cut_sk,
        profiles: ProfileSel::All,
        distance: "1".into(),
        direction: ExtrudeDirection::OneSide,
        through_all: true,
        op: BodyOp::Cut(b),
    })
    .unwrap();
    let m = regen(&d, &Occt);
    ok(&m);
    let bb = bbox(&m, b);
    assert!((bb.min.z + 5.0).abs() < 1e-9 && (bb.max.z - 5.0).abs() < 1e-9, "{bb:?}");
    assert!((vol(&m, b) - (60.0 * 40.0 * 10.0 - PI * 25.0 * 10.0)).abs() < 1e-6);
}

#[test]
fn revolve_about_a_sketch_line() {
    let mut d = Document::default();
    // Profile on XZ: rectangle x 10..20, z 0..30, revolved about the sketch's y axis (world Z).
    let mut s = Sketch::new();
    s.add_rectangle(10.0, 0.0, 20.0, 30.0).unwrap();
    let a0 = s.add_point(0.0, 0.0).unwrap();
    let a1 = s.add_point(0.0, 10.0).unwrap();
    let axis = s.add_line(a0, a1).unwrap();
    if let Some(Entity::Line { construction, .. }) = s.get_mut(axis) {
        *construction = true;
    }
    let sk = d
        .add_feature(FeatureKind::Sketch {
            plane: SketchPlane::Origin { name: "XZ".into(), offset: "0".into() },
            sketch: s,
        })
        .unwrap();
    let rv = d
        .add_feature(FeatureKind::Revolve {
            sketch: sk,
            profiles: ProfileSel::All,
            axis: RevolveAxis::Line { line: axis },
            angle: "360".into(),
            op: BodyOp::NewBody,
        })
        .unwrap();
    let m = regen(&d, &Occt);
    ok(&m);
    assert!((vol(&m, BodyId(rv.0)) - PI * 300.0 * 30.0).abs() < 1e-6);
    // Half turn about world Z by name.
    if let Some(f) = d.feature_mut(rv)
        && let FeatureKind::Revolve { axis, angle, .. } = &mut f.kind
    {
        *axis = RevolveAxis::Name("Z".into());
        *angle = "180".into();
    }
    let m = regen(&d, &Occt);
    ok(&m);
    assert!((vol(&m, BodyId(rv.0)) - PI * 300.0 * 15.0).abs() < 1e-6);
}

fn plate(d: &mut Document) -> BodyId {
    params(d);
    let sk = d.add_feature(FeatureKind::Sketch { plane: xy(), sketch: plate_sketch(false) }).unwrap();
    BodyId(extrude(d, sk, "T", BodyOp::NewBody).0)
}

#[test]
fn fillet_vertical_edges_survives_parameter_change() {
    let mut d = Document::default();
    let b = plate(&mut d);
    d.add_feature(FeatureKind::Fillet { body: b, edges: EdgeSel::name("vertical"), radius: "3".into() }).unwrap();
    let expect = |l: f64, w: f64, t: f64| (l * w - (4.0 - PI) * 9.0) * t;
    let m = regen(&d, &Occt);
    ok(&m);
    assert!((vol(&m, b) - expect(60.0, 40.0, 5.0)).abs() < 1e-6, "{}", vol(&m, b));
    d.set_parameter("T", "12".into(), None).unwrap();
    d.set_parameter("L", "70".into(), None).unwrap();
    let m = regen(&d, &Occt);
    ok(&m);
    assert!((vol(&m, b) - expect(70.0, 40.0, 12.0)).abs() < 1e-6);
}

#[test]
fn chamfer_top_outer_edges() {
    let mut d = Document::default();
    let b = plate(&mut d);
    d.add_feature(FeatureKind::Chamfer { body: b, edges: EdgeSel::name("top_outer"), distance: "1".into() }).unwrap();
    let m = regen(&d, &Occt);
    ok(&m);
    // Four prisms of cross-section 0.5 mm² along the perimeter, corners overlap.
    let removed = 60.0 * 40.0 * 5.0 - vol(&m, b);
    assert!(removed > 0.5 * 2.0 * (60.0 + 40.0) - 2.0 && removed < 0.5 * 2.0 * (60.0 + 40.0) + 1.0, "{removed}");
    assert_eq!(Occt.faces(&m.body(b).unwrap().shape).unwrap().len(), 10);
}

#[test]
fn shell_open_top() {
    let mut d = Document::default();
    let b = plate(&mut d);
    d.set_parameter("T", "20".into(), None).unwrap();
    d.add_feature(FeatureKind::Shell { body: b, faces: FaceSel::name("top"), thickness: "2".into() }).unwrap();
    let m = regen(&d, &Occt);
    ok(&m);
    assert!((vol(&m, b) - (60.0 * 40.0 * 20.0 - 56.0 * 36.0 * 18.0)).abs() < 1e-6);
}

#[test]
fn counterbored_holes_on_the_top_face() {
    let mut d = Document::default();
    let b = plate(&mut d);
    d.set_parameter("T", "10".into(), None).unwrap();
    d.add_feature(FeatureKind::Hole {
        body: b,
        face: FaceSel::name("top"),
        points: vec![Vec3Expr::num(10.0, 10.0, 0.0), Vec3Expr::num(50.0, 30.0, 99.0)],
        diameter: "5.5".into(),
        depth: None,
        counterbore: Some(("10".into(), "4".into())),
        countersink: None,
    })
    .unwrap();
    let m = regen(&d, &Occt);
    ok(&m);
    let one = PI * 2.75 * 2.75 * 6.0 + PI * 25.0 * 4.0;
    assert!((vol(&m, b) - (60.0 * 40.0 * 10.0 - 2.0 * one)).abs() < 1e-6, "{}", vol(&m, b));
}

#[test]
fn countersunk_blind_hole() {
    let mut d = Document::default();
    let b = plate(&mut d);
    d.set_parameter("T", "10".into(), None).unwrap();
    d.add_feature(FeatureKind::Hole {
        body: b,
        face: FaceSel::name("top"),
        points: vec![Vec3Expr::num(30.0, 20.0, 0.0)],
        diameter: "4".into(),
        depth: Some("6".into()),
        counterbore: None,
        countersink: Some(("8".into(), "90".into())),
    })
    .unwrap();
    let m = regen(&d, &Occt);
    ok(&m);
    // cylinder r=2 h=6 ∪ cone r 4→2 over h=2: frustum 2π/3·(16+8+4) = 56π/3, minus the overlap
    // of the cylinder inside the frustum (π·4·2 = 8π).
    let removed = PI * 4.0 * 6.0 + 56.0 * PI / 3.0 - 8.0 * PI;
    assert!((vol(&m, b) - (60.0 * 40.0 * 10.0 - removed)).abs() < 1e-3, "{}", 24000.0 - vol(&m, b));
}

#[test]
fn press_pull_top_face() {
    let mut d = Document::default();
    let b = plate(&mut d);
    d.add_feature(FeatureKind::PressPull { body: b, face: FaceSel::name("top"), distance: "3".into() }).unwrap();
    let m = regen(&d, &Occt);
    ok(&m);
    assert!((bbox(&m, b).max.z - 8.0).abs() < 1e-9);
    d.add_feature(FeatureKind::PressPull { body: b, face: FaceSel::name("top"), distance: "-6".into() }).unwrap();
    let m = regen(&d, &Occt);
    ok(&m);
    assert!((bbox(&m, b).max.z - 2.0).abs() < 1e-9);
}

#[test]
fn sketch_on_a_body_face_and_join() {
    let mut d = Document::default();
    let b = plate(&mut d);
    let mut s = Sketch::new();
    let c = s.add_point(0.0, 0.0).unwrap();
    s.add_circle(c, 5.0).unwrap();
    let sk = d
        .add_feature(FeatureKind::Sketch {
            plane: SketchPlane::Face { body: b, face: FaceSel::name("top"), offset: "0".into() },
            sketch: s,
        })
        .unwrap();
    extrude(&mut d, sk, "10", BodyOp::Join(b));
    let m = regen(&d, &Occt);
    ok(&m);
    // the boss sits at the centre of the top face and goes up
    let bb = bbox(&m, b);
    assert!((bb.max.z - 15.0).abs() < 1e-9, "{bb:?}");
    assert!((vol(&m, b) - (60.0 * 40.0 * 5.0 + PI * 25.0 * 10.0)).abs() < 1e-6);
}

#[test]
fn bad_selectors_are_feature_errors() {
    let mut d = Document::default();
    let b = plate(&mut d);
    d.add_feature(FeatureKind::Fillet { body: b, edges: EdgeSel::name("diagonal"), radius: "1".into() }).unwrap();
    d.add_feature(FeatureKind::Shell { body: b, faces: FaceSel::name("+Q"), thickness: "1".into() }).unwrap();
    d.add_feature(FeatureKind::Fillet { body: b, edges: EdgeSel::name("circular"), radius: "1".into() }).unwrap();
    d.add_feature(FeatureKind::PressPull {
        body: b,
        face: FaceSel::Near { near: [f64::NAN, 0.0, 0.0] },
        distance: "1".into(),
    })
    .unwrap();
    let m = regen(&d, &Occt);
    assert_eq!(m.error_count(), 4, "{:?}", m.status);
    assert!(m.body(b).is_some(), "the base body survives");
}

#[test]
fn face_signature_tracks_the_face_after_resize() {
    let mut d = Document::default();
    let b = plate(&mut d);
    let m = regen(&d, &Occt);
    let faces = Occt.faces(&m.body(b).unwrap().shape).unwrap();
    let side = faces.iter().find(|f| f.normal.x > 0.99).unwrap();
    let sig = FaceSel::Signature { face: select::face_signature(side) };
    d.add_feature(FeatureKind::PressPull { body: b, face: sig, distance: "5".into() }).unwrap();
    d.set_parameter("L", "65".into(), None).unwrap();
    let m = regen(&d, &Occt);
    ok(&m);
    assert!((bbox(&m, b).max.x - 70.0).abs() < 1e-9, "{:?}", bbox(&m, b));
}
