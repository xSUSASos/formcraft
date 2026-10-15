//! FormCraft command-line tool.
//!
//! ```text
//! formcraft-cli run <script|-> [--save out.step|out.stl] [--quiet]
//! formcraft-cli commands [filter]
//! formcraft-cli info <file.step|file.fcraft>
//! formcraft-cli convert <in.step|in.fcraft> <out.step|out.stl>
//! formcraft-cli render <in.fcraft|script.fcs|in.step> <out.png> [--view iso|top|front|…] [--size WxH]
//! formcraft-cli mcp [--port N]
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
  formcraft-cli info <file.step|file.fcraft>
  formcraft-cli convert <in.step|in.fcraft|script.fcs> <out.step|out.stl>
  formcraft-cli render <in.fcraft|script.fcs|in.step> <out.png> [--view iso|top|front|right|…] [--size WxH]
  formcraft-cli mcp [--port N]     MCP server on stdio (headless, or attached to `formcraft --control N`)
  formcraft-cli screenshot <scene|all> <out.png|dir> [--theme light|dark|both] [--size WxH]
                                   offscreen UI screenshots (UI_SPEC.md §8 scenes; default 1920x1080, both themes)
  formcraft-cli ai \"<request>\" [--save <out.step|out.stl|out.fcraft>] [--plan-only]
                                   build a part with the local LLM (FORMCRAFT_AI_MODEL, FORMCRAFT_AI_URL)

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
        Some("convert") => match (args.get(1), args.get(2)) {
            (Some(a), Some(b)) => convert(Path::new(a), Path::new(b)),
            _ => Err("convert needs <input> <output>".into()),
        },
        Some("render") => render(&args[1..]),
        Some("mcp") => mcp(&args[1..]),
        Some("ai") => ai(&args[1..]),
        Some("screenshot") => screenshot(&args[1..]),
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

pub use formcraft_engine::script::parse as parse_script;

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
    if let Err(e) = formcraft_ai::install(&mut session, &formcraft_ai::AiConfig::from_env()) {
        eprintln!("assistant unavailable: {e}");
    }
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

fn screenshot(args: &[String]) -> Result<(), String> {
    use formcraft_ui_egui::screens;
    use formcraft_ui_egui::theme::ThemeKind;
    let (mut pos, mut themes, mut size) = (Vec::new(), vec![ThemeKind::Light, ThemeKind::Dark], [1920u32, 1080]);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--theme" => {
                themes = match it.next().map(String::as_str) {
                    Some("light") => vec![ThemeKind::Light],
                    Some("dark") => vec![ThemeKind::Dark],
                    Some("both") => vec![ThemeKind::Light, ThemeKind::Dark],
                    _ => return Err("--theme light|dark|both".into()),
                }
            }
            "--size" => {
                let v = it.next().ok_or("--size WxH")?;
                let (w, h) = v.split_once('x').ok_or("--size WxH")?;
                size = [
                    w.parse::<u32>().map_err(|e| e.to_string())?.clamp(320, 7680),
                    h.parse::<u32>().map_err(|e| e.to_string())?.clamp(240, 4320),
                ];
            }
            p => pos.push(p.to_string()),
        }
    }
    let (scene, out) = match pos.as_slice() {
        [s, o] => (s.clone(), PathBuf::from(o)),
        _ => return Err("screenshot needs <scene|all> <out>".into()),
    };
    let scenes: Vec<&str> =
        if scene == "all" { screens::SCENES.iter().map(|(s, _)| *s).collect() } else { vec![scene.as_str()] };
    let many = scenes.len() > 1 || themes.len() > 1;
    if many {
        std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    }
    for s in &scenes {
        for t in &themes {
            let img = screens::render(s, *t, size)?;
            let name = format!("{s}-{}.png", if *t == ThemeKind::Light { "light" } else { "dark" });
            let path = if many { out.join(name) } else { out.clone() };
            img.save(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            eprintln!("{}", path.display());
        }
    }
    Ok(())
}

fn ai(args: &[String]) -> Result<(), String> {
    let mut prompt = None;
    let (mut save, mut plan_only) = (None::<String>, false);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--save" => save = Some(it.next().ok_or("--save needs a path")?.clone()),
            "--plan-only" => plan_only = true,
            p if prompt.is_none() => prompt = Some(p.to_string()),
            other => return Err(format!("unexpected argument `{other}`")),
        }
    }
    let prompt = prompt.ok_or("ai needs a request in quotes")?;
    let mut s = Session::new();
    let config = formcraft_ai::AiConfig::from_env();
    let generator = formcraft_ai::install(&mut s, &config)?;
    eprintln!("{} · {} …", config.backend, config.model);
    let req = formcraft_ai::AiRequest { prompt, use_selection: false };
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let p = generator.generate(&s, &req, &mut |pr| eprintln!("{}", pr.label()), &cancel).map_err(|e| e.to_string())?;
    for (i, a) in p.attempts.iter().enumerate() {
        eprintln!(
            "attempt {}: {:.1} s{}",
            i + 1,
            a.seconds,
            a.error.as_ref().map(|e| format!(" — {e}")).unwrap_or_default()
        );
    }
    println!("{}", serde_json::to_string_pretty(&p.plan).unwrap_or_default());
    if plan_only {
        return Ok(());
    }
    let mut preview = p.preview;
    let report = preview.execute("model.inspect", &json!({})).map_err(|e| e.to_string())?;
    println!("{}", serde_json::to_string_pretty(&report).unwrap_or_default());
    if let Some(path) = save {
        let cmd = if path.ends_with(".fcraft") { "doc.save" } else { "doc.export" };
        preview.execute(cmd, &json!({"path": path})).map_err(|e| e.to_string())?;
        eprintln!("saved {path}");
    }
    eprintln!("done in {:.1} s", p.elapsed.as_secs_f64());
    Ok(())
}

fn commands(filter: Option<&str>) -> Result<(), String> {
    let mut s = Session::new();
    let v = s.execute("app.commands", &json!({"filter": filter.unwrap_or("")})).map_err(|e| e.to_string())?;
    println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
    Ok(())
}

/// A session loaded from a .fcraft design, a command script or a STEP file (bodies as
/// imported features would need Phase 7; STEP is handled by the callers directly).
fn load(path: &Path) -> Result<Session, String> {
    let mut s = Session::new();
    match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("fcraft") => {
            s.execute("doc.open", &json!({"path": path.to_string_lossy()})).map_err(|e| e.to_string())?;
        }
        Some("fcs") | Some("txt") | Some("json") => {
            let steps = parse_script(&read_script(&path.to_string_lossy())?)?;
            formcraft_engine::script::run(&mut s, &steps).map_err(|(i, e)| format!("step {}: {e}", i + 1))?;
        }
        _ => return Err(format!("{}: expected .fcraft or a .fcs script", path.display())),
    }
    Ok(s)
}

fn is_step(p: &Path) -> bool {
    matches!(p.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref(), Some("step" | "stp"))
}

fn shapes_of(path: &Path) -> Result<(Session, Vec<formcraft_kernel::Shape>), String> {
    if is_step(path) {
        let s = Session::new();
        let solids = formcraft_io::import_step(s.kernel().as_ref(), path).map_err(|e| e.to_string())?;
        Ok((s, solids))
    } else {
        let s = load(path)?;
        let shapes = s.model().bodies.iter().filter(|b| b.visible).map(|b| b.shape.clone()).collect();
        Ok((s, shapes))
    }
}

fn convert(input: &Path, output: &Path) -> Result<(), String> {
    let (s, shapes) = shapes_of(input)?;
    formcraft_io::export(s.kernel().as_ref(), &shapes, output).map_err(|e| e.to_string())?;
    eprintln!("{} body(ies) → {}", shapes.len(), output.display());
    Ok(())
}

fn render(args: &[String]) -> Result<(), String> {
    let (Some(input), Some(out)) = (args.first(), args.get(1)) else {
        return Err("render needs <input> <out.png>".into());
    };
    let mut p = json!({});
    let mut it = args.iter().skip(2);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--view" => p["view"] = json!(it.next().ok_or("--view needs a name")?),
            "--size" => {
                let v = it.next().ok_or("--size needs WxH")?;
                let (w, h) = v.split_once('x').ok_or("--size needs WxH")?;
                p["width"] = json!(w.parse::<u32>().map_err(|e| e.to_string())?);
                p["height"] = json!(h.parse::<u32>().map_err(|e| e.to_string())?);
            }
            "--no-grid" => p["grid"] = json!(false),
            "--no-edges" => p["edges"] = json!(false),
            other => return Err(format!("unexpected argument `{other}`")),
        }
    }
    let (s, shapes) = shapes_of(Path::new(input))?;
    let o = formcraft_mcp::backend::render_options(&p)?;
    let bodies: Vec<(u64, formcraft_kernel::Shape)> =
        shapes.into_iter().enumerate().map(|(i, sh)| (i as u64 + 1, sh)).collect();
    let mut r = formcraft_render::gpu::Renderer::headless().map_err(|e| e.to_string())?;
    let png = formcraft_render::headless::render_png(&mut r, s.kernel().as_ref(), &bodies, &o)?;
    std::fs::write(out, png).map_err(|e| format!("{out}: {e}"))?;
    eprintln!("rendered {} body(ies) → {out}", bodies.len());
    Ok(())
}

fn mcp(args: &[String]) -> Result<(), String> {
    let backend: Box<dyn formcraft_mcp::Backend> = match args.iter().position(|a| a == "--port") {
        Some(i) => {
            let port: u16 =
                args.get(i + 1).ok_or("--port needs a number")?.parse().map_err(|e| format!("--port: {e}"))?;
            Box::new(formcraft_mcp::Remote::connect(port)?)
        }
        None => Box::new(formcraft_mcp::Headless::new()),
    };
    let mut server = formcraft_mcp::Server::new(backend);
    let stdin = std::io::stdin();
    server.serve(stdin.lock(), std::io::stdout().lock()).map_err(|e| e.to_string())
}

fn info(path: &Path) -> Result<(), String> {
    if !is_step(path) {
        let mut s = load(path)?;
        let v = s.execute("model.inspect", &json!({})).map_err(|e| e.to_string())?;
        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
        return Ok(());
    }
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
