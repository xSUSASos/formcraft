//! MCP tool definitions and dispatch.

use serde_json::{Value, json};

use crate::backend::Backend;

pub fn definitions(has_ui: bool) -> Value {
    let mut tools = vec![
        json!({"name": "execute",
            "description": "Run a FormCraft command with JSON parameters (no dialogs). Every modelling step is a command and becomes a feature on the parametric timeline. Examples: doc.parameter {name: \"L\", expression: 60}; sketch.create {plane: \"XY\", rectangles: [{corner: [0,0], size: [\"L\", 40]}]}; solid.extrude {distance: 5}; solid.hole {points: [[10,10,5]], diameter: 6.6}; solid.fillet {edges: \"vertical\", radius: 3}; feature.edit {feature: \"Extrude1\", params: {distance: 8}}; doc.undo; doc.export {path}. Use list_commands for schemas.",
            "inputSchema": {"type": "object", "properties": {"command": {"type": "string"}, "params": {"type": "object"}}, "required": ["command"]}}),
        json!({"name": "list_commands",
            "description": "Commands with ids, labels, descriptions and JSON parameter schemas. Optional filter: a substring of the id or label.",
            "inputSchema": {"type": "object", "properties": {"filter": {"type": "string"}}}}),
        json!({"name": "inspect_model",
            "description": "The current model: bodies with exact volume, surface area, centroid, bounding box, face/edge counts, cylindrical face count and validity; timeline features with status (ok/error/suppressed/rolled_back); parameters with values.",
            "inputSchema": {"type": "object", "properties": {}}}),
        json!({"name": "render",
            "description": "Render the visible bodies to a PNG image (shaded with edges, fitted to the model). view: iso (default), top, bottom, front, back, left, right.",
            "inputSchema": {"type": "object", "properties": {
                "view": {"type": "string"}, "width": {"type": "integer", "minimum": 16}, "height": {"type": "integer", "minimum": 16},
                "edges": {"type": "boolean"}, "grid": {"type": "boolean"}, "path": {"type": "string", "description": "Also save the PNG here."}}}}),
        json!({"name": "list_selectors",
            "description": "How to refer to faces and edges in commands (fillet edges, shell faces, hole face, sketch on a face): names, near-points, edges_of.",
            "inputSchema": {"type": "object", "properties": {}}}),
        json!({"name": "run_script",
            "description": "Run several commands, one per line: `command.id {json params}` (# comments). Stops at the first error and reports it.",
            "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]}}),
        json!({"name": "new_design",
            "description": "Start an empty design (discards the current one).",
            "inputSchema": {"type": "object", "properties": {"name": {"type": "string"}}}}),
    ];
    if has_ui {
        tools.push(json!({"name": "screenshot", "description": "Screenshot of the FormCraft window (PNG).",
            "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}}}}));
        tools.push(json!({"name": "ui_click", "description": "Click in the canvas at screen pixels (space: \"screen\", default) or at a world point (space: \"world\", z = 0 plane by default). button: left (default) or right.",
            "inputSchema": {"type": "object", "properties": {"x": {"type": "number"}, "y": {"type": "number"}, "z": {"type": "number"}, "space": {"type": "string"}, "button": {"type": "string"}}, "required": ["x", "y"]}}));
    }
    json!({"tools": tools})
}

fn text(v: &Value) -> Value {
    json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(v).unwrap_or_default()}]})
}

fn err_text(e: &str) -> Value {
    json!({"content": [{"type": "text", "text": e}], "isError": true})
}

fn image(v: &Value) -> Value {
    match v.get("pngBase64").and_then(Value::as_str) {
        Some(data) => {
            let mut meta = v.clone();
            if let Some(o) = meta.as_object_mut() {
                o.remove("pngBase64");
            }
            json!({"content": [{"type": "image", "data": data, "mimeType": "image/png"}, {"type": "text", "text": meta.to_string()}]})
        }
        None => text(v),
    }
}

/// Run a tool. `Err` = protocol error (unknown tool, bad arguments); a failing tool is an
/// `isError` result so the model sees the message.
pub fn call(b: &mut dyn Backend, name: &str, args: Value) -> Result<Value, String> {
    let a = match args {
        Value::Null => json!({}),
        v @ Value::Object(_) => v,
        _ => return Err(format!("`arguments` of `{name}` must be an object")),
    };
    let r = match name {
        "execute" => {
            let command = a.get("command").and_then(Value::as_str).ok_or("`execute` needs `command`")?.to_string();
            b.call(
                "engine.execute",
                json!({"command": command, "params": a.get("params").cloned().unwrap_or(json!({}))}),
            )
        }
        "list_commands" => b.call("engine.commands", json!({})).map(|v| {
            let f = a.get("filter").and_then(Value::as_str).map(str::to_lowercase);
            match (v, f) {
                (Value::Array(items), Some(f)) => Value::Array(
                    items
                        .into_iter()
                        .filter(|c| {
                            c["id"].as_str().is_some_and(|x| x.contains(&f))
                                || c["label"].as_str().is_some_and(|x| x.to_lowercase().contains(&f))
                        })
                        .collect(),
                ),
                (v, _) => v,
            }
        }),
        "inspect_model" => b.call("model.inspect", json!({})),
        "list_selectors" => b.call("selectors.list", json!({})),
        "new_design" => b.call("engine.execute", json!({"command": "doc.new", "params": a})),
        "run_script" => {
            let text = a.get("text").and_then(Value::as_str).ok_or("`run_script` needs `text`")?;
            let steps = formcraft_engine::script::parse(text);
            match steps {
                Err(e) => Err(e),
                Ok(steps) => {
                    let mut out = Vec::new();
                    let mut failed = None;
                    for (i, (cmd, p)) in steps.iter().enumerate() {
                        match b.call("engine.execute", json!({"command": cmd, "params": p})) {
                            Ok(v) => out.push(json!({"step": i + 1, "command": cmd, "message": v.get("message").cloned().unwrap_or(json!("ok"))})),
                            Err(e) => {
                                failed = Some(format!("step {} ({cmd}): {e}", i + 1));
                                break;
                            }
                        }
                    }
                    match failed {
                        Some(e) => Err(format!("{e}\ncompleted before the error: {}", Value::Array(out))),
                        None => Ok(json!({"steps": out})),
                    }
                }
            }
        }
        "render" => return Ok(b.call("ui.render", a).map(|v| image(&v)).unwrap_or_else(|e| err_text(&e))),
        "screenshot" if b.has_ui() => {
            return Ok(b.call("ui.screenshot", a).map(|v| image(&v)).unwrap_or_else(|e| err_text(&e)));
        }
        "ui_click" if b.has_ui() => b.call("ui.click", a),
        other => return Err(format!("unknown tool `{other}`")),
    };
    Ok(match r {
        Ok(v) => text(&v),
        Err(e) => err_text(&e),
    })
}
