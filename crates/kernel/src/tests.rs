use super::*;

#[test]
fn plane_round_trip() {
    let p = Plane::new(DVec3::new(1.0, 2.0, 3.0), DVec3::new(1.0, 0.0, 0.1), DVec3::Z).unwrap();
    assert!((p.x_dir.dot(p.normal)).abs() < 1e-12);
    let q = DVec2::new(3.5, -2.0);
    assert!(p.to_local(p.to_world(q)).distance(q) < 1e-12);
    assert!(Plane::XZ.y_dir().distance(DVec3::Z) < 1e-12, "XZ sketch y goes up");
}

#[test]
fn degenerate_plane_rejected() {
    assert!(Plane::new(DVec3::ZERO, DVec3::Z, DVec3::Z).is_err());
    assert!(Plane::new(DVec3::ZERO, DVec3::X, DVec3::ZERO).is_err());
    assert!(Plane::new(DVec3::splat(f64::NAN), DVec3::X, DVec3::Z).is_err());
}

#[test]
fn loops_validate() {
    assert!(Loop2::rect(DVec2::ZERO, DVec2::new(10.0, 5.0)).validate().is_ok());
    assert!(Loop2::circle(DVec2::ZERO, 3.0).validate().is_ok());
    assert!(Loop2::circle(DVec2::ZERO, 0.0).validate().is_err());
    assert!(Loop2::default().validate().is_err());
    // open loop
    let open =
        Loop2::new(vec![Segment2::Line { a: DVec2::ZERO, b: DVec2::X }, Segment2::Line { a: DVec2::X, b: DVec2::ONE }]);
    assert!(open.validate().is_err());
    // repeated point gives a zero-length edge (crashed opencascade-rs in the spike)
    let dup = Loop2::polygon(&[DVec2::ZERO, DVec2::X, DVec2::ONE, DVec2::ZERO]);
    assert!(dup.validate().is_err());
}

#[test]
fn mesh_volume_of_unit_tetra_cube() {
    // unit cube, 12 outward triangles
    let p = |x: f32, y: f32, z: f32| [x, y, z];
    let positions = vec![
        p(0., 0., 0.),
        p(1., 0., 0.),
        p(1., 1., 0.),
        p(0., 1., 0.),
        p(0., 0., 1.),
        p(1., 0., 1.),
        p(1., 1., 1.),
        p(0., 1., 1.),
    ];
    let indices = vec![
        0, 2, 1, 0, 3, 2, // bottom
        4, 5, 6, 4, 6, 7, // top
        0, 1, 5, 0, 5, 4, // front
        1, 2, 6, 1, 6, 5, // right
        2, 3, 7, 2, 7, 6, // back
        3, 0, 4, 3, 4, 7, // left
    ];
    let m = Mesh { positions, indices, triangle_face: vec![0; 12], ..Default::default() };
    assert!((m.volume() - 1.0).abs() < 1e-9);
    assert!((m.area() - 6.0).abs() < 1e-9);
    assert_eq!(m.bbox().size(), DVec3::ONE);
}

#[test]
fn shape_downcast() {
    let s = Shape::new("test", 42u32);
    assert_eq!(*s.get::<u32>().unwrap(), 42);
    assert_eq!(s.get::<String>().unwrap_err(), KernelError::ForeignShape);
    assert!(s.same(&s.clone()));
}
