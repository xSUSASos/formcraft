// FormCraft OCCT bridge: a small C++ facade over OpenCascade for the cxx bridge in src/ffi.rs.
// Every function catches OCCT and C++ exceptions and rethrows std::runtime_error, which cxx
// turns into a Rust Err (functions are declared as Result on the Rust side). Nothing may abort.
#pragma once

#include <memory>
#include <TopoDS_Shape.hxx>

#include "rust/cxx.h"

namespace formcraft {

using Shape = TopoDS_Shape;
struct MeshData;

// --- primitives ---
std::unique_ptr<Shape> make_box(double x, double y, double z, double dx, double dy, double dz);
std::unique_ptr<Shape> make_cylinder(rust::Slice<const double> base, rust::Slice<const double> axis, double r,
                                     double h);
std::unique_ptr<Shape> make_cone(rust::Slice<const double> base, rust::Slice<const double> axis, double r1,
                                 double r2, double h);
std::unique_ptr<Shape> make_sphere(rust::Slice<const double> center, double r);
std::unique_ptr<Shape> make_torus(rust::Slice<const double> center, rust::Slice<const double> axis, double r1,
                                  double r2);

// --- profiles and sweeps ---
// segs: stride 10 per segment: [kind, 9 params] with kind 0 = line (p0, p1),
// 1 = arc (start, mid, end), 2 = circle (center, normal, radius). All points are 3D.
// loop_sizes: segment count of each loop; region_sizes: loop count of each region
// (first loop of a region is the outer boundary, the rest are holes). normal: plane normal.
std::unique_ptr<Shape> make_faces(rust::Slice<const double> segs, rust::Slice<const uint32_t> loop_sizes,
                                  rust::Slice<const uint32_t> region_sizes, rust::Slice<const double> normal);
std::unique_ptr<Shape> prism(const Shape &s, double dx, double dy, double dz);
std::unique_ptr<Shape> revolve(const Shape &s, rust::Slice<const double> origin, rust::Slice<const double> dir,
                               double angle);

// --- modifications ---
// op: 0 union, 1 cut, 2 intersect
std::unique_ptr<Shape> boolean_op(const Shape &a, const Shape &b, uint32_t op);
std::unique_ptr<Shape> fillet(const Shape &s, rust::Slice<const uint32_t> edges, double r);
std::unique_ptr<Shape> chamfer(const Shape &s, rust::Slice<const uint32_t> edges, double d);
std::unique_ptr<Shape> thick_solid(const Shape &s, rust::Slice<const uint32_t> faces, double offset);
// m: 12 doubles, row-major 3x4 affine matrix
std::unique_ptr<Shape> transform(const Shape &s, rust::Slice<const double> m);
std::unique_ptr<Shape> compound_new();
void compound_add(Shape &compound, const Shape &s);
uint32_t solid_count(const Shape &s);
std::unique_ptr<Shape> solid_at(const Shape &s, uint32_t i);

// --- queries ---
// Per face 21 doubles: kind, origin(3), axis(3), radius, centroid(3), normal(3), area, bbox(6).
rust::Vec<double> face_info(const Shape &s);
// Per edge: [kind, start3, end3, mid3, length, c3, axis3, radius, nfaces, faces...]; see ffi.rs.
rust::Vec<double> edge_info(const Shape &s);
// [volume, area, cx, cy, cz, xmin, ymin, zmin, xmax, ymax, zmax]
rust::Vec<double> mass_props(const Shape &s);
bool is_valid(const Shape &s);
void tessellate(const Shape &s, double deflection, MeshData &out);

// --- exchange ---
void write_step(const Shape &s, rust::Str path);
std::unique_ptr<Shape> read_step(rust::Str path);
rust::Vec<uint8_t> to_brep(const Shape &s);
std::unique_ptr<Shape> from_brep(rust::Slice<const uint8_t> bytes);

} // namespace formcraft
