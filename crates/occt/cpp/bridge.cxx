// FormCraft OCCT bridge implementation. Written against the public OpenCascade 7.8 reference
// documentation. Every entry point runs inside FC_GUARD so OCCT exceptions become
// std::runtime_error (→ Rust Err) instead of aborting the process.
#include "formcraft-occt/cpp/bridge.hxx"
#include "formcraft-occt/src/ffi.rs.h"

#include <cmath>
#include <sstream>
#include <stdexcept>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepGProp_Face.hxx>
#include <BRepLib_ToolTriangulatedShape.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCone.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <BRepPrimAPI_MakeTorus.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GCPnts_TangentialDeflection.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GProp_GProps.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <IFSelect_ReturnStatus.hxx>
#include <Message.hxx>
#include <Message_Messenger.hxx>
#include <Message_Printer.hxx>
#include <Poly_Triangulation.hxx>
#include <STEPControl_Reader.hxx>
#include <STEPControl_Writer.hxx>
#include <ShapeFix_Face.hxx>
#include <ShapeFix_Shape.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_IndexedDataMapOfShapeListOfShape.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopTools_ListOfShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Pln.hxx>
#include <gp_Trsf.hxx>

#define FC_GUARD(op, ...)                                                                                       \
  try {                                                                                                        \
    __VA_ARGS__                                                                                                \
  } catch (const Standard_Failure &e) {                                                                        \
    throw std::runtime_error(std::string(op) + ": " + (e.GetMessageString() ? e.GetMessageString() : "") +     \
                             " (" + e.DynamicType()->Name() + ")");                                            \
  } catch (const std::exception &e) {                                                                          \
    throw std::runtime_error(std::string(op) + ": " + e.what());                                               \
  } catch (...) {                                                                                              \
    throw std::runtime_error(std::string(op) + ": unknown C++ exception");                                     \
  }

namespace formcraft {

namespace {

gp_Pnt pnt(rust::Slice<const double> v, size_t at = 0) {
  if (v.size() < at + 3) throw std::runtime_error("point needs 3 coordinates");
  return gp_Pnt(v[at], v[at + 1], v[at + 2]);
}

gp_Dir dir(rust::Slice<const double> v, size_t at = 0) {
  if (v.size() < at + 3) throw std::runtime_error("direction needs 3 coordinates");
  gp_Vec d(v[at], v[at + 1], v[at + 2]);
  if (d.Magnitude() < 1e-12) throw std::runtime_error("zero-length direction");
  return gp_Dir(d);
}

std::unique_ptr<Shape> boxed(const TopoDS_Shape &s) {
  if (s.IsNull()) throw std::runtime_error("operation produced an empty shape");
  return std::make_unique<Shape>(s);
}

void quiet() {
  // STEP transfer statistics go to the default messenger's console printer; keep stdout clean
  // (stdout carries MCP JSON-RPC).
  static bool done = false;
  if (!done) {
    const Message_SequenceOfPrinters &printers = Message::DefaultMessenger()->Printers();
    for (Message_SequenceOfPrinters::Iterator it(printers); it.More(); it.Next()) {
      it.Value()->SetTraceLevel(Message_Fail);
    }
    done = true;
  }
}

TopoDS_Edge make_segment_edge(const double *p) {
  int kind = static_cast<int>(p[0]);
  const double *a = p + 1;
  if (kind == 0) {
    BRepBuilderAPI_MakeEdge me(gp_Pnt(a[0], a[1], a[2]), gp_Pnt(a[3], a[4], a[5]));
    if (!me.IsDone()) throw std::runtime_error("cannot build line edge");
    return me.Edge();
  }
  if (kind == 1) {
    GC_MakeArcOfCircle arc(gp_Pnt(a[0], a[1], a[2]), gp_Pnt(a[3], a[4], a[5]), gp_Pnt(a[6], a[7], a[8]));
    if (!arc.IsDone()) throw std::runtime_error("cannot build arc");
    BRepBuilderAPI_MakeEdge me(arc.Value());
    if (!me.IsDone()) throw std::runtime_error("cannot build arc edge");
    return me.Edge();
  }
  if (kind == 2) {
    gp_Vec n(a[3], a[4], a[5]);
    if (n.Magnitude() < 1e-12 || !(a[6] > 0)) throw std::runtime_error("bad circle");
    gp_Circ c(gp_Ax2(gp_Pnt(a[0], a[1], a[2]), gp_Dir(n)), a[6]);
    BRepBuilderAPI_MakeEdge me(c);
    if (!me.IsDone()) throw std::runtime_error("cannot build circle edge");
    return me.Edge();
  }
  throw std::runtime_error("unknown segment kind");
}

TopoDS_Shape indexed(const TopoDS_Shape &s, TopAbs_ShapeEnum kind, uint32_t i, TopTools_IndexedMapOfShape &map) {
  if (map.IsEmpty()) TopExp::MapShapes(s, kind, map);
  if (i >= static_cast<uint32_t>(map.Extent())) throw std::runtime_error("index out of range");
  return map(static_cast<int>(i) + 1);
}

void push3(rust::Vec<double> &v, const gp_XYZ &p) {
  v.push_back(p.X());
  v.push_back(p.Y());
  v.push_back(p.Z());
}

void push_bbox(rust::Vec<double> &v, const TopoDS_Shape &s) {
  Bnd_Box b;
  BRepBndLib::AddOptimal(s, b, Standard_False, Standard_False);
  double x0 = 0, y0 = 0, z0 = 0, x1 = 0, y1 = 0, z1 = 0;
  if (!b.IsVoid()) b.Get(x0, y0, z0, x1, y1, z1);
  for (double d : {x0, y0, z0, x1, y1, z1}) v.push_back(d);
}

} // namespace

// ---------------------------------------------------------------- primitives

std::unique_ptr<Shape> make_box(double x, double y, double z, double dx, double dy, double dz) {
  FC_GUARD("box", {
    BRepPrimAPI_MakeBox mk(gp_Pnt(x, y, z), dx, dy, dz);
    mk.Build();
    if (!mk.IsDone()) throw std::runtime_error("not done");
    return boxed(mk.Shape());
  })
}

std::unique_ptr<Shape> make_cylinder(rust::Slice<const double> base, rust::Slice<const double> axis, double r,
                                     double h) {
  FC_GUARD("cylinder", {
    BRepPrimAPI_MakeCylinder mk(gp_Ax2(pnt(base), dir(axis)), r, h);
    mk.Build();
    if (!mk.IsDone()) throw std::runtime_error("not done");
    return boxed(mk.Shape());
  })
}

std::unique_ptr<Shape> make_cone(rust::Slice<const double> base, rust::Slice<const double> axis, double r1,
                                 double r2, double h) {
  FC_GUARD("cone", {
    BRepPrimAPI_MakeCone mk(gp_Ax2(pnt(base), dir(axis)), r1, r2, h);
    mk.Build();
    if (!mk.IsDone()) throw std::runtime_error("not done");
    return boxed(mk.Shape());
  })
}

std::unique_ptr<Shape> make_sphere(rust::Slice<const double> center, double r) {
  FC_GUARD("sphere", {
    BRepPrimAPI_MakeSphere mk(pnt(center), r);
    mk.Build();
    if (!mk.IsDone()) throw std::runtime_error("not done");
    return boxed(mk.Shape());
  })
}

std::unique_ptr<Shape> make_torus(rust::Slice<const double> center, rust::Slice<const double> axis, double r1,
                                  double r2) {
  FC_GUARD("torus", {
    BRepPrimAPI_MakeTorus mk(gp_Ax2(pnt(center), dir(axis)), r1, r2);
    mk.Build();
    if (!mk.IsDone()) throw std::runtime_error("not done");
    return boxed(mk.Shape());
  })
}

// ---------------------------------------------------------------- profiles and sweeps

std::unique_ptr<Shape> make_faces(rust::Slice<const double> segs, rust::Slice<const uint32_t> loop_sizes,
                                  rust::Slice<const uint32_t> region_sizes, rust::Slice<const double> normal) {
  FC_GUARD("profile", {
    const size_t stride = 10;
    std::vector<TopoDS_Wire> wires;
    size_t seg = 0;
    for (uint32_t n : loop_sizes) {
      if (n == 0) throw std::runtime_error("empty loop");
      BRepBuilderAPI_MakeWire mw;
      for (uint32_t k = 0; k < n; ++k, ++seg) {
        if ((seg + 1) * stride > segs.size()) throw std::runtime_error("segment data too short");
        mw.Add(make_segment_edge(segs.data() + seg * stride));
        if (!mw.IsDone()) throw std::runtime_error("segments do not form a connected wire");
      }
      TopoDS_Wire w = mw.Wire();
      if (!w.Closed() && !BRep_Tool::IsClosed(w)) throw std::runtime_error("loop is not closed");
      wires.push_back(w);
    }
    gp_Dir n = dir(normal);
    BRep_Builder bb;
    TopoDS_Compound comp;
    bb.MakeCompound(comp);
    size_t w = 0;
    int faces = 0;
    for (uint32_t r : region_sizes) {
      if (r == 0 || w + r > wires.size()) throw std::runtime_error("bad region layout");
      // A plane through the first vertex of the outer wire, with the profile's normal.
      TopExp_Explorer ex(wires[w], TopAbs_VERTEX);
      if (!ex.More()) throw std::runtime_error("outer loop has no vertices");
      gp_Pln pl(BRep_Tool::Pnt(TopoDS::Vertex(ex.Current())), n);
      BRepBuilderAPI_MakeFace mf(pl, wires[w], Standard_True);
      if (!mf.IsDone()) throw std::runtime_error("cannot build face from outer loop");
      for (uint32_t h = 1; h < r; ++h) mf.Add(wires[w + h]);
      if (!mf.IsDone()) throw std::runtime_error("cannot add hole loop");
      ShapeFix_Face fix(mf.Face());
      fix.FixOrientation();
      fix.Perform();
      TopoDS_Face f = fix.Face();
      // Keep the face normal pointing along the profile normal.
      BRepAdaptor_Surface sa(f);
      if (sa.GetType() == GeomAbs_Plane) {
        gp_Dir fn = sa.Plane().Axis().Direction();
        if (f.Orientation() == TopAbs_REVERSED) fn.Reverse();
        if (fn.Dot(n) < 0) f.Reverse();
      }
      bb.Add(comp, f);
      ++faces;
      w += r;
    }
    if (faces == 1) {
      TopExp_Explorer ex(comp, TopAbs_FACE);
      return boxed(ex.Current());
    }
    return boxed(comp);
  })
}

std::unique_ptr<Shape> prism(const Shape &s, double dx, double dy, double dz) {
  FC_GUARD("extrude", {
    gp_Vec v(dx, dy, dz);
    if (v.Magnitude() < 1e-9) throw std::runtime_error("zero extrusion distance");
    BRepPrimAPI_MakePrism mk(s, v, Standard_True);
    mk.Build();
    if (!mk.IsDone()) throw std::runtime_error("not done");
    return boxed(mk.Shape());
  })
}

std::unique_ptr<Shape> revolve(const Shape &s, rust::Slice<const double> origin, rust::Slice<const double> d,
                               double angle) {
  FC_GUARD("revolve", {
    if (!(std::abs(angle) > 1e-9)) throw std::runtime_error("zero revolve angle");
    BRepPrimAPI_MakeRevol mk(s, gp_Ax1(pnt(origin), dir(d)), angle, Standard_True);
    mk.Build();
    if (!mk.IsDone()) throw std::runtime_error("not done");
    return boxed(mk.Shape());
  })
}

// ---------------------------------------------------------------- modifications

std::unique_ptr<Shape> boolean_op(const Shape &a, const Shape &b, uint32_t op) {
  FC_GUARD("boolean", {
    TopoDS_Shape r;
    if (op == 0) {
      BRepAlgoAPI_Fuse mk(a, b);
      if (mk.HasErrors()) throw std::runtime_error("union failed");
      mk.SimplifyResult();
      r = mk.Shape();
    } else if (op == 1) {
      BRepAlgoAPI_Cut mk(a, b);
      if (mk.HasErrors()) throw std::runtime_error("cut failed");
      mk.SimplifyResult();
      r = mk.Shape();
    } else if (op == 2) {
      BRepAlgoAPI_Common mk(a, b);
      if (mk.HasErrors()) throw std::runtime_error("intersect failed");
      mk.SimplifyResult();
      r = mk.Shape();
    } else {
      throw std::runtime_error("unknown boolean op");
    }
    return boxed(r);
  })
}

std::unique_ptr<Shape> fillet(const Shape &s, rust::Slice<const uint32_t> edges, double r) {
  FC_GUARD("fillet", {
    if (edges.empty()) throw std::runtime_error("no edges selected");
    BRepFilletAPI_MakeFillet mk(s);
    TopTools_IndexedMapOfShape map;
    for (uint32_t e : edges) mk.Add(r, TopoDS::Edge(indexed(s, TopAbs_EDGE, e, map)));
    mk.Build();
    if (!mk.IsDone()) throw std::runtime_error("radius too large for the selected edges or geometry not supported");
    return boxed(mk.Shape());
  })
}

std::unique_ptr<Shape> chamfer(const Shape &s, rust::Slice<const uint32_t> edges, double d) {
  FC_GUARD("chamfer", {
    if (edges.empty()) throw std::runtime_error("no edges selected");
    BRepFilletAPI_MakeChamfer mk(s);
    TopTools_IndexedMapOfShape map;
    for (uint32_t e : edges) mk.Add(d, TopoDS::Edge(indexed(s, TopAbs_EDGE, e, map)));
    mk.Build();
    if (!mk.IsDone()) throw std::runtime_error("distance too large for the selected edges");
    return boxed(mk.Shape());
  })
}

std::unique_ptr<Shape> thick_solid(const Shape &s, rust::Slice<const uint32_t> faces, double offset) {
  FC_GUARD("shell", {
    TopTools_ListOfShape remove;
    TopTools_IndexedMapOfShape map;
    for (uint32_t f : faces) remove.Append(indexed(s, TopAbs_FACE, f, map));
    BRepOffsetAPI_MakeThickSolid mk;
    mk.MakeThickSolidByJoin(s, remove, offset, 1e-4);
    mk.Build();
    if (!mk.IsDone()) throw std::runtime_error("wall thickness too large or geometry not supported");
    return boxed(mk.Shape());
  })
}

std::unique_ptr<Shape> transform(const Shape &s, rust::Slice<const double> m) {
  FC_GUARD("transform", {
    if (m.size() != 12) throw std::runtime_error("matrix needs 12 values");
    gp_Trsf t;
    t.SetValues(m[0], m[1], m[2], m[3], m[4], m[5], m[6], m[7], m[8], m[9], m[10], m[11]);
    BRepBuilderAPI_Transform mk(s, t, Standard_True);
    if (!mk.IsDone()) throw std::runtime_error("not done");
    return boxed(mk.Shape());
  })
}

std::unique_ptr<Shape> compound_new() {
  FC_GUARD("compound", {
    BRep_Builder b;
    TopoDS_Compound c;
    b.MakeCompound(c);
    return std::make_unique<Shape>(c);
  })
}

void compound_add(Shape &compound, const Shape &s) {
  FC_GUARD("compound", {
    BRep_Builder b;
    b.Add(compound, s);
  })
}

uint32_t solid_count(const Shape &s) {
  FC_GUARD("solids", {
    TopTools_IndexedMapOfShape map;
    TopExp::MapShapes(s, TopAbs_SOLID, map);
    return static_cast<uint32_t>(map.Extent());
  })
}

std::unique_ptr<Shape> solid_at(const Shape &s, uint32_t i) {
  FC_GUARD("solids", {
    TopTools_IndexedMapOfShape map;
    return boxed(indexed(s, TopAbs_SOLID, i, map));
  })
}

// ---------------------------------------------------------------- queries

rust::Vec<double> face_info(const Shape &s) {
  FC_GUARD("faces", {
    rust::Vec<double> out;
    TopTools_IndexedMapOfShape map;
    TopExp::MapShapes(s, TopAbs_FACE, map);
    for (int i = 1; i <= map.Extent(); ++i) {
      TopoDS_Face f = TopoDS::Face(map(i));
      BRepAdaptor_Surface sa(f, Standard_True);
      double kind = 5, radius = 0;
      gp_XYZ p(0, 0, 0), d(0, 0, 0);
      switch (sa.GetType()) {
      case GeomAbs_Plane: {
        kind = 0;
        p = sa.Plane().Location().XYZ();
        d = sa.Plane().Axis().Direction().XYZ();
        if (f.Orientation() == TopAbs_REVERSED) d.Reverse();
        break;
      }
      case GeomAbs_Cylinder:
        kind = 1;
        p = sa.Cylinder().Location().XYZ();
        d = sa.Cylinder().Axis().Direction().XYZ();
        radius = sa.Cylinder().Radius();
        break;
      case GeomAbs_Cone:
        kind = 2;
        p = sa.Cone().Location().XYZ();
        d = sa.Cone().Axis().Direction().XYZ();
        break;
      case GeomAbs_Sphere:
        kind = 3;
        p = sa.Sphere().Location().XYZ();
        radius = sa.Sphere().Radius();
        break;
      case GeomAbs_Torus:
        kind = 4;
        p = sa.Torus().Location().XYZ();
        d = sa.Torus().Axis().Direction().XYZ();
        break;
      default:
        break;
      }
      GProp_GProps props;
      BRepGProp::SurfaceProperties(f, props);
      gp_Pnt c = props.CentreOfMass();
      // Normal at the surface point nearest to the centroid (orientation-aware).
      gp_Vec nrm(0, 0, 1);
      double u = 0.5 * (sa.FirstUParameter() + sa.LastUParameter());
      double v = 0.5 * (sa.FirstVParameter() + sa.LastVParameter());
      Handle(Geom_Surface) surf = BRep_Tool::Surface(f);
      if (!surf.IsNull()) {
        GeomAPI_ProjectPointOnSurf proj(c, surf);
        if (proj.NbPoints() > 0) proj.LowerDistanceParameters(u, v);
      }
      BRepGProp_Face gf(f);
      gp_Pnt on;
      gf.Normal(u, v, on, nrm);
      if (nrm.Magnitude() > 1e-12) nrm.Normalize();
      out.push_back(kind);
      push3(out, p);
      push3(out, d);
      out.push_back(radius);
      push3(out, c.XYZ());
      push3(out, nrm.XYZ());
      out.push_back(props.Mass());
      push_bbox(out, f);
    }
    return out;
  })
}

rust::Vec<double> edge_info(const Shape &s) {
  FC_GUARD("edges", {
    rust::Vec<double> out;
    TopTools_IndexedMapOfShape edges, faces;
    TopExp::MapShapes(s, TopAbs_EDGE, edges);
    TopExp::MapShapes(s, TopAbs_FACE, faces);
    TopTools_IndexedDataMapOfShapeListOfShape anc;
    TopExp::MapShapesAndAncestors(s, TopAbs_EDGE, TopAbs_FACE, anc);
    for (int i = 1; i <= edges.Extent(); ++i) {
      TopoDS_Edge e = TopoDS::Edge(edges(i));
      double kind = 2, radius = 0, length = 0;
      gp_XYZ a(0, 0, 0), b(0, 0, 0), m(0, 0, 0), c(0, 0, 0), ax(0, 0, 0);
      if (!BRep_Tool::Degenerated(e)) {
        BRepAdaptor_Curve ca(e);
        double t0 = ca.FirstParameter(), t1 = ca.LastParameter();
        a = ca.Value(t0).XYZ();
        b = ca.Value(t1).XYZ();
        m = ca.Value(0.5 * (t0 + t1)).XYZ();
        if (ca.GetType() == GeomAbs_Line) {
          kind = 0;
          ax = ca.Line().Direction().XYZ();
        } else if (ca.GetType() == GeomAbs_Circle) {
          kind = 1;
          c = ca.Circle().Location().XYZ();
          ax = ca.Circle().Axis().Direction().XYZ();
          radius = ca.Circle().Radius();
        }
        GProp_GProps props;
        BRepGProp::LinearProperties(e, props);
        length = props.Mass();
      } else {
        kind = 3; // degenerated (sphere pole); skipped on the Rust side
      }
      out.push_back(kind);
      push3(out, a);
      push3(out, b);
      push3(out, m);
      out.push_back(length);
      push3(out, c);
      push3(out, ax);
      out.push_back(radius);
      std::vector<int> fidx;
      int ai = anc.FindIndex(e);
      if (ai > 0) {
        for (TopTools_ListOfShape::Iterator it(anc(ai)); it.More(); it.Next()) {
          int fi = faces.FindIndex(it.Value());
          bool seen = false;
          for (int x : fidx) seen = seen || x == fi;
          if (fi > 0 && !seen) fidx.push_back(fi);
        }
      }
      out.push_back(static_cast<double>(fidx.size()));
      for (int fi : fidx) out.push_back(static_cast<double>(fi - 1));
    }
    return out;
  })
}

rust::Vec<double> mass_props(const Shape &s) {
  FC_GUARD("mass properties", {
    GProp_GProps vp, sp;
    BRepGProp::VolumeProperties(s, vp);
    BRepGProp::SurfaceProperties(s, sp);
    rust::Vec<double> out;
    out.push_back(vp.Mass());
    out.push_back(sp.Mass());
    push3(out, (std::abs(vp.Mass()) > 1e-12 ? vp.CentreOfMass() : sp.CentreOfMass()).XYZ());
    push_bbox(out, s);
    return out;
  })
}

bool is_valid(const Shape &s) {
  try {
    BRepCheck_Analyzer an(s);
    return an.IsValid();
  } catch (...) {
    return false;
  }
}

void tessellate(const Shape &s, double deflection, MeshData &out) {
  FC_GUARD("tessellate", {
    BRepMesh_IncrementalMesh mesher(s, deflection, Standard_False, 0.35, Standard_True);
    TopTools_IndexedMapOfShape faces, edges;
    TopExp::MapShapes(s, TopAbs_FACE, faces);
    TopExp::MapShapes(s, TopAbs_EDGE, edges);
    for (int fi = 1; fi <= faces.Extent(); ++fi) {
      TopoDS_Face f = TopoDS::Face(faces(fi));
      TopLoc_Location loc;
      Handle(Poly_Triangulation) tri = BRep_Tool::Triangulation(f, loc);
      if (tri.IsNull()) continue;
      if (!tri->HasNormals()) BRepLib_ToolTriangulatedShape::ComputeNormals(f, tri);
      const gp_Trsf &t = loc.Transformation();
      bool rev = f.Orientation() == TopAbs_REVERSED;
      uint32_t base = static_cast<uint32_t>(out.positions.size() / 3);
      for (int n = 1; n <= tri->NbNodes(); ++n) {
        gp_Pnt p = tri->Node(n).Transformed(t);
        out.positions.push_back(static_cast<float>(p.X()));
        out.positions.push_back(static_cast<float>(p.Y()));
        out.positions.push_back(static_cast<float>(p.Z()));
        gp_Dir nd = tri->HasNormals() ? tri->Normal(n).Transformed(t) : gp_Dir(0, 0, 1);
        if (rev) nd.Reverse();
        out.normals.push_back(static_cast<float>(nd.X()));
        out.normals.push_back(static_cast<float>(nd.Y()));
        out.normals.push_back(static_cast<float>(nd.Z()));
      }
      for (int k = 1; k <= tri->NbTriangles(); ++k) {
        int a, b, c;
        tri->Triangle(k).Get(a, b, c);
        if (rev) std::swap(b, c);
        out.indices.push_back(base + static_cast<uint32_t>(a - 1));
        out.indices.push_back(base + static_cast<uint32_t>(b - 1));
        out.indices.push_back(base + static_cast<uint32_t>(c - 1));
        out.triangle_face.push_back(static_cast<uint32_t>(fi - 1));
      }
    }
    for (int ei = 1; ei <= edges.Extent(); ++ei) {
      TopoDS_Edge e = TopoDS::Edge(edges(ei));
      if (BRep_Tool::Degenerated(e)) continue;
      BRepAdaptor_Curve ca(e);
      GCPnts_TangentialDeflection pts(ca, 0.2, deflection);
      out.edge_ids.push_back(static_cast<uint32_t>(ei - 1));
      for (int k = 1; k <= pts.NbPoints(); ++k) {
        gp_Pnt p = pts.Value(k);
        out.edge_points.push_back(static_cast<float>(p.X()));
        out.edge_points.push_back(static_cast<float>(p.Y()));
        out.edge_points.push_back(static_cast<float>(p.Z()));
      }
      out.edge_offsets.push_back(static_cast<uint32_t>(out.edge_points.size() / 3));
    }
  })
}

// ---------------------------------------------------------------- exchange

void write_step(const Shape &s, rust::Str path) {
  FC_GUARD("STEP export", {
    quiet();
    STEPControl_Writer w;
    if (w.Transfer(s, STEPControl_AsIs) != IFSelect_RetDone) throw std::runtime_error("transfer failed");
    std::string p(path.data(), path.size());
    if (w.Write(p.c_str()) != IFSelect_RetDone) throw std::runtime_error("cannot write file");
  })
}

std::unique_ptr<Shape> read_step(rust::Str path) {
  FC_GUARD("STEP import", {
    quiet();
    STEPControl_Reader r;
    std::string p(path.data(), path.size());
    if (r.ReadFile(p.c_str()) != IFSelect_RetDone) throw std::runtime_error("cannot read file");
    if (r.TransferRoots() == 0) throw std::runtime_error("no shapes in file");
    return boxed(r.OneShape());
  })
}

rust::Vec<uint8_t> to_brep(const Shape &s) {
  FC_GUARD("serialize", {
    std::ostringstream os;
    BRepTools::Write(s, os);
    std::string str = os.str();
    rust::Vec<uint8_t> v;
    v.reserve(str.size());
    for (char ch : str) v.push_back(static_cast<uint8_t>(ch));
    return v;
  })
}

std::unique_ptr<Shape> from_brep(rust::Slice<const uint8_t> bytes) {
  FC_GUARD("deserialize", {
    std::string str(reinterpret_cast<const char *>(bytes.data()), bytes.size());
    std::istringstream is(str);
    TopoDS_Shape s;
    BRep_Builder b;
    BRepTools::Read(s, is, b);
    return boxed(s);
  })
}

} // namespace formcraft
