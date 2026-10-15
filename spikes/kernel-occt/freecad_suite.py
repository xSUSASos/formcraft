# Same test set as spikes/common/expect.rs, executed on OCCT through FreeCAD's Part module.
import json, math, os
import Part
from FreeCAD import Vector as V

out = os.environ.get("FC_OUT", "C:/fc-target/stepcheck/occt-fc")
os.makedirs(out, exist_ok=True)
r = 2.0; a = 20.0 - 2 * r
exp = {
    "box_hole": 60*40*10 - math.pi*25*10,
    "fillet_all": a**3 + 2*r*3*a*a + math.pi*r*r*3*a + 4/3*math.pi*r**3,
    "cyl_cut": 40*40*20 - math.pi*100*20,
    "revolve": math.pi*(400-100)*30,
    "shell": 40*30*20 - 36*26*18,
}
def build(name):
    if name == "box_hole":
        return Part.makeBox(60, 40, 10).cut(Part.makeCylinder(5, 12, V(30, 20, -1)))
    if name == "fillet_all":
        b = Part.makeBox(20, 20, 20); return b.makeFillet(2.0, b.Edges)
    if name == "cyl_cut":
        return Part.makeBox(40, 40, 20).cut(Part.makeCylinder(10, 22, V(20, 20, -1)))
    if name == "revolve":
        w = Part.makePolygon([V(10,0,0), V(20,0,0), V(20,0,30), V(10,0,30), V(10,0,0)])
        return Part.Face(w).revolve(V(0,0,0), V(0,0,1), 360)
    if name == "shell":
        b = Part.makeBox(40, 30, 20)
        top = [f for f in b.Faces if abs(f.CenterOfMass.z - 20) < 1e-6]
        return b.makeThickness(top, -2.0, 1e-3)
for name, v in exp.items():
    rec = {"name": name, "expected": round(v, 2)}
    try:
        s = build(name)
        p = f"{out}/{name}.step"; s.exportStep(p)
        back = Part.Shape(); back.read(p)
        bb = back.BoundBox
        rec.update(valid=s.isValid(), volume=round(s.Volume, 2), rt_volume=round(back.Volume, 2),
                   err_pct=round(abs(back.Volume - v) / v * 100, 4), faces=len(back.Faces),
                   bbox=[round(x, 3) for x in (bb.XMin, bb.YMin, bb.ZMin, bb.XMax, bb.YMax, bb.ZMax)])
    except Exception as e:
        rec["error"] = str(e)
    print("FCSUITE " + json.dumps(rec))
