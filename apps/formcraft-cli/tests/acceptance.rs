//! Phase 2 acceptance: `formcraft-cli run examples/bracket.fcs --save out.step`, then read the
//! STEP back (`formcraft-cli info`) and compare volume and bounding box with the exact values.
//! Optionally (FREECAD_CMD set) the file is also opened in FreeCAD.
//! Runs as a plain program (`harness = false`, see crates/occt/tests/step.rs).

use std::path::PathBuf;
use std::process::{Command, ExitCode};

use serde_json::Value;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_formcraft-cli"))
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn expected_volume() -> f64 {
    60.0 * 40.0 * 5.0 - 2.0 * std::f64::consts::PI * 3.3 * 3.3 * 5.0
}

fn check(name: &str, ok: bool, detail: String, failed: &mut u32) {
    if ok {
        println!("test {name} ... ok");
    } else {
        println!("test {name} ... FAILED: {detail}");
        *failed += 1;
    }
}

fn main() -> ExitCode {
    let mut failed = 0;
    let dir = std::env::temp_dir().join(format!("formcraft-accept-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let step = dir.join("bracket.step");
    let stl = dir.join("bracket.stl");

    for out in [&step, &stl] {
        let r =
            cli().arg("run").arg(root().join("examples/bracket.fcs")).arg("--save").arg(out).arg("--quiet").output();
        let ok = r.as_ref().is_ok_and(|o| o.status.success()) && out.is_file();
        let detail = r.map(|o| String::from_utf8_lossy(&o.stderr).to_string()).unwrap_or_else(|e| e.to_string());
        check(
            &format!("run_and_save_{}", out.extension().and_then(|e| e.to_str()).unwrap_or("")),
            ok,
            detail,
            &mut failed,
        );
    }

    let info = cli().arg("info").arg(&step).output();
    match info {
        Ok(o) if o.status.success() => {
            let v: Value = serde_json::from_slice(&o.stdout).unwrap_or(Value::Null);
            let s = &v["solids"][0];
            let vol = s["volume"].as_f64().unwrap_or(0.0);
            check(
                "step_volume",
                (vol - expected_volume()).abs() < 1e-3,
                format!("{vol} vs {}", expected_volume()),
                &mut failed,
            );
            let min: Vec<f64> =
                s["bbox"]["min"].as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
            let max: Vec<f64> =
                s["bbox"]["max"].as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
            let bbox_ok = min.len() == 3
                && max.len() == 3
                && min.iter().all(|x| x.abs() < 1e-6)
                && (max[0] - 60.0).abs() < 1e-6
                && (max[1] - 40.0).abs() < 1e-6
                && (max[2] - 5.0).abs() < 1e-6;
            check("step_bbox", bbox_ok, format!("{min:?} {max:?}"), &mut failed);
            check(
                "step_one_valid_solid",
                v["solids"].as_array().map(Vec::len) == Some(1) && s["valid"] == true,
                v.to_string(),
                &mut failed,
            );
        }
        other => check("info", false, format!("{other:?}"), &mut failed),
    }

    // Optional external check with FreeCAD (FREECAD_CMD=path to freecadcmd, ASCII temp dir).
    if let Ok(fc) = std::env::var("FREECAD_CMD") {
        let script = root().join("tools/freecad_check.py");
        let o = Command::new(fc).arg(&script).env("FC_FILES", &step).output();
        let out = o.map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
        let line = out.lines().find(|l| l.starts_with("FCCHECK ")).unwrap_or("");
        let v: Value = serde_json::from_str(line.trim_start_matches("FCCHECK ")).unwrap_or(Value::Null);
        let vol = v["volume"].as_f64().unwrap_or(0.0);
        check("freecad_opens_step", v["ok"] == true && (vol - expected_volume()).abs() < 1e-2, out, &mut failed);
    } else {
        println!("test freecad_opens_step ... skipped (set FREECAD_CMD)");
    }

    let _ = std::fs::remove_dir_all(&dir);
    println!("acceptance: {failed} failed");
    if failed == 0 { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}
