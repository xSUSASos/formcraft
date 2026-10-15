# Load STEP files with FreeCAD and print volume, bbox, face count as JSON lines.
# Usage: FC_FILES="a.step;b.step" FreeCADCmd tools/freecad_check.py
import json
import os

import Part  # noqa: F401  (FreeCAD module)

for path in filter(None, os.environ.get("FC_FILES", "").split(";")):
    rec = {"file": os.path.basename(path)}
    try:
        shape = Part.Shape()
        shape.read(path)
        bb = shape.BoundBox
        rec.update(
            ok=shape.isValid(),
            solids=len(shape.Solids),
            faces=len(shape.Faces),
            volume=round(shape.Volume, 3),
            bbox=[round(v, 3) for v in (bb.XMin, bb.YMin, bb.ZMin, bb.XMax, bb.YMax, bb.ZMax)],
        )
    except Exception as e:  # report, never crash the batch
        rec.update(ok=False, error=str(e))
    print("FCCHECK " + json.dumps(rec))
