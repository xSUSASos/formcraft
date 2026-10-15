//! Headless GPU render smoke test. Skips (passes) when the machine has no usable adapter.

use formcraft_kernel::{DVec3, Kernel};
use formcraft_occt::Occt;

use crate::camera::Camera;
use crate::gpu::Renderer;
use crate::scene::{self, GridColors, Item, Scene};

#[test]
fn renders_a_box_offscreen() {
    let Ok(mut r) = Renderer::headless() else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let shape = Occt.make_box(DVec3::ZERO, DVec3::new(40.0, 30.0, 20.0)).unwrap();
    let mesh = Occt.tessellate(&shape, 0.1).unwrap();
    let red = [0.8, 0.1, 0.1, 1.0];
    let (v, i) = scene::mesh_vertices(&mesh, red, &|_| None);
    let e = scene::edge_vertices(&mesh, [0.0, 0.0, 0.0, 1.0], &|_| None);
    r.set_mesh(1, &v, &i, &e);
    let mut cam = Camera::home();
    cam.fit(DVec3::ZERO, DVec3::new(40.0, 30.0, 20.0), 4.0 / 3.0);
    let grid = GridColors {
        minor: [0.5; 4],
        major: [0.6; 4],
        x: [1.0, 0.0, 0.0, 1.0],
        y: [0.0, 1.0, 0.0, 1.0],
        z: [0.0, 0.0, 1.0, 1.0],
    };
    let s = Scene {
        camera: cam,
        background: [0.0, 0.0, 0.0, 1.0],
        lines: scene::grid_lines(&cam, &grid, true),
        items: vec![Item { key: 1, visible: true, shaded: true, edges: true, transparent: false }],
    };
    let (w, h) = (320u32, 240u32);
    let px = r.render_rgba(&s, [w, h]).unwrap();
    assert_eq!(px.len(), (w * h * 4) as usize);
    // The box projects onto the middle of the image: mostly red there.
    let at = |x: u32, y: u32| {
        let o = ((y * w + x) * 4) as usize;
        [px[o] as u16, px[o + 1] as u16, px[o + 2] as u16]
    };
    let c = at(w / 2, h / 2);
    assert!(c[0] > 60 && c[0] > c[1] + 25 && c[0] > c[2] + 25, "centre pixel {c:?} is not the red body");
    let corner = at(1, 1);
    assert!(corner[0] < 30 && corner[1] < 30, "corner {corner:?} is background");
    let png = crate::encode_png(&px, w, h).unwrap();
    assert_eq!(&png[1..4], b"PNG");
    if let Ok(path) = std::env::var("FORMCRAFT_RENDER_TEST_PNG") {
        let _ = std::fs::write(path, &png);
    }
}
