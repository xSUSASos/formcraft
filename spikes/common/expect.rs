// Shared spike harness: analytic expectations and mesh-based volume/bbox.
#![allow(dead_code)]
use std::f64::consts::PI;

pub struct Case {
    pub name: &'static str,
    pub volume: f64,
    pub bbox: [f64; 6],
}

pub fn cases() -> Vec<Case> {
    let r: f64 = 2.0;
    let a: f64 = 20.0 - 2.0 * r;
    let fillet_v = a * a * a + 2.0 * r * 3.0 * a * a + PI * r * r * 3.0 * a + 4.0 / 3.0 * PI * r * r * r;
    vec![
        Case { name: "box_hole", volume: 60.0 * 40.0 * 10.0 - PI * 25.0 * 10.0, bbox: [0., 0., 0., 60., 40., 10.] },
        Case { name: "fillet_all", volume: fillet_v, bbox: [0., 0., 0., 20., 20., 20.] },
        Case { name: "cyl_cut", volume: 40.0 * 40.0 * 20.0 - PI * 100.0 * 20.0, bbox: [0., 0., 0., 40., 40., 20.] },
        Case { name: "revolve", volume: PI * (400.0 - 100.0) * 30.0, bbox: [-20., -20., 0., 20., 20., 30.] },
        Case { name: "shell", volume: 40.0 * 30.0 * 20.0 - 36.0 * 26.0 * 18.0, bbox: [0., 0., 0., 40., 30., 20.] },
    ]
}

pub fn mesh_volume(v: &[[f64; 3]], tri: &[[usize; 3]]) -> f64 {
    let mut s = 0.0;
    for t in tri {
        let (a, b, c) = (v[t[0]], v[t[1]], v[t[2]]);
        s += a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    s / 6.0
}

pub fn bbox(v: &[[f64; 3]]) -> [f64; 6] {
    let mut b = [f64::MAX, f64::MAX, f64::MAX, f64::MIN, f64::MIN, f64::MIN];
    for p in v {
        for i in 0..3 {
            b[i] = b[i].min(p[i]);
            b[i + 3] = b[i + 3].max(p[i]);
        }
    }
    b
}

pub fn report(kernel: &str, name: &str, got: Option<(f64, [f64; 6])>, note: &str) -> bool {
    let c = cases().into_iter().find(|c| c.name == name).unwrap();
    match got {
        None => {
            println!("{kernel:8} {name:12} FAIL  {note}");
            false
        }
        Some((vol, bb)) => {
            let dv = ((vol.abs() - c.volume) / c.volume).abs();
            let db = bb.iter().zip(c.bbox.iter()).map(|(x, y)| (x - y).abs()).fold(0.0, f64::max);
            let ok = dv < 0.01 && db < 0.05;
            println!(
                "{kernel:8} {name:12} {}  vol={vol:.2} expected={:.2} (err {:.3}%) bbox_err={db:.3} {note}",
                if ok { "PASS" } else { "FAIL" },
                c.volume,
                dv * 100.0
            );
            ok
        }
    }
}
