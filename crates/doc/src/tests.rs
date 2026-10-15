use super::*;

fn boxed(x: f64) -> FeatureKind {
    FeatureKind::Box { corner: Vec3Expr::num(0.0, 0.0, 0.0), size: Vec3Expr::num(x, x, x), op: BodyOp::NewBody }
}

#[test]
fn features_get_names_ids_and_bodies() {
    let mut d = Document::default();
    let a = d.add_feature(boxed(10.0)).unwrap();
    let b = d.add_feature(boxed(5.0)).unwrap();
    assert_ne!(a, b);
    assert_eq!(d.feature(a).unwrap().name, "Box1");
    assert_eq!(d.feature(b).unwrap().name, "Box2");
    assert_eq!(d.body_name(BodyId(a.0)), "Body1");
    let c = d
        .add_feature(FeatureKind::Combine {
            target: BodyId(a.0),
            tool: BodyId(b.0),
            operation: CombineOp::Cut,
            keep_tool: false,
        })
        .unwrap();
    assert_eq!(d.feature(c).unwrap().name, "Combine1");
    assert_eq!(d.bodies.len(), 2, "combine creates no body");
}

#[test]
fn marker_rolls_back_and_new_features_go_at_the_marker() {
    let mut d = Document::default();
    let a = d.add_feature(boxed(1.0)).unwrap();
    let b = d.add_feature(boxed(2.0)).unwrap();
    d.set_marker(Some(1));
    assert_eq!(d.active_features().len(), 1);
    let c = d.add_feature(boxed(3.0)).unwrap();
    let order: Vec<FeatureId> = d.features.iter().map(|f| f.id).collect();
    assert_eq!(order, vec![a, c, b]);
    assert_eq!(d.marker, Some(2));
    d.set_marker(Some(99));
    assert_eq!(d.marker, None, "a marker past the end means 'all'");
}

#[test]
fn delete_and_reorder() {
    let mut d = Document::default();
    let a = d.add_feature(boxed(1.0)).unwrap();
    let b = d.add_feature(boxed(2.0)).unwrap();
    d.reorder(b, 0).unwrap();
    assert_eq!(d.features[0].id, b);
    d.delete_feature(a).unwrap();
    assert_eq!(d.features.len(), 1);
    assert!(d.delete_feature(a).is_err());
    assert!(!d.bodies.contains_key(&BodyId(a.0)));
}

#[test]
fn parameters_are_checked() {
    let mut d = Document::default();
    d.set_parameter("plate_length", "60".into(), Some("length")).unwrap();
    d.set_parameter("hole_d", "plate_length / 10".into(), None).unwrap();
    assert_eq!(d.parameter_values().unwrap()["hole_d"], 6.0);
    assert_eq!(d.eval(&"hole_d * 2".into()).unwrap(), 12.0);
    assert!(matches!(d.set_parameter("1bad", "1".into(), None), Err(DocError::BadName(_))));
    assert!(matches!(d.set_parameter("pi", "1".into(), None), Err(DocError::BadName(_))));
    assert!(d.set_parameter("x", "nope + 1".into(), None).is_err());
    assert!(d.set_parameter("plate_length", "hole_d".into(), None).is_err(), "cycle refused");
    assert_eq!(d.parameter("plate_length").unwrap().expr.as_str(), "60", "a refused edit changes nothing");
    assert!(d.remove_parameter("plate_length").is_err(), "still used by hole_d");
    d.remove_parameter("hole_d").unwrap();
    d.remove_parameter("plate_length").unwrap();
}

#[test]
fn json_round_trip_and_hostile_json() {
    let mut d = Document::new("Bracket");
    d.set_parameter("t", "5".into(), None).unwrap();
    let a = d.add_feature(boxed(10.0)).unwrap();
    let json = d.to_json().unwrap();
    let back = Document::from_json(&json).unwrap();
    assert_eq!(back, d);
    let mut back = back;
    let b = back.add_feature(boxed(1.0)).unwrap();
    assert!(b.0 > a.0, "ids continue after load");
    assert!(Document::from_json("{\"features\": 5}").is_err());
    assert!(Document::from_json("not json").is_err());
    let marker = Document::from_json("{\"marker\": 1000}").unwrap();
    assert_eq!(marker.marker, None);
}
