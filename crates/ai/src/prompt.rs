//! The system prompt and the output schema, generated from the command registry (only commands
//! marked `ai`), so the assistant always sees exactly the commands and parameters that exist —
//! including commands registered at runtime by plugins.

use formcraft_engine::doc::select::{EDGE_NAMES, FACE_NAMES};
use formcraft_engine::{CommandSpec, Registry};
use serde_json::{Value, json};

const RULES: &str = "\
You are the modelling assistant of FormCraft, a parametric 3D CAD. You turn a request into a plan: \
named parameters plus a list of commands. The commands become features on the timeline.

Rules:
- Units are millimetres and degrees. Z is up; the XY plane is the ground; parts usually sit on z = 0.
- Answer with ONE JSON object only: {\"summary\": ..., \"parameters\": [...], \"steps\": [...]}. No prose, no code.
- summary: a short name of the result, e.g. \"Flange D100, 4 x D9 holes\".
- parameters: every main dimension from the request as a parameter with a clear snake_case name \
(flange_d, plate_length, hole_d) and a numeric value. Steps refer to them in expressions: \
\"flange_d / 2\", \"plate_length - 2 * margin\".
- steps: {\"command\": id, \"params\": {...}} using only the commands below and only their parameters.
- Numbers may be expressions with parameters (strings), e.g. \"hole_d\" or \"L/2\".
- Through holes in a plate: solid.hole with points on the top face ([x, y, top_z]) and a diameter.
- Holes on a bolt circle: compute every point explicitly (x = r*cos(a), y = r*sin(a)); write numbers or expressions with cos/sin in degrees.
- Faces and edges are chosen with selectors (see below), never by index.
- Keep it simple: the fewest features that give the requested shape.";

/// Compact one-line-per-property rendering of a JSON schema (small models read this better
/// than raw JSON schema and it is ~3x shorter).
fn render_props(schema: &Value, indent: usize, out: &mut String) {
    let req: Vec<&str> = schema["required"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let Some(props) = schema["properties"].as_object() else { return };
    for (name, p) in props {
        let pad = " ".repeat(indent);
        let ty = type_name(p);
        let mark = if req.contains(&name.as_str()) { " (required)" } else { "" };
        let desc = p["description"].as_str().unwrap_or("");
        out.push_str(&format!("{pad}- {name}: {ty}{mark}"));
        if !desc.is_empty() {
            out.push_str(&format!(" — {desc}"));
        }
        out.push('\n');
        let items = &p["items"];
        if items["properties"].is_object() {
            render_props(items, indent + 2, out);
        } else if p["properties"].is_object() {
            render_props(p, indent + 2, out);
        }
    }
}

fn type_name(p: &Value) -> String {
    if let Some(e) = p["enum"].as_array() {
        let v: Vec<String> = e.iter().map(|x| x.to_string()).collect();
        return format!("one of {}", v.join("|"));
    }
    let t = match &p["type"] {
        Value::String(s) => s.clone(),
        Value::Array(a) => {
            let v: Vec<&str> = a.iter().filter_map(Value::as_str).collect();
            if v == ["number", "string"] { "number|expr".into() } else { v.join("|") }
        }
        _ => "any".into(),
    };
    if t == "array" {
        let it = &p["items"];
        if it["properties"].is_object() {
            return "array of objects:".into();
        }
        let n = p["minItems"].as_u64().filter(|n| Some(*n) == p["maxItems"].as_u64());
        let inner = type_name(it);
        return match n {
            Some(n) => format!("[{inner} x{n}]"),
            None => format!("array of {inner}"),
        };
    }
    t
}

pub fn ai_commands(r: &Registry) -> Vec<&CommandSpec> {
    r.specs().filter(|s| s.ai).collect()
}

pub fn catalog(r: &Registry) -> String {
    let mut out = String::new();
    for spec in ai_commands(r) {
        out.push_str(&format!("\n{} — {}\n", spec.id, spec.description));
        render_props(&spec.params, 2, &mut out);
    }
    out
}

pub fn selectors_help() -> String {
    format!(
        "Selectors (for edges, faces):\n- face names: {}\n- edge names: {}\n- {{\"near\": [x, y, z]}}: the face/edge closest to a point\n- {{\"edges_of\": <face selector>}}: edges around faces\n- an array of selectors means all of them",
        FACE_NAMES.join(", "),
        EDGE_NAMES.join(", ")
    )
}

pub fn system_prompt(r: &Registry) -> String {
    format!("{RULES}\n\nCommands:{}\n{}", catalog(r), selectors_help())
}

/// JSON schema of a plan (for structured output). Command ids are an enum of the AI commands.
pub fn output_schema(r: &Registry) -> Value {
    let ids: Vec<&str> = ai_commands(r).iter().map(|s| s.id.as_str()).collect();
    json!({
        "type": "object",
        "required": ["summary", "parameters", "steps"],
        "properties": {
            "summary": {"type": "string"},
            "parameters": {"type": "array", "items": {
                "type": "object", "required": ["name", "expression"],
                "properties": {"name": {"type": "string"}, "expression": {"type": ["number", "string"]}, "comment": {"type": "string"}}
            }},
            "steps": {"type": "array", "minItems": 1, "items": {
                "type": "object", "required": ["command", "params"],
                "properties": {"command": {"type": "string", "enum": ids}, "params": {"type": "object"}}
            }}
        }
    })
}
