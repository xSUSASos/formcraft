//! cxx bridge to `cpp/bridge.hxx`. Every fallible function is declared `Result`: the C++ side
//! converts all exceptions into `std::runtime_error`, which cxx turns into `Err`.

#[cxx::bridge(namespace = "formcraft")]
pub(crate) mod bridge {
    /// Flat triangulation buffers filled by `tessellate`.
    #[derive(Default)]
    struct MeshData {
        positions: Vec<f32>,
        normals: Vec<f32>,
        indices: Vec<u32>,
        triangle_face: Vec<u32>,
        edge_ids: Vec<u32>,
        /// End offset (in points) of each edge polyline in `edge_points`.
        edge_offsets: Vec<u32>,
        edge_points: Vec<f32>,
    }

    unsafe extern "C++" {
        include!("formcraft-occt/cpp/bridge.hxx");

        type Shape;

        fn make_box(x: f64, y: f64, z: f64, dx: f64, dy: f64, dz: f64) -> Result<UniquePtr<Shape>>;
        fn make_cylinder(base: &[f64], axis: &[f64], r: f64, h: f64) -> Result<UniquePtr<Shape>>;
        fn make_cone(base: &[f64], axis: &[f64], r1: f64, r2: f64, h: f64) -> Result<UniquePtr<Shape>>;
        fn make_sphere(center: &[f64], r: f64) -> Result<UniquePtr<Shape>>;
        fn make_torus(center: &[f64], axis: &[f64], r1: f64, r2: f64) -> Result<UniquePtr<Shape>>;

        fn make_faces(
            segs: &[f64],
            loop_sizes: &[u32],
            region_sizes: &[u32],
            normal: &[f64],
        ) -> Result<UniquePtr<Shape>>;
        fn prism(s: &Shape, dx: f64, dy: f64, dz: f64) -> Result<UniquePtr<Shape>>;
        fn face_prism(s: &Shape, face: u32, dx: f64, dy: f64, dz: f64) -> Result<UniquePtr<Shape>>;
        fn revolve(s: &Shape, origin: &[f64], dir: &[f64], angle: f64) -> Result<UniquePtr<Shape>>;

        fn boolean_op(a: &Shape, b: &Shape, op: u32) -> Result<UniquePtr<Shape>>;
        fn fillet(s: &Shape, edges: &[u32], r: f64) -> Result<UniquePtr<Shape>>;
        fn chamfer(s: &Shape, edges: &[u32], d: f64) -> Result<UniquePtr<Shape>>;
        fn thick_solid(s: &Shape, faces: &[u32], offset: f64) -> Result<UniquePtr<Shape>>;
        fn transform(s: &Shape, m: &[f64]) -> Result<UniquePtr<Shape>>;
        fn compound_new() -> Result<UniquePtr<Shape>>;
        fn compound_add(compound: Pin<&mut Shape>, s: &Shape) -> Result<()>;
        fn solid_count(s: &Shape) -> Result<u32>;
        fn solid_at(s: &Shape, i: u32) -> Result<UniquePtr<Shape>>;

        fn face_info(s: &Shape) -> Result<Vec<f64>>;
        fn edge_info(s: &Shape) -> Result<Vec<f64>>;
        fn mass_props(s: &Shape) -> Result<Vec<f64>>;
        fn is_valid(s: &Shape) -> bool;
        fn tessellate(s: &Shape, deflection: f64, out: &mut MeshData) -> Result<()>;

        fn write_step(s: &Shape, path: &str) -> Result<()>;
        fn read_step(path: &str) -> Result<UniquePtr<Shape>>;
        fn to_brep(s: &Shape) -> Result<Vec<u8>>;
        fn from_brep(bytes: &[u8]) -> Result<UniquePtr<Shape>>;
    }
}

// SAFETY: a TopoDS_Shape is a handle to reference-counted (atomic) topology. All access from
// Rust goes through `crate::KERNEL_LOCK`, so no two threads run OCCT code on it at once
// (meshing writes triangulations into shared topology even through `const`).
unsafe impl Send for bridge::Shape {}
unsafe impl Sync for bridge::Shape {}
