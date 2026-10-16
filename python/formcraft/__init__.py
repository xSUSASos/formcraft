"""FormCraft scripting client (ADR 0003): drives a running FormCraft through its loopback
control channel. Standard library only.

    import formcraft
    fc = formcraft.connect()            # port from FORMCRAFT_PORT, default 7878
    fc.execute("solid.box", size=[60, 40, 5])
    print(fc.inspect()["bodies"][0]["volume"])
    fc.export("C:/out/part.stl")

Every FormCraft command is available as ``fc.execute(command_id, **params)``; ``fc.commands()``
lists them with their JSON schemas. Errors from the app raise :class:`FormCraftError`.
"""

from __future__ import annotations

import json
import os
import socket
from typing import Any

__all__ = ["Client", "FormCraftError", "connect"]
__version__ = "0.1.0"

DEFAULT_PORT = 7878


class FormCraftError(RuntimeError):
    """A command or protocol error reported by FormCraft."""


class Client:
    """One connection to a FormCraft control channel (127.0.0.1 only)."""

    def __init__(self, port: int | None = None, host: str = "127.0.0.1", timeout: float = 300.0):
        self.port = int(port if port is not None else os.environ.get("FORMCRAFT_PORT", DEFAULT_PORT))
        self._sock = socket.create_connection((host, self.port), timeout=timeout)
        self._file = self._sock.makefile("rwb")
        self._next = 1

    # -- protocol ------------------------------------------------------------------------
    def call(self, method: str, params: dict[str, Any] | None = None) -> Any:
        """Send ``{id, method, params}`` and return ``result`` (or raise on ``error``)."""
        rid = self._next
        self._next += 1
        line = json.dumps({"id": rid, "method": method, "params": params or {}}) + "\n"
        self._file.write(line.encode("utf-8"))
        self._file.flush()
        reply = self._file.readline()
        if not reply:
            raise FormCraftError("FormCraft closed the connection")
        msg = json.loads(reply)
        if msg.get("id") not in (rid, None):
            raise FormCraftError(f"reply for request {msg.get('id')}, expected {rid}")
        if "error" in msg:
            raise FormCraftError(str(msg["error"]))
        return msg.get("result")

    # -- commands ------------------------------------------------------------------------
    def execute(self, command: str, **params: Any) -> Any:
        """Run a command, e.g. ``execute("solid.fillet", edges="vertical", radius=3)``."""
        return self.call("engine.execute", {"command": command, "params": params})

    def commands(self) -> list[dict[str, Any]]:
        return self.call("engine.commands")

    def inspect(self) -> dict[str, Any]:
        """Bodies (volume, bbox, faces…), features with status, parameters."""
        return self.call("model.inspect")

    def bodies(self) -> list[dict[str, Any]]:
        return self.inspect().get("bodies", [])

    def parameter(self, name: str, expression: float | str, comment: str | None = None) -> Any:
        p: dict[str, Any] = {"name": name, "expression": expression}
        if comment:
            p["comment"] = comment
        return self.execute("doc.parameter", **p)

    def new(self, name: str | None = None) -> Any:
        return self.execute("doc.new", **({"name": name} if name else {}))

    def open(self, path: str) -> Any:
        return self.execute("doc.open", path=path)

    def save(self, path: str) -> Any:
        return self.execute("doc.save", path=path)

    def export(self, path: str, bodies: list[int | str] | None = None) -> Any:
        """STEP or STL by extension; all visible bodies unless ``bodies`` (ids or names) is given."""
        p: dict[str, Any] = {"path": path}
        if bodies is not None:
            p["bodies"] = list(bodies)
        return self.execute("doc.export", **p)

    def render(self, path: str, view: str = "iso", width: int = 1024, height: int = 768) -> Any:
        """Save a PNG of the model (offscreen render)."""
        return self.call("ui.render", {"path": path, "view": view, "width": width, "height": height})

    def close(self) -> None:
        try:
            self._file.close()
        finally:
            self._sock.close()

    def __enter__(self) -> "Client":
        return self

    def __exit__(self, *exc: object) -> None:
        self.close()


def connect(port: int | None = None, host: str = "127.0.0.1", timeout: float = 300.0) -> Client:
    """Connect to ``formcraft --control PORT`` (or ``formcraft-cli script``)."""
    return Client(port, host, timeout)
