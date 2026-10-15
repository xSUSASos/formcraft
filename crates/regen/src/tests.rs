use std::f64::consts::PI;

use formcraft_doc::Vec3Expr;
use formcraft_occt::Occt;

use super::*;

fn vol(m: &Model, b: BodyId) -> f64 {
    Occt.mass_props(&m.body(b).unwrap().shape).unwrap().volume
}

fn plate(d: &mut Document) -> FeatureId {
    d.add_feature(FeatureKind::Box {
        corner: Vec3Expr::num(0.0, 0.0, 0.0),
        size: Vec3Expr(["L".into(), "W".into(), "T".into()]),
        op: BodyOp::NewBody,
    })
    .unwrap()
}

fn params(d: &mut Document) {
    d.set_parameter("L", "60".into(), None).unwrap();
    d.set_parameter("W", "40".into(), None).unwrap();
    d.set_parameter("T", "5".into(), None).unwrap();
}

#[test]
fn box_with_parameters_and_a_cut_cylinder() {
    let mut d = Document::default();
    params(&mut d);
    let p = plate(&mut d);
    let body = BodyId(p.0);
    d.add_feature(FeatureKind::Cylinder {
        base: Vec3Expr(["L/2".into(), "W/2".into(), "-1".into()]),
        axis: Vec3Expr::num(0.0, 0.0, 1.0),
        radius: "3".into(),
        height: "T + 2".into(),
        op: BodyOp::Cut(body),
    })
    .unwrap();
    let m = regen(&d, &Occt);
    assert_eq!(m.error_count(), 0, "{:?}", m.status);
    assert_eq!(m.bodies.len(), 1);
    assert!((vol(&m, body) - (60.0 * 40.0 * 5.0 - PI * 9.0 * 5.0)).abs() < 1e-6);

    // Change a parameter → the model follows.
    d.set_parameter("T", "10".into(), None).unwrap();
    let m = regen(&d, &Occt);
    assert!((vol(&m, body) - (60.0 * 40.0 * 10.0 - PI * 9.0 * 10.0)).abs() < 1e-6);
}

#[test]
fn errors_are_per_feature_and_regen_continues() {
    let mut d = Document::default();
    params(&mut d);
    let p = plate(&mut d);
    // Bad radius
    d.add_feature(FeatureKind::Sphere {
        center: Vec3Expr::num(0.0, 0.0, 0.0),
        radius: "-1".into(),
        op: BodyOp::NewBody,
    })
    .unwrap();
    // Unknown parameter
    d.add_feature(FeatureKind::Sphere {
        center: Vec3Expr::num(0.0, 0.0, 0.0),
        radius: "nope".into(),
        op: BodyOp::NewBody,
    })
    .unwrap();
    // Cut into a body that does not exist
    d.add_feature(FeatureKind::Box {
        corner: Vec3Expr::num(0.0, 0.0, 0.0),
        size: Vec3Expr::num(1.0, 1.0, 1.0),
        op: BodyOp::Cut(BodyId(999)),
    })
    .unwrap();
    // A good feature after the bad ones still runs.
    d.add_feature(FeatureKind::Box {
        corner: Vec3Expr::num(100.0, 0.0, 0.0),
        size: Vec3Expr::num(1.0, 1.0, 1.0),
        op: BodyOp::NewBody,
    })
    .unwrap();
    let m = regen(&d, &Occt);
    assert_eq!(m.error_count(), 3, "{:?}", m.status);
    assert_eq!(m.bodies.len(), 2);
    assert!(m.body(BodyId(p.0)).is_some());
    let msgs: Vec<String> = m.errors().map(|s| format!("{:?}", s.status)).collect();
    assert!(msgs.iter().any(|s| s.contains("nope")), "{msgs:?}");
}

#[test]
fn combine_consumes_the_tool_unless_kept() {
    let mut d = Document::default();
    let a = d
        .add_feature(FeatureKind::Box {
            corner: Vec3Expr::num(0.0, 0.0, 0.0),
            size: Vec3Expr::num(10.0, 10.0, 10.0),
            op: BodyOp::NewBody,
        })
        .unwrap();
    let b = d
        .add_feature(FeatureKind::Box {
            corner: Vec3Expr::num(5.0, 5.0, 5.0),
            size: Vec3Expr::num(10.0, 10.0, 10.0),
            op: BodyOp::NewBody,
        })
        .unwrap();
    let c = d
        .add_feature(FeatureKind::Combine {
            target: BodyId(a.0),
            tool: BodyId(b.0),
            operation: CombineOp::Join,
            keep_tool: false,
        })
        .unwrap();
    let m = regen(&d, &Occt);
    assert_eq!(m.bodies.len(), 1);
    assert!((vol(&m, BodyId(a.0)) - 1875.0).abs() < 1e-6);
    if let Some(f) = d.feature_mut(c)
        && let FeatureKind::Combine { keep_tool, .. } = &mut f.kind
    {
        *keep_tool = true;
    }
    assert_eq!(regen(&d, &Occt).bodies.len(), 2);
}

#[test]
fn marker_and_suppression() {
    let mut d = Document::default();
    let a = d
        .add_feature(FeatureKind::Box {
            corner: Vec3Expr::num(0.0, 0.0, 0.0),
            size: Vec3Expr::num(10.0, 10.0, 10.0),
            op: BodyOp::NewBody,
        })
        .unwrap();
    d.add_feature(FeatureKind::Box {
        corner: Vec3Expr::num(0.0, 0.0, 0.0),
        size: Vec3Expr::num(5.0, 5.0, 20.0),
        op: BodyOp::Join(BodyId(a.0)),
    })
    .unwrap();
    assert!((vol(&regen(&d, &Occt), BodyId(a.0)) - (1000.0 + 250.0)).abs() < 1e-6);
    d.set_marker(Some(1));
    let m = regen(&d, &Occt);
    assert_eq!(m.status[1].status, Status::RolledBack);
    assert!((vol(&m, BodyId(a.0)) - 1000.0).abs() < 1e-6);
    d.set_marker(None);
    d.features[1].suppressed = true;
    assert!((vol(&regen(&d, &Occt), BodyId(a.0)) - 1000.0).abs() < 1e-6);
}

#[test]
fn negative_box_size_extends_backwards() {
    let mut d = Document::default();
    let a = d
        .add_feature(FeatureKind::Box {
            corner: Vec3Expr::num(0.0, 0.0, 0.0),
            size: Vec3Expr::num(-10.0, 5.0, -2.0),
            op: BodyOp::NewBody,
        })
        .unwrap();
    let m = regen(&d, &Occt);
    let bb = Occt.mass_props(&m.body(BodyId(a.0)).unwrap().shape).unwrap().bbox;
    assert!(bb.min.distance(DVec3::new(-10.0, 0.0, -2.0)) < 1e-9);
}
