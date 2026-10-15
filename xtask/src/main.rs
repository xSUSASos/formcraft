//! Workspace tooling: `cargo xtask <command>`.

mod layers;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const USAGE: &str = "\
usage: cargo xtask <command>

commands:
  ci       fmt --check, clippy -D warnings, test, layers, deny (stops at the first failure)
  layers   enforce the crate dependency layering (docs/architecture.md)
  deny     cargo deny check (licenses, advisories, bans, sources)
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("ci") => ci(),
        Some("layers") => layers(),
        Some("deny") => deny(),
        Some("-h" | "--help" | "help") | None => {
            print!("{USAGE}");
            Ok(())
        }
        Some(other) => Err(format!("unknown command `{other}`\n\n{USAGE}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Workspace root (parent of the xtask crate).
fn root() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    dir.parent().map_or_else(|| dir.to_path_buf(), Path::to_path_buf)
}

fn cargo() -> Command {
    let mut c = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    c.current_dir(root());
    c
}

fn run(mut cmd: Command, what: &str) -> Result<(), String> {
    eprintln!("$ {what}");
    let status = cmd.status().map_err(|e| format!("{what}: failed to spawn: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("{what}: exited with {status}")) }
}

fn cargo_step(args: &[&str]) -> Result<(), String> {
    let mut c = cargo();
    c.args(args);
    run(c, &format!("cargo {}", args.join(" ")))
}

fn layers() -> Result<(), String> {
    let out = cargo()
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .map_err(|e| format!("cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err(format!("cargo metadata failed:\n{}", String::from_utf8_lossy(&out.stderr)));
    }
    let meta: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("cargo metadata: bad JSON: {e}"))?;
    let violations = layers::check(&meta)?;
    if violations.is_empty() {
        println!("layers: OK");
        Ok(())
    } else {
        for v in &violations {
            println!("  - {v}");
        }
        Err(format!("{} layering violation(s)", violations.len()))
    }
}

fn deny() -> Result<(), String> {
    // Advisories need network access to the advisory DB; licenses/bans/sources are the gate.
    cargo_step(&["deny", "--log-level", "error", "check", "licenses", "bans", "sources"])
}

fn ci() -> Result<(), String> {
    type Step = (&'static str, fn() -> Result<(), String>);
    let steps: [Step; 5] = [
        ("fmt", || cargo_step(&["fmt", "--all", "--", "--check"])),
        ("clippy", || cargo_step(&["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"])),
        ("test", || cargo_step(&["test", "--workspace"])),
        ("layers", layers),
        ("deny", deny),
    ];
    for (i, (name, f)) in steps.iter().enumerate() {
        eprintln!("\n=== ci: {name} ===");
        if let Err(e) = f() {
            println!("\nCI summary:");
            for (n, _) in &steps[..i] {
                println!("  ok    {n}");
            }
            println!("  FAIL  {name}: {e}");
            return Err(format!("ci failed at `{name}`"));
        }
    }
    println!("\nCI summary: all {} steps passed", steps.len());
    Ok(())
}
