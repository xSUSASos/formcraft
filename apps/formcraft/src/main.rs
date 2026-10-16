//! FormCraft desktop app.
//!
//! `formcraft [--screenshot out.png] [--fresh-layout] [--theme dark|light] [--float-browser]
//!  [--text-commands] [--view top|front|right|home] [--control PORT]`
//!
//! `--control PORT` serves the loopback control channel for `formcraft-cli mcp --port PORT`.
//! `--plugin DIR` loads an add-in folder (development); `--scripts DIR` lists scripts from DIR.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::ExitCode;

use formcraft_ui_egui::theme::ThemeKind;
use formcraft_ui_egui::{AppOptions, run};

/// Log lines go to stderr and to `<logs>/formcraft.log` (the previous run's file is kept as
/// `formcraft.old.log`).
struct Tee(Option<std::fs::File>);

impl std::io::Write for Tee {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = std::io::stderr().write_all(buf);
        if let Some(f) = self.0.as_mut() {
            f.write_all(buf)?;
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.as_mut().map_or(Ok(()), std::io::Write::flush)
    }
}

fn init_logging() {
    let file = formcraft_ui_egui::settings::logs_dir().and_then(|d| {
        std::fs::create_dir_all(&d).ok()?;
        let path = d.join("formcraft.log");
        let _ = std::fs::rename(&path, d.join("formcraft.old.log"));
        std::fs::File::create(path).ok()
    });
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,wgpu=warn,naga=warn,eframe=warn"))
        .target(env_logger::Target::Pipe(Box::new(Tee(file))))
        .init();
    log::info!("FormCraft {} starting", env!("CARGO_PKG_VERSION"));
}

fn main() -> ExitCode {
    init_logging();
    let mut opts = AppOptions::default();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--screenshot" => {
                opts.screenshot = args.next().map(Into::into);
                opts.fresh_layout = true;
            }
            "--fresh-layout" => opts.fresh_layout = true,
            "--float-browser" => opts.float_browser = true,
            "--text-commands" => opts.text_commands = true,
            "--view" => opts.view = args.next(),
            "--script" => opts.script = args.next().map(Into::into),
            "--dialog" => opts.dialog = args.next(),
            "--edit" => opts.edit = args.next(),
            "--overlay" => opts.overlay = args.next(),
            "--plugin" => match args.next() {
                Some(d) => opts.plugin_dirs.push(d.into()),
                None => {
                    eprintln!("--plugin needs a folder with plugin.toml");
                    return ExitCode::FAILURE;
                }
            },
            "--scripts" => match args.next() {
                Some(d) => opts.script_dirs.push(d.into()),
                None => {
                    eprintln!("--scripts needs a folder");
                    return ExitCode::FAILURE;
                }
            },
            "--control" => match args.next().and_then(|p| p.parse::<u16>().ok()) {
                Some(p) => opts.control = Some(p),
                None => {
                    eprintln!("--control needs a port number");
                    return ExitCode::FAILURE;
                }
            },
            "--theme" => {
                opts.theme = match args.next().as_deref() {
                    Some("light") => Some(ThemeKind::Light),
                    _ => Some(ThemeKind::Dark),
                }
            }
            other => {
                eprintln!("unknown argument `{other}`");
                return ExitCode::FAILURE;
            }
        }
    }
    match run(opts) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
