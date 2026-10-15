//! FormCraft command-line tool.
//!
//! ```text
//! formcraft-cli run <script|-> [--save out.step|out.stl] [--quiet]
//! formcraft-cli commands [filter]
//! formcraft-cli info <file.step>
//! ```
//!
//! A script is either a JSON array of `{"command": id, "params": {...}}` objects or text with
//! one command per line: `solid.box {"size": [60, 40, 5]}` (`#` starts a comment).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use formcraft_engine::Session;
use serde_json::{Value, json};

const USAGE: &str = "\
usage:
  formcraft-cli run <script|-> [--save <out.step|out.stl>] [--quiet]
  formcraft-cli commands [filter]
  formcraft-cli info <file.step>

A script is a JSON array of {\"command\": id, \"params\": {...}} or one command per line:
  solid.box {\"size\": [60, 40, 5]}
";

/// Upper bound for scripts read from files/stdin (hostile input).
const MAX_SCRIPT: u64 = 16 * 1024 * 1024;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let r = match args.first().map(String::as_str) {
        Some("run") => run(&args[1..]),
        Some("commands") => commands(args.get(1).map(String::as_str)),
        Some("info") => match args.get(1) {
            Some(p) => info(Path::new(p)),
            None => Err("info needs a file".into()),
        },
        Some("-h" | "--help" | "help") | None => {
            print!("{USAGE}");
            Ok(())
        }
        Some(other) => Err(format!("unknown command `{other}`\n\n{USAGE}")),
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn read_script(src: &str) -> Result<String, String> {
    let mut s = String::new();
    if src == "-" {
        std::io::stdin().take(MAX_SCRIPT).read_to_string(&mut s).map_err(|e| e.to_string())?;
    } else {
        let f = std::fs::File::open(src).map_err(|e| format!("{src}: {e}"))?;
        f.take(MAX_SCRIPT).read_to_string(&mut s).map_err(|e| format!("{src}: {e}"))?;
    }
    Ok(s)
}

/// Parse a script into (command, params) steps.
pub fn parse_script(text: &str) -> Result<Vec<(String, Value)>, String> {
    let t = text.trim_start();
    if t.starts_with('[') {
        let v: Vec<Value> = serde_json::from_str(t).map_err(|e| format!("script JSON: {e}"))?;
        return v
            .into_iter()
            .enumerate()
            .map(|(i, step)| {
                let cmd = step.get("command").and_then(Value::as_str).ok_or(format!("step {i}: missing `command`"))?;
                Ok((cmd.to_string(), step.get("params").cloned().unwrap_or(json!({}))))
            })
            .collect();
    }
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
    Ok(out)
}

fn run(args: &[String]) -> Result<(), String> {
    let mut script = None;
    let mut save: Option<PathBuf> = None;
    let mut quiet = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--save" => save = Some(it.next().ok_or("--save needs a path")?.into()),
            "--quiet" | "-q" => quiet = true,
            s if script.is_none() => script = Some(s.to_string()),
            other => return Err(format!("unexpected argument `{other}`")),
        }
    }
    let steps = parse_script(&read_script(&script.ok_or("run needs a script (or - for stdin)")?)?)?;
    let mut session = Session::new();
    for (i, (cmd, params)) in steps.iter().enumerate() {
        match session.execute(cmd, params) {
            Ok(v) => {
                if !quiet {
                    let msg = v.get("message").and_then(Value::as_str).unwrap_or("ok");
                    eprintln!("[{}] {cmd}: {msg}", i + 1);
                }
            }
            Err(e) => return Err(format!("step {} ({cmd}): {e}", i + 1)),
        }
    }
    if let Some(path) = save {
        session.execute("doc.export", &json!({"path": path.to_string_lossy()})).map_err(|e| e.to_string())?;
    }
    let report = session.execute("model.inspect", &json!({})).map_err(|e| e.to_string())?;
    println!("{}", serde_json::to_string_pretty(&report).unwrap_or_default());
    Ok(())
}

fn commands(filter: Option<&str>) -> Result<(), String> {
    let mut s = Session::new();
    let v = s.execute("app.commands", &json!({"filter": filter.unwrap_or("")})).map_err(|e| e.to_string())?;
    println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
    Ok(())
}

fn info(path: &Path) -> Result<(), String> {
    let s = Session::new();
    let k = s.kernel();
    let solids = formcraft_io::import_step(k.as_ref(), path).map_err(|e| e.to_string())?;
    let mut bodies = Vec::new();
    for (i, b) in solids.iter().enumerate() {
        let m = k.mass_props(b).map_err(|e| e.to_string())?;
        bodies.push(json!({
            "index": i,
            "volume": m.volume,
            "area": m.area,
            "bbox": {"min": m.bbox.min.to_array(), "max": m.bbox.max.to_array()},
            "faces": k.faces(b).map(|f| f.len()).unwrap_or(0),
            "valid": k.is_valid(b),
        }));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({"file": path.display().to_string(), "solids": bodies}))
            .unwrap_or_default()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_parse_in_both_forms() {
        let a = parse_script("# plate\nsolid.box {\"size\": [1,2,3]}\n\nmodel.inspect\n").unwrap();
        assert_eq!(a.len(), 2);
        assert_eq!(a[0].0, "solid.box");
        let b = parse_script("[{\"command\": \"solid.box\", \"params\": {\"size\": [1,2,3]}}]").unwrap();
        assert_eq!(b[0].1["size"][2], 3);
        assert!(parse_script("solid.box {bad").is_err());
        assert!(parse_script("[{\"params\": {}}]").is_err());
    }
}
