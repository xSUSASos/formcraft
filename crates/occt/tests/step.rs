//! STEP exchange tests. They run as a plain program (`harness = false`): after any STEP
//! transfer, a libtest test binary crashes with an access violation *at process exit* (all
//! tests already passed), while ordinary binaries — the app, the CLI, this runner — exit
//! cleanly. See PROGRESS.md («Тупики» / known issues).

use std::path::Path;
use std::process::ExitCode;

use formcraft_kernel::{DVec2, DVec3, Kernel, Loop2, Plane, Profile};
use formcraft_occt::Occt;

const K: Occt = Occt;

fn plate() -> Result<formcraft_kernel::Shape, String> {
    let p = Profile::new(Plane::XY, Loop2::rect(DVec2::ZERO, DVec2::new(60.0, 40.0)))
        .with_hole(Loop2::circle(DVec2::new(30.0, 20.0), 5.0));
    K.extrude(&p, DVec3::Z * 10.0).map_err(|e| e.to_string())
}

fn round_trip_in_unicode_dir() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("formcraft тест {}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("деталь.step");
    let s = plate()?;
    K.write_step(&s, &path).map_err(|e| e.to_string())?;
    let back = K.read_step(&path).map_err(|e| e.to_string())?;
    let (a, b) = (K.mass_props(&s).map_err(|e| e.to_string())?, K.mass_props(&back).map_err(|e| e.to_string())?);
    let _ = std::fs::remove_dir_all(&dir);
    if (a.volume - b.volume).abs() > 1e-6 * a.volume || a.bbox.min.distance(b.bbox.min) > 1e-6 {
        return Err(format!("round trip changed the body: {a:?} vs {b:?}"));
    }
    Ok(())
}

fn missing_file_is_an_error() -> Result<(), String> {
    match K.read_step(Path::new("does/not/exist.step")) {
        Err(_) => Ok(()),
        Ok(_) => Err("reading a missing file succeeded".into()),
    }
}

fn garbage_file_is_an_error() -> Result<(), String> {
    let path = std::env::temp_dir().join(format!("formcraft-garbage-{}.step", std::process::id()));
    std::fs::write(&path, b"ISO-10303-21;\nthis is not step\n").map_err(|e| e.to_string())?;
    let r = K.read_step(&path);
    let _ = std::fs::remove_file(&path);
    match r {
        Err(_) => Ok(()),
        Ok(s) if K.solids(&s).map(|v| v.is_empty()).unwrap_or(true) => Ok(()),
        Ok(_) => Err("garbage produced a solid".into()),
    }
}

fn main() -> ExitCode {
    let cases: [(&str, fn() -> Result<(), String>); 3] = [
        ("round_trip_in_unicode_dir", round_trip_in_unicode_dir),
        ("missing_file_is_an_error", missing_file_is_an_error),
        ("garbage_file_is_an_error", garbage_file_is_an_error),
    ];
    let mut failed = 0;
    for (name, f) in cases {
        match f() {
            Ok(()) => println!("test {name} ... ok"),
            Err(e) => {
                println!("test {name} ... FAILED: {e}");
                failed += 1;
            }
        }
    }
    println!("step: {} passed, {failed} failed", cases.len() - failed);
    if failed == 0 { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}
