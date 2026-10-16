"""Batch STL export (FormCraft example script): every visible body of the open design goes to
its own STL file.

Run it against a running FormCraft:

    formcraft --control 7878
    python examples/scripts/export_stl.py C:/out            # FORMCRAFT_PORT=7878 by default

or from Tools › Scripts and Add-Ins (the app passes FORMCRAFT_PORT itself).
"""

import os
import sys

try:
    import formcraft
except ImportError:  # running from the source tree
    sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "..", "python"))
    import formcraft


def main() -> int:
    out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.getcwd(), "stl")
    os.makedirs(out, exist_ok=True)
    with formcraft.connect() as fc:
        bodies = [b for b in fc.bodies() if b.get("visible", True)]
        if not bodies:
            print("no bodies to export")
            return 1
        for b in bodies:
            path = os.path.join(out, f"{b['name']}.stl")
            fc.export(path, bodies=[b["id"]])
            print(f"{b['name']}: {b['volume']:.2f} mm³ -> {path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
