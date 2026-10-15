//! Command scripts: a JSON array of `{"command": id, "params": {...}}`, or one command per line
//! (`solid.box {"size": [60, 40, 5]}`, `#` comments). Used by the CLI, the app's `--script`
//! and the Text Commands panel.

use serde_json::{Value, json};

use crate::Session;

pub const MAX_STEPS: usize = 100_000;

pub fn parse(text: &str) -> Result<Vec<(String, Value)>, String> {
    let t = text.trim_start();
    let steps: Vec<(String, Value)> = if t.starts_with('[') {
        let v: Vec<Value> = serde_json::from_str(t).map_err(|e| format!("script JSON: {e}"))?;
        v.into_iter()
            .enumerate()
            .map(|(i, step)| {
                let cmd = step.get("command").and_then(Value::as_str).ok_or(format!("step {i}: missing `command`"))?;
                Ok((cmd.to_string(), step.get("params").cloned().unwrap_or(json!({}))))
            })
            .collect::<Result<_, String>>()?
    } else {
        let mut out = Vec::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (cmd, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
            let params = if rest.trim().is_empty() {
                json!({})
            } else {
                serde_json::from_str(rest.trim()).map_err(|e| format!("line {}: {e}", n + 1))?
            };
            out.push((cmd.to_string(), params));
        }
        out
    };
    if steps.len() > MAX_STEPS {
        return Err(format!("script has more than {MAX_STEPS} steps"));
    }
    Ok(steps)
}

/// Run every step; stops at the first error (`Err((step index, message))`).
pub fn run(s: &mut Session, steps: &[(String, Value)]) -> Result<Vec<Value>, (usize, String)> {
    steps.iter().enumerate().map(|(i, (cmd, p))| s.execute(cmd, p).map_err(|e| (i, format!("{cmd}: {e}")))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_forms_parse() {
        let a = parse("# plate\nsolid.box {\"size\": [1,2,3]}\n\nmodel.inspect\n").unwrap();
        assert_eq!(a.len(), 2);
        assert_eq!(a[0].0, "solid.box");
        let b = parse("[{\"command\": \"solid.box\", \"params\": {\"size\": [1,2,3]}}]").unwrap();
        assert_eq!(b[0].1["size"][2], 3);
        assert!(parse("solid.box {bad").is_err());
        assert!(parse("[{\"params\": {}}]").is_err());
    }

    #[test]
    fn run_stops_at_first_error() {
        let mut s = Session::new();
        let steps =
            parse("solid.box {\"size\": [1,1,1]}\nsolid.sphere {\"radius\": -1}\nsolid.box {\"size\": [2,2,2]}")
                .unwrap();
        let e = run(&mut s, &steps).unwrap_err();
        assert_eq!(e.0, 1);
        assert_eq!(s.model().bodies.len(), 1);
    }
}
