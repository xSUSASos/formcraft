//! Reading command parameters from JSON. Numbers may be given as JSON numbers or as expression
//! strings (`"L*2"`, `"25.4 mm"`); bodies by id or by name.

use formcraft_doc::{BodyId, BodyOp, Document, Expr, FeatureId, Vec3Expr};
use serde_json::Value;

use crate::{EngineError, Result};

fn bad(msg: String) -> EngineError {
    EngineError::InvalidParams(msg)
}

/// An expression from a number or a string.
pub fn expr_of(v: &Value, key: &str) -> Result<Expr> {
    match v {
        Value::Number(n) => n
            .as_f64()
            .filter(|f| f.is_finite())
            .map(Expr::num)
            .ok_or_else(|| bad(format!("`{key}` is not a finite number"))),
        Value::String(s) if !s.trim().is_empty() && s.len() <= formcraft_doc::expr::MAX_LEN => {
            Ok(Expr(s.trim().to_string()))
        }
        _ => Err(bad(format!("`{key}` must be a number or an expression string"))),
    }
}

pub fn opt_expr(p: &Value, key: &str) -> Result<Option<Expr>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => expr_of(v, key).map(Some),
    }
}

pub fn expr(p: &Value, key: &str) -> Result<Expr> {
    opt_expr(p, key)?.ok_or_else(|| bad(format!("missing `{key}`")))
}

pub fn vec3(p: &Value, key: &str, default: Option<[f64; 3]>) -> Result<Vec3Expr> {
    match p.get(key) {
        None | Some(Value::Null) => {
            default.map(|[x, y, z]| Vec3Expr::num(x, y, z)).ok_or_else(|| bad(format!("missing `{key}` ([x, y, z])")))
        }
        Some(Value::Array(a)) if a.len() == 3 => {
            let mut out = [Expr::num(0.0), Expr::num(0.0), Expr::num(0.0)];
            for (slot, v) in out.iter_mut().zip(a) {
                *slot = expr_of(v, key)?;
            }
            Ok(Vec3Expr(out))
        }
        _ => Err(bad(format!("`{key}` must be [x, y, z]"))),
    }
}

pub fn opt_str<'a>(p: &'a Value, key: &str) -> Option<&'a str> {
    p.get(key).and_then(Value::as_str)
}

pub fn str<'a>(p: &'a Value, key: &str) -> Result<&'a str> {
    opt_str(p, key).ok_or_else(|| bad(format!("missing `{key}` (string)")))
}

pub fn opt_bool(p: &Value, key: &str) -> Result<Option<bool>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        _ => Err(bad(format!("`{key}` must be true or false"))),
    }
}

/// A non-negative integer id; integral floats (`1.0`, as some clients send) are accepted.
pub fn as_id(v: &Value) -> Option<u32> {
    v.as_u64()
        .or_else(|| v.as_f64().filter(|f| f.fract() == 0.0 && *f >= 0.0 && *f <= u32::MAX as f64).map(|f| f as u64))
        .and_then(|i| u32::try_from(i).ok())
}

/// A body by numeric id or by name.
pub fn body_ref(d: &Document, v: &Value, key: &str) -> Result<BodyId> {
    match v {
        Value::Number(_) => {
            let id = as_id(v).ok_or_else(|| bad(format!("`{key}`: bad body id")))?;
            d.bodies
                .contains_key(&BodyId(id))
                .then_some(BodyId(id))
                .ok_or_else(|| bad(format!("`{key}`: no body with id {id}")))
        }
        Value::String(s) => d
            .bodies
            .iter()
            .find(|(_, st)| st.name == *s)
            .map(|(id, _)| *id)
            .ok_or_else(|| bad(format!("`{key}`: no body named `{s}`"))),
        _ => Err(bad(format!("`{key}` must be a body id or name"))),
    }
}

pub fn body(d: &Document, p: &Value, key: &str) -> Result<BodyId> {
    body_ref(d, p.get(key).ok_or_else(|| bad(format!("missing `{key}`")))?, key)
}

/// A feature by numeric id or by timeline name.
pub fn feature(d: &Document, p: &Value, key: &str) -> Result<FeatureId> {
    match p.get(key) {
        Some(v @ Value::Number(_)) => {
            let id = as_id(v).map(FeatureId).ok_or_else(|| bad(format!("`{key}`: bad id")))?;
            d.feature(id).map(|f| f.id).ok_or_else(|| bad(format!("`{key}`: no feature {}", id.0)))
        }
        Some(Value::String(s)) => d
            .features
            .iter()
            .find(|f| f.name == *s)
            // "2" means the id 2 (LLMs and scripts often quote numbers)
            .or_else(|| s.trim().parse::<u32>().ok().and_then(|n| d.feature(FeatureId(n))))
            .map(|f| f.id)
            .ok_or_else(|| bad(format!("`{key}`: no feature named `{s}`"))),
        _ => Err(bad(format!("missing `{key}` (feature id or name)"))),
    }
}

/// `operation` (new_body | join | cut | intersect) with `target` for the last three. Without a
/// `target` the latest body on the timeline is used (like Fusion picks the body you work on).
pub fn body_op(d: &Document, p: &Value) -> Result<BodyOp> {
    let op = opt_str(p, "operation").unwrap_or("new_body");
    if op == "new_body" {
        return Ok(BodyOp::NewBody);
    }
    let t = match p.get("target") {
        Some(_) => body(d, p, "target")?,
        None => d
            .active_features()
            .iter()
            .rev()
            .map(|f| BodyId(f.id.0))
            .find(|b| d.bodies.contains_key(b))
            .ok_or_else(|| bad(format!("`{op}` needs a body: there is none yet (use new_body)")))?,
    };
    match op {
        "join" => Ok(BodyOp::Join(t)),
        "cut" => Ok(BodyOp::Cut(t)),
        "intersect" => Ok(BodyOp::Intersect(t)),
        other => Err(bad(format!("unknown operation `{other}` (new_body, join, cut, intersect)"))),
    }
}

/// JSON-schema fragments shared by command definitions.
pub mod schema {
    use serde_json::{Value, json};

    pub fn num(desc: &str) -> Value {
        json!({"type": ["number", "string"], "description": desc})
    }
    pub fn vec3(desc: &str) -> Value {
        json!({"type": "array", "items": {"type": ["number", "string"]}, "minItems": 3, "maxItems": 3, "description": desc})
    }
    pub fn body(desc: &str) -> Value {
        json!({"type": ["integer", "string"], "description": desc})
    }
    pub fn operation() -> Value {
        json!({"type": "string", "enum": ["new_body", "join", "cut", "intersect"], "default": "new_body",
               "description": "What to do with the result: a new body, or join/cut/intersect with `target`."})
    }
}
