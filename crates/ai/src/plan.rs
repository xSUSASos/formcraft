//! What the LLM returns: a plan of named parameters and command steps, no code.
//!
//! ```json
//! {"summary": "Flange D100 with 4 holes",
//!  "parameters": [{"name": "flange_d", "expression": 100, "comment": "outer diameter"}],
//!  "steps": [{"command": "solid.cylinder", "params": {"diameter": "flange_d", "height": 10}}]}
//! ```

use formcraft_engine::doc::{FeatureId, valid_name};
use formcraft_engine::regen::Status;
use formcraft_engine::{EngineError, Registry, Session};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const MAX_STEPS: usize = 60;
pub const MAX_PARAMETERS: usize = 50;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanParam {
    pub name: String,
    pub expression: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub command: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Plan {
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub parameters: Vec<PlanParam>,
    #[serde(default)]
    pub steps: Vec<Step>,
}

/// Where a plan went wrong (fed back to the LLM).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlanError {
    /// 1-based step, 0 = the plan as a whole or its parameters.
    pub step: usize,
    pub command: String,
    pub message: String,
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.step == 0 {
            write!(f, "{}", self.message)
        } else {
            write!(f, "step {} ({}): {}", self.step, self.command, self.message)
        }
    }
}

fn whole(message: impl Into<String>) -> PlanError {
    PlanError { step: 0, command: String::new(), message: message.into() }
}

/// The JSON object in an LLM reply: tolerates `<think>` blocks, Markdown fences and text
/// around the object. A bare array is taken as the steps.
pub fn parse(text: &str) -> Result<Plan, PlanError> {
    let mut t = text.trim();
    if let Some(end) = t.find("</think>") {
        t = t.get(end + "</think>".len()..).unwrap_or("").trim();
    }
    let mut candidates = [t.find('{').zip(t.rfind('}')), t.find('[').zip(t.rfind(']'))];
    // whichever bracket opens first is the outer value
    candidates.sort_by_key(|c| c.map_or(usize::MAX, |(a, _)| a));
    let mut last_err = String::from("the reply contains no JSON object");
    for (a, b) in candidates.into_iter().flatten() {
        let Some(slice) = t.get(a..=b) else { continue };
        match serde_json::from_str::<Value>(slice) {
            Ok(Value::Array(steps)) => {
                return serde_json::from_value(json!({"steps": steps})).map_err(|e| whole(format!("bad steps: {e}")));
            }
            Ok(v @ Value::Object(_)) => {
                return serde_json::from_value(v)
                    .map_err(|e| whole(format!("the plan does not match the format: {e}")));
            }
            Ok(_) => {}
            Err(e) => {
                // a valid value followed by junk (`}}`, a comment): take the first value
                let first =
                    t.get(a..).and_then(|rest| serde_json::Deserializer::from_str(rest).into_iter::<Value>().next());
                if let Some(Ok(v @ Value::Object(_))) = first {
                    return serde_json::from_value(v)
                        .map_err(|e| whole(format!("the plan does not match the format: {e}")));
                }
                last_err = format!("invalid JSON: {e}");
            }
        }
    }
    Err(whole(last_err))
}

/// A reply cut off by the token limit: keep the complete part. Cuts the text after the last
/// closed `}` where closing the still-open brackets gives a plan with steps (e.g. a repetition
/// loop after correct steps). `None` when nothing usable is left.
pub fn salvage(text: &str) -> Option<Plan> {
    let start = text.find('{')?;
    let body = text.get(start..)?;
    let mut stack: Vec<char> = Vec::new();
    let (mut in_str, mut esc) = (false, false);
    let mut best = None;
    let mut tries = 0;
    for (i, c) in body.char_indices() {
        if in_str {
            match (esc, c) {
                (true, _) => esc = false,
                (false, '\\') => esc = true,
                (false, '"') => in_str = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' => stack.push('}'),
            '[' => stack.push(']'),
            '}' | ']' => {
                if stack.pop() != Some(c) {
                    break;
                }
                // a complete element inside `steps` (depth: object → steps array → element)
                if c == '}' && stack.len() == 2 && tries < 400 {
                    tries += 1;
                    let mut candidate = body.get(..=i)?.to_string();
                    candidate.extend(stack.iter().rev());
                    if let Ok(p) = serde_json::from_str::<Plan>(&candidate)
                        && !p.steps.is_empty()
                    {
                        best = Some(p);
                    }
                }
            }
            _ => {}
        }
    }
    best.map(|mut p: Plan| {
        // a loop shows up as the same step again (same command and scalar values, e.g. another
        // hole of the same diameter): keep the plan up to the first repeat
        let scalars = |s: &Step| -> Vec<(String, Value)> {
            s.params
                .as_object()
                .map(|o| o.iter().filter(|(_, v)| !v.is_array() && !v.is_object()).map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default()
        };
        if let Some(i) = (1..p.steps.len()).find(|&i| {
            matches!((p.steps.get(i - 1), p.steps.get(i)), (Some(a), Some(b)) if a.command == b.command && scalars(a) == scalars(b))
        }) {
            p.steps.truncate(i);
        }
        p
    })
}

/// Static checks before anything runs: known AI commands, object params, sane sizes and names.
pub fn validate(plan: &Plan, registry: &Registry) -> Result<(), PlanError> {
    if plan.steps.is_empty() {
        return Err(whole("the plan has no steps"));
    }
    if plan.steps.len() > MAX_STEPS {
        return Err(whole(format!("too many steps ({} > {MAX_STEPS})", plan.steps.len())));
    }
    if plan.parameters.len() > MAX_PARAMETERS {
        return Err(whole(format!("too many parameters ({} > {MAX_PARAMETERS})", plan.parameters.len())));
    }
    for p in &plan.parameters {
        if !valid_name(&p.name) {
            return Err(whole(format!(
                "parameter name `{}` is not valid (letters, digits, _; not a unit name)",
                p.name
            )));
        }
        if !(p.expression.is_number() || p.expression.is_string()) {
            return Err(whole(format!("parameter `{}`: expression must be a number or a string", p.name)));
        }
    }
    for (i, s) in plan.steps.iter().enumerate() {
        let err = |m: String| PlanError { step: i + 1, command: s.command.clone(), message: m };
        match registry.get(&s.command) {
            None => return Err(err(format!("unknown command `{}`", s.command))),
            Some(c) if !c.spec.ai => return Err(err(format!("`{}` is not available to the assistant", s.command))),
            Some(_) => {}
        }
        if !(s.params.is_object() || s.params.is_null()) {
            return Err(err("params must be a JSON object".into()));
        }
    }
    Ok(())
}

/// Run a plan as one undo step (features grouped under the summary). Fails with the first
/// failing step, or with the first feature that regenerates with an error; nothing is kept then.
pub fn apply(s: &mut Session, plan: &Plan, label: &str) -> Result<Vec<FeatureId>, PlanError> {
    validate(plan, s.registry())?;
    let mut failed: Option<PlanError> = None;
    let group = if plan.summary.trim().is_empty() { label } else { plan.summary.trim() };
    let r = s.transaction(label, Some(group), |s| {
        let before: Vec<FeatureId> = s.document().features.iter().map(|f| f.id).collect();
        for p in &plan.parameters {
            let mut v = json!({"name": p.name, "expression": p.expression});
            if let Some(c) = &p.comment {
                v["comment"] = json!(c);
            }
            if let Err(e) = s.execute("doc.parameter", &v) {
                failed = Some(whole(format!("parameter `{}`: {e}", p.name)));
                return Err(e);
            }
        }
        for (i, st) in plan.steps.iter().enumerate() {
            if let Err(e) = s.execute(&st.command, &st.params) {
                failed = Some(PlanError { step: i + 1, command: st.command.clone(), message: e.to_string() });
                return Err(e);
            }
        }
        let bad = s.model().status.iter().filter(|f| !before.contains(&f.id)).find_map(|f| match &f.status {
            Status::Error(e) => Some(format!("feature {} failed to rebuild: {e}", f.name)),
            _ => None,
        });
        if let Some(m) = bad {
            failed = Some(whole(m.clone()));
            return Err(EngineError::Failed(m));
        }
        Ok(())
    });
    match r {
        Ok(((), added)) => Ok(added),
        Err(e) => Err(failed.unwrap_or_else(|| whole(e.to_string()))),
    }
}
