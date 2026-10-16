//! ai-bench (PLAN Phase 5): prompts with reference results → % valid / rebuilt / matching.
//!
//! A case: `{"id", "prompt", "level": "simple"|"medium", "expect": {"size": [x, y, z],
//! "volume": v, "cylindrical_faces": n?}, "reference": <plan>}`. The reference plan proves the
//! expectations are consistent (a test builds every reference and checks it matches).
//!
//! - **valid**: the assistant produced a plan that applied and left valid bodies;
//! - **rebuilt**: after scaling every plan parameter by 1.1 the model still rebuilds without
//!   errors (the plan is really parametric, not a frozen shape);
//! - **matched**: overall size within max(0.5 mm, 2%) per axis (axes compared sorted, the part
//!   may be oriented differently), volume within 3%, and the cylindrical face count if given.

use std::sync::atomic::AtomicBool;
use std::time::Instant;

use formcraft_engine::Session;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::pipeline::{AiRequest, Generator};
use crate::plan::{self, Plan};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Expect {
    pub size: [f64; 3],
    pub volume: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cylindrical_faces: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Case {
    pub id: String,
    pub prompt: String,
    #[serde(default = "simple")]
    pub level: String,
    pub expect: Expect,
    pub reference: Plan,
}

fn simple() -> String {
    "simple".into()
}

pub const CASES: &str = include_str!("../bench/cases.json");

pub fn cases() -> Result<Vec<Case>, String> {
    serde_json::from_str(CASES).map_err(|e| format!("bench/cases.json: {e}"))
}

/// What the finished model looks like (all visible bodies together).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Measured {
    pub size: [f64; 3],
    pub volume: f64,
    pub cylindrical_faces: u64,
    pub bodies: usize,
    pub valid: bool,
}

pub fn measure(s: &mut Session) -> Result<Measured, String> {
    let v = s.execute("model.inspect", &json!({})).map_err(|e| e.to_string())?;
    let bodies = v["bodies"].as_array().cloned().unwrap_or_default();
    if bodies.is_empty() {
        return Err("no body".into());
    }
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    let (mut volume, mut cyl, mut valid) = (0.0, 0, true);
    for b in &bodies {
        for i in 0..3 {
            lo[i] = lo[i].min(b["bbox"]["min"][i].as_f64().unwrap_or(f64::NAN));
            hi[i] = hi[i].max(b["bbox"]["max"][i].as_f64().unwrap_or(f64::NAN));
        }
        volume += b["volume"].as_f64().unwrap_or(f64::NAN);
        cyl += b["cylindrical_faces"].as_u64().unwrap_or(0);
        valid &= b["valid"].as_bool().unwrap_or(false);
    }
    let errors = v["errors"].as_u64().unwrap_or(0);
    Ok(Measured {
        size: [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]],
        volume,
        cylindrical_faces: cyl,
        bodies: bodies.len(),
        valid: valid && errors == 0 && volume.is_finite() && volume > 0.0,
    })
}

/// Why a measured model does not match (empty: it matches).
pub fn mismatch(m: &Measured, e: &Expect) -> Vec<String> {
    let mut out = Vec::new();
    let mut a = m.size;
    let mut b = e.size;
    a.sort_by(f64::total_cmp);
    b.sort_by(f64::total_cmp);
    for i in 0..3 {
        if (a[i] - b[i]).abs() > (0.02 * b[i]).max(0.5) {
            out.push(format!("size {:?} ≠ {:?}", m.size.map(round), e.size));
            break;
        }
    }
    if (m.volume - e.volume).abs() > 0.03 * e.volume.abs() {
        out.push(format!("volume {:.1} ≠ {:.1}", m.volume, e.volume));
    }
    if let Some(c) = e.cylindrical_faces
        && c != m.cylindrical_faces
    {
        out.push(format!("cylindrical faces {} ≠ {c}", m.cylindrical_faces));
    }
    out
}

fn round(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Scale every numeric parameter by `k` and check the model still rebuilds (and changes).
pub fn rebuilds(s: &mut Session, plan: &Plan, k: f64) -> Result<(), String> {
    let before = measure(s)?;
    let mut changed = 0;
    for p in &plan.parameters {
        if let Some(v) = p.expression.as_f64().filter(|v| *v != 0.0) {
            s.execute("doc.parameter", &json!({"name": p.name, "expression": v * k})).map_err(|e| e.to_string())?;
            changed += 1;
        }
    }
    if changed == 0 {
        return Err("no numeric parameters".into());
    }
    let after = measure(s)?;
    if !after.valid {
        return Err("invalid after a parameter change".into());
    }
    if (after.volume - before.volume).abs() < 1e-9 {
        return Err("parameters do not drive the shape".into());
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct CaseResult {
    pub id: String,
    pub level: String,
    pub valid: bool,
    pub rebuilt: bool,
    pub matched: bool,
    pub attempts: usize,
    pub seconds: f64,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub backend: String,
    pub model: String,
    pub cases: Vec<CaseResult>,
    pub valid_pct: f64,
    pub rebuilt_pct: f64,
    pub matched_pct: f64,
    pub simple_valid_pct: f64,
    pub mean_seconds: f64,
}

fn pct(n: usize, d: usize) -> f64 {
    if d == 0 { 0.0 } else { (n as f64 * 1000.0 / d as f64).round() / 10.0 }
}

/// Run one case end to end on a fresh session.
pub fn run_case(g: &Generator, case: &Case) -> CaseResult {
    let t = Instant::now();
    let mut r = CaseResult {
        id: case.id.clone(),
        level: case.level.clone(),
        valid: false,
        rebuilt: false,
        matched: false,
        attempts: 0,
        seconds: 0.0,
        notes: Vec::new(),
    };
    let base = Session::new();
    let req = AiRequest { prompt: case.prompt.clone(), use_selection: false };
    match g.generate(&base, &req, &mut |_| {}, &AtomicBool::new(false)) {
        Err(e) => r.notes.push(e.to_string()),
        Ok(p) => {
            r.attempts = p.attempts.len();
            let mut s = p.preview;
            match measure(&mut s) {
                Err(e) => r.notes.push(e),
                Ok(m) => {
                    r.valid = m.valid;
                    let why = mismatch(&m, &case.expect);
                    r.matched = m.valid && why.is_empty();
                    r.notes.extend(why);
                    match rebuilds(&mut s, &p.plan, 1.1) {
                        Ok(()) => r.rebuilt = r.valid,
                        Err(e) => r.notes.push(format!("rebuild: {e}")),
                    }
                }
            }
        }
    }
    r.seconds = t.elapsed().as_secs_f64();
    r
}

pub fn run(g: &Generator, cases: &[Case], progress: &mut dyn FnMut(usize, &CaseResult)) -> Report {
    let mut out = Vec::new();
    for (i, c) in cases.iter().enumerate() {
        let r = run_case(g, c);
        progress(i, &r);
        out.push(r);
    }
    summarize(g.backend.kind(), &g.backend.model(), out)
}

pub fn summarize(backend: &str, model: &str, cases: Vec<CaseResult>) -> Report {
    let n = cases.len();
    let simple: Vec<&CaseResult> = cases.iter().filter(|c| c.level == "simple").collect();
    Report {
        backend: backend.into(),
        model: model.into(),
        valid_pct: pct(cases.iter().filter(|c| c.valid).count(), n),
        rebuilt_pct: pct(cases.iter().filter(|c| c.rebuilt).count(), n),
        matched_pct: pct(cases.iter().filter(|c| c.matched).count(), n),
        simple_valid_pct: pct(simple.iter().filter(|c| c.valid).count(), simple.len()),
        mean_seconds: if n == 0 { 0.0 } else { cases.iter().map(|c| c.seconds).sum::<f64>() / n as f64 },
        cases,
    }
}

/// The reference plans' own results (used to check the expectations).
pub fn check_reference(case: &Case) -> Result<Measured, String> {
    let mut s = Session::new();
    plan::apply(&mut s, &case.reference, "reference").map_err(|e| e.to_string())?;
    let m = measure(&mut s)?;
    let why = mismatch(&m, &case.expect);
    if !why.is_empty() {
        return Err(format!("{m:?}: {}", why.join("; ")));
    }
    rebuilds(&mut s, &case.reference, 1.1)?;
    Ok(m)
}

pub fn to_value(r: &Report) -> Value {
    serde_json::to_value(r).unwrap_or(Value::Null)
}
