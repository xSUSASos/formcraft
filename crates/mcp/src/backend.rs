//! Where MCP tool calls go: an in-process headless session, or a running FormCraft app through
//! its loopback control channel (`formcraft --control PORT`). Both speak the same methods:
//!
//! `engine.execute {command, params}`, `engine.commands`, `model.inspect`,
//! `ui.render {width, height, view, edges, grid}` → `{pngBase64}`, `selectors.list`,
//! and with a UI: `ui.screenshot {path?}`, `ui.click {x, y, space, button}`.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::Duration;

use formcraft_engine::Session;
pub use formcraft_render::control::{base64, render_options, renderer, selectors, session_call};
use formcraft_render::gpu::Renderer;
use serde_json::{Value, json};

pub trait Backend {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String>;
    fn has_ui(&self) -> bool;
    fn describe(&self) -> String;
}

/// In-process session (no window). Rendering uses a headless GPU device, created on first use.
pub struct Headless {
    pub session: Session,
    renderer: Option<Result<Renderer, String>>,
}

impl Headless {
    pub fn new() -> Self {
        let mut session = Session::new();
        if let Err(e) = formcraft_ai::install(&mut session, &formcraft_ai::AiConfig::from_env()) {
            log::warn!("assistant unavailable: {e}");
        }
        Headless { session, renderer: None }
    }
}

impl Default for Headless {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for Headless {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        session_call(&mut self.session, &mut self.renderer, method, params)
    }
    fn has_ui(&self) -> bool {
        false
    }
    fn describe(&self) -> String {
        "headless session (no window)".into()
    }
}

/// A running app's control channel: newline-delimited JSON {id, method, params} → {id, result|error}.
pub struct Remote {
    stream: BufReader<TcpStream>,
    next: u64,
    port: u16,
}

impl Remote {
    pub fn connect(port: u16) -> Result<Self, String> {
        let s = TcpStream::connect(("127.0.0.1", port))
            .map_err(|e| format!("cannot connect to FormCraft on port {port}: {e}"))?;
        s.set_read_timeout(Some(Duration::from_secs(120))).map_err(|e| e.to_string())?;
        Ok(Remote { stream: BufReader::new(s), next: 1, port })
    }
}

impl Backend for Remote {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next;
        self.next += 1;
        let line = json!({"id": id, "method": method, "params": params}).to_string() + "\n";
        self.stream.get_mut().write_all(line.as_bytes()).map_err(|e| e.to_string())?;
        let mut reply = String::new();
        self.stream.read_line(&mut reply).map_err(|e| e.to_string())?;
        if reply.len() > 256 * 1024 * 1024 {
            return Err("reply too large".into());
        }
        let v: Value = serde_json::from_str(&reply).map_err(|e| format!("bad reply: {e}"))?;
        match (v.get("result"), v.get("error")) {
            (Some(r), _) => Ok(r.clone()),
            (_, Some(e)) => Err(e.as_str().map(str::to_string).unwrap_or_else(|| e.to_string())),
            _ => Err("empty reply".into()),
        }
    }
    fn has_ui(&self) -> bool {
        true
    }
    fn describe(&self) -> String {
        format!("running FormCraft app on 127.0.0.1:{}", self.port)
    }
}
