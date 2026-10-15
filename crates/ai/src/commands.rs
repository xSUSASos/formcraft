//! `ai.generate`, `ai.explain`, `ai.status` — registered into a session's command registry,
//! so the UI, S-box, MCP and CLI reach the assistant the same way as any other command.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use formcraft_engine::doc::FeatureId;
use formcraft_engine::regen::Status;
use formcraft_engine::{CommandSpec, EngineError, Session, Source};
use serde_json::{Value, json};

use crate::pipeline::{AiRequest, Generator};
use crate::plan;

pub fn register(s: &mut Session, generator: Arc<Generator>) -> Result<(), EngineError> {
    let g = generator.clone();
    s.registry_mut().register(
        CommandSpec::new(
            "ai.generate",
            "Generate",
            "Describe a part in words; the assistant builds it as a group of features (one undo step).",
        )
        .params(json!({"type": "object", "required": ["prompt"], "properties": {
            "prompt": {"type": "string", "description": "What to build or change, e.g. \"flange D100, 4 holes D9 on D80, 10 thick\"."},
            "use_selection": {"type": "boolean", "default": false, "description": "Refer to the current selection (Edit with AI)."},
            "apply": {"type": "boolean", "default": true, "description": "false: only return the plan."}
        }}))
        .place("Solid", "Create", true)
        .icon("ai")
        .undoable()
        .source(Source::Builtin),
        Arc::new(move |s, p| generate(&g, s, p)),
    )?;
    s.registry_mut().register(
        CommandSpec::new("ai.explain", "Explain", "Explain a timeline feature and its parameters in plain words.")
            .params(json!({"type": "object", "required": ["feature"], "properties": {
                "feature": {"type": ["integer", "string"], "description": "Feature id or name."}
            }}))
            .source(Source::Builtin),
        Arc::new(explain),
    )?;
    let g = generator;
    s.registry_mut().register(
        CommandSpec::new("ai.status", "Assistant Status", "LLM backend, model and whether the server is reachable.")
            .source(Source::Builtin),
        Arc::new(move |_, _| {
            Ok(match g.backend.status() {
                Ok(v) => {
                    json!({"ok": true, "status": v, "message": format!("{} · {}", g.backend.kind(), g.backend.model())})
                }
                Err(e) => json!({"ok": false, "error": e, "message": format!("Assistant offline: {e}")}),
            })
        }),
    )?;
    Ok(())
}

fn generate(g: &Generator, s: &mut Session, p: &Value) -> Result<Value, EngineError> {
    let prompt = p.get("prompt").and_then(Value::as_str).unwrap_or("").trim().to_string();
    if prompt.is_empty() {
        return Err(EngineError::InvalidParams("`prompt` is empty".into()));
    }
    let req = AiRequest { prompt, use_selection: p.get("use_selection").and_then(Value::as_bool).unwrap_or(false) };
    let cancel = AtomicBool::new(false);
    let proposal = g.generate(s, &req, &mut |_| {}, &cancel).map_err(|e| EngineError::Failed(e.to_string()))?;
    let summary = proposal.plan.summary.trim().to_string();
    let base = json!({
        "summary": summary, "plan": proposal.plan, "attempts": proposal.attempts.len(),
        "seconds": proposal.elapsed.as_secs_f64(),
    });
    if p.get("apply").and_then(Value::as_bool) == Some(false) {
        let mut v = base;
        v["message"] = json!(format!("Plan: {summary}"));
        return Ok(v);
    }
    let added =
        plan::apply(s, &proposal.plan, &format!("AI: {summary}")).map_err(|e| EngineError::Failed(e.to_string()))?;
    let mut v = base;
    v["features"] = json!(added.iter().map(|f| f.0).collect::<Vec<_>>());
    v["message"] = json!(format!(
        "AI: {summary} — {} feature(s), {:.0} s{}",
        added.len(),
        proposal.elapsed.as_secs_f64(),
        if proposal.attempts.len() > 1 { format!(", {} attempts", proposal.attempts.len()) } else { String::new() }
    ));
    Ok(v)
}

fn what(kind: &str) -> &'static str {
    match kind {
        "box" => "an axis-aligned box from a corner and a size",
        "cylinder" => "a cylinder from a base centre, an axis, a radius and a height",
        "sphere" => "a sphere from a centre and a radius",
        "sketch" => "a 2D sketch: curves and dimensions on a plane, the base for extrudes and revolves",
        "extrude" => "pulls the closed profiles of a sketch into a solid along the sketch normal",
        "revolve" => "spins the profiles of a sketch around an axis into a solid",
        "fillet" => "rounds the selected edges with a radius",
        "chamfer" => "bevels the selected edges by a distance",
        "shell" => "hollows the body, removing the selected faces and leaving walls of a thickness",
        "hole" => "drills holes into a face at points (through or to a depth, plain, counterbored or countersunk)",
        "press_pull" => "moves faces of the body by a distance (offset)",
        "combine" => "joins, cuts or intersects bodies",
        _ => "a feature",
    }
}

fn value_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(a) => format!("[{}]", a.iter().map(value_text).collect::<Vec<_>>().join(", ")),
        Value::Object(o) if o.is_empty() => "—".into(),
        other => other.to_string(),
    }
}

/// A plain-words description built from the feature's own data (fast, offline, exact).
fn explain(s: &mut Session, p: &Value) -> Result<Value, EngineError> {
    let d = s.document();
    let f = match p.get("feature") {
        Some(Value::String(name)) => d.features.iter().find(|f| &f.name == name),
        Some(v) => formcraft_engine::params::as_id(v).and_then(|id| d.feature(FeatureId(id))),
        None => None,
    }
    .ok_or_else(|| EngineError::InvalidParams("no such feature".into()))?;
    let data = serde_json::to_value(&f.kind).unwrap_or(Value::Null);
    let kind = data["type"].as_str().unwrap_or("feature").to_string();
    let mut lines = vec![format!("{} — {}: {}.", f.name, f.kind.base_name(), what(&kind))];
    if let Some(o) = data.as_object() {
        for (k, v) in o.iter().filter(|(k, _)| k.as_str() != "type") {
            lines.push(format!("  {k}: {}", value_text(v)));
        }
    }
    if let Some(name) = f.group.and_then(|g| d.groups.get(&g)) {
        lines.push(format!("Part of the group \"{name}\"."));
    }
    let status = s.model().status.iter().find(|x| x.id == f.id).map(|x| &x.status);
    lines.push(match status {
        Some(Status::Error(e)) => format!("Status: error — {e}"),
        Some(Status::Suppressed) => "Status: suppressed (skipped when rebuilding).".into(),
        Some(Status::RolledBack) => "Status: after the timeline marker (not computed).".into(),
        _ => "Status: rebuilt without errors.".into(),
    });
    let text = lines.join("\n");
    Ok(json!({"feature": f.id.0, "text": text, "message": lines.first().cloned().unwrap_or_default()}))
}
