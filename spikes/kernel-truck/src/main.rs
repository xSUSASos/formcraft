use std::f64::consts::PI;
use truck_meshalgo::prelude::*;
use truck_modeling::*;
use truck_stepio::out;

#[path = "../../common/expect.rs"]
mod expect;

fn cuboid(o: Point3, d: Vector3) -> Solid {
    let v = builder::vertex(o);
    let e = builder::tsweep(&v, Vector3::new(d.x, 0., 0.));
    let f = builder::tsweep(&e, Vector3::new(0., d.y, 0.));
    builder::tsweep(&f, Vector3::new(0., 0., d.z))
}

fn cylinder(c: Point3, r: f64, h: f64) -> Solid {
    let v = builder::vertex(Point3::new(c.x + r, c.y, c.z));
    let w = builder::rsweep(&v, c, Vector3::unit_z(), Rad(2.0 * PI));
    let f = builder::try_attach_plane(&[w]).unwrap();
    builder::tsweep(&f, Vector3::new(0., 0., h))
}

fn measure(s: &Solid) -> Option<(f64, [f64; 6])> {
    let mesh = s.triangulation(0.005).to_polygon();
    let v: Vec<[f64; 3]> = mesh.positions().iter().map(|p| [p.x, p.y, p.z]).collect();
    let t: Vec<[usize; 3]> = mesh.faces().triangle_iter().map(|t| [t[0].pos, t[1].pos, t[2].pos]).collect();
    if t.is_empty() {
        return None;
    }
    Some((expect::mesh_volume(&v, &t), expect::bbox(&v)))
}

fn step(s: &Solid, path: &str) {
    let c = s.compress();
    let txt = out::CompleteStepDisplay::new(out::StepModel::from(&c), Default::default()).to_string();
    std::fs::write(path, txt).unwrap();
}

fn main() {
    std::fs::create_dir_all("out").unwrap();
    let mut pass = 0;
    let mut run = |name: &str, s: Option<Solid>, note: &str| {
        let m = s.as_ref().and_then(measure);
        if expect::report("truck", name, m, note) {
            pass += 1;
        }
        if let Some(s) = s {
            step(&s, &format!("out/{name}.step"));
        }
    };
    // 1. box with hole
    let b = cuboid(Point3::origin(), Vector3::new(60., 40., 10.));
    let mut c = cylinder(Point3::new(30., 20., -1.), 5.0, 12.0);
    c.not();
    run("box_hole", truck_shapeops::and(&b, &c, 0.05), "");
    // 2. fillet all edges: no API in truck-shapeops 0.4 (master has single-edge fillet only)
    run("fillet_all", None, "no multi-edge fillet API (0.4; master: simple_fillet per edge pair, no corner blends)");
    // 3. cylinder cut
    let b = cuboid(Point3::origin(), Vector3::new(40., 40., 20.));
    let mut c = cylinder(Point3::new(20., 20., -1.), 10.0, 22.0);
    c.not();
    run("cyl_cut", truck_shapeops::and(&b, &c, 0.05), "");
    // 4. revolve
    let v0 = builder::vertex(Point3::new(10., 0., 0.));
    let v1 = builder::vertex(Point3::new(20., 0., 0.));
    let v2 = builder::vertex(Point3::new(20., 0., 30.));
    let v3 = builder::vertex(Point3::new(10., 0., 30.));
    let w: Wire = vec![builder::line(&v0, &v1), builder::line(&v1, &v2), builder::line(&v2, &v3), builder::line(&v3, &v0)].into();
    let f = builder::try_attach_plane(&[w]).unwrap();
    let rev = builder::rsweep(&f, Point3::origin(), Vector3::unit_z(), Rad(2.0 * PI));
    run("revolve", Some(rev), "");
    // 5. shell: no offset/thicken API; emulate with boolean of two boxes
    let outer = cuboid(Point3::origin(), Vector3::new(40., 30., 20.));
    let mut inner = cuboid(Point3::new(2., 2., 2.), Vector3::new(36., 26., 19.));
    inner.not();
    run("shell", truck_shapeops::and(&outer, &inner, 0.05), "(emulated by boolean: no offset/thick-solid API)");
    println!("truck: {pass}/5 passed");
}
