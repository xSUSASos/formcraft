//! Embed every `fewshot/*.json` example (adding a file is enough; no list to maintain).

use std::fmt::Write as _;

fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fewshot");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    names.sort();
    let mut out = String::from("pub const FILES: &[(&str, &str)] = &[\n");
    for p in &names {
        println!("cargo:rerun-if-changed={}", p.display());
        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("example");
        let _ = writeln!(out, "    ({stem:?}, include_str!({:?})),", p.display().to_string());
    }
    out.push_str("];\n");
    let target =
        std::path::Path::new(&std::env::var("OUT_DIR").unwrap_or_else(|_| ".".into())).join("fewshot_files.rs");
    if let Err(e) = std::fs::write(&target, out) {
        println!("cargo:warning=fewshot list: {e}");
    }
}
