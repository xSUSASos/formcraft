use glam::{DVec3, dvec3};
use opencascade::primitives::{Direction, Face, Shape, Wire};

#[path = "../../common/expect.rs"]
mod expect;

fn measure(s: &Shape) -> Option<(f64, [f64; 6])> {
    let m = s.mesh_with_tolerance(0.005).ok()?;
    let v: Vec<[f64; 3]> = m.vertices.iter().map(|p| [p.x, p.y, p.z]).collect();
    let t: Vec<[usize; 3]> = m.indices.chunks(3).map(|c| [c[0], c[1], c[2]]).collect();
    Some((expect::mesh_volume(&v, &t), expect::bbox(&v)))
}

fn main() {
    let out = std::path::Path::new("out");
    std::fs::create_dir_all(out).unwrap();
    let mut results: Vec<(&str, Shape)> = vec![];

    let b = Shape::box_from_corners(dvec3(0., 0., 0.), dvec3(60., 40., 10.));
    let cyl = Shape::cylinder(dvec3(30., 20., -1.), 5.0, DVec3::Z, 12.0);
    results.push(("box_hole", b.subtract(&cyl).into()));

    let b = Shape::box_from_corners(dvec3(0., 0., 0.), dvec3(20., 20., 20.));
    results.push(("fillet_all", b.fillet(2.0)));

    let b = Shape::box_from_corners(dvec3(0., 0., 0.), dvec3(40., 40., 20.));
    let cyl = Shape::cylinder(dvec3(20., 20., -1.), 10.0, DVec3::Z, 22.0);
    results.push(("cyl_cut", b.subtract(&cyl).into()));

    let w = Wire::from_ordered_points([dvec3(10., 0., 0.), dvec3(20., 0., 0.), dvec3(20., 0., 30.), dvec3(10., 0., 30.), dvec3(10., 0., 0.)]).unwrap();
    let rev: Shape = Face::from_wire(&w).revolve(DVec3::ZERO, DVec3::Z, None).into();
    results.push(("revolve", rev));

    let b = Shape::box_from_corners(dvec3(0., 0., 0.), dvec3(40., 30., 20.));
    let top = b.faces().farthest(Direction::PosZ);
    results.push(("shell", b.hollow(-2.0, [top])));

    let mut pass = 0;
    for (name, s) in &results {
        if expect::report("occt", name, measure(s), "") {
            pass += 1;
        }
        let p = out.join(format!("{name}.step"));
        s.write_step(&p).unwrap();
        let back = Shape::read_step(&p).unwrap();
        expect::report("occt-rt", name, measure(&back), "(step round-trip)");
    }
    println!("occt: {pass}/{} passed", results.len());
}
