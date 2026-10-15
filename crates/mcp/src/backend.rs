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
use formcraft_render::gpu::Renderer;
use formcraft_render::headless;
use serde_json::{Value, json};

pub trait Backend {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String>;
    fn has_ui(&self) -> bool;
    fn describe(&self) -> String;
}

/// Standard base64 (for PNG payloads).
pub fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let b = [c.first().copied().unwrap_or(0), c.get(1).copied().unwrap_or(0), c.get(2).copied().unwrap_or(0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for (i, shift) in [18, 12, 6, 0].iter().enumerate() {
            if i <= c.len() {
                out.push(T[((n >> shift) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Render options from MCP/control params.
pub fn render_options(p: &Value) -> Result<headless::Options, String> {
    let mut o = headless::Options::default();
    if let Some(w) = p.get("width").and_then(Value::as_u64) {
        o.width = (w as u32).clamp(16, headless::MAX_SIZE);
    }
    if let Some(h) = p.get("height").and_then(Value::as_u64) {
        o.height = (h as u32).clamp(16, headless::MAX_SIZE);
    }
    if let Some(v) = p.get("view").and_then(Value::as_str) {
        o.view =
            headless::view_dir(v).ok_or(format!("unknown view `{v}` (iso, top, bottom, front, back, left, right)"))?;
        o.orthographic = v != "iso" && v != "home";
    }
    if let Some(e) = p.get("edges").and_then(Value::as_bool) {
        o.edges = e;
    }
    if let Some(g) = p.get("grid").and_then(Value::as_bool) {
        o.grid = g;
    }
    Ok(o)
}

pub fn selectors() -> Value {
    json!({
        "faces": formcraft_engine::doc::select::FACE_NAMES,
        "edges": formcraft_engine::doc::select::EDGE_NAMES,
        "forms": [
            "a name, e.g. \"top\"",
            "{\"near\": [x, y, z]} — the face/edge closest to a point",
            "{\"edges_of\": <face selector>} — edges bounding faces",
            "an array of selectors (union)",
        ],
        "notes": "Selectors are resolved on every rebuild, so they survive parameter changes. Faces: top/bottom = extreme planar faces along ±Z; +X/-X/+Y/-Y likewise. Edges: vertical = straight edges parallel to Z; top_outer = outer boundary of the top face.",
    })
}

/// In-process session (no window). Rendering uses a headless GPU device, created on first use.
pub struct Headless {
    pub session: Session,
    renderer: Option<Result<Renderer, String>>,
}

impl Headless {
    pub fn new() -> Self {
        Headless { session: Session::new(), renderer: None }
    }
}

impl Default for Headless {
    fn default() -> Self {
        Self::new()
    }
}

/// Lazily created headless renderer (creating a GPU device is slow, and may fail).
pub fn renderer(slot: &mut Option<Result<Renderer, String>>) -> Result<&mut Renderer, String> {
    if slot.is_none() {
        *slot = Some(Renderer::headless().map_err(|e| e.to_string()));
    }
    match slot {
        Some(Ok(r)) => Ok(r),
        Some(Err(e)) => Err(format!("rendering is not available: {e}")),
        None => Err("renderer".into()),
    }
}

/// The control methods implemented on a session (shared with the app's control channel).
pub fn session_call(
    s: &mut Session,
    slot: &mut Option<Result<Renderer, String>>,
    method: &str,
    p: Value,
) -> Result<Value, String> {
    match method {
        "engine.execute" => {
            let cmd = p.get("command").and_then(Value::as_str).ok_or("missing `command`")?.to_string();
            let params = p.get("params").cloned().unwrap_or(json!({}));
            s.execute(&cmd, &params).map_err(|e| e.to_string())
        }
        "engine.commands" => {
            s.execute("app.commands", &json!({})).map(|v| v["commands"].clone()).map_err(|e| e.to_string())
        }
        "model.inspect" => s.execute("model.inspect", &json!({})).map_err(|e| e.to_string()),
        "selectors.list" => Ok(selectors()),
        "ui.render" => {
            let o = render_options(&p)?;
            let bodies: Vec<(u64, formcraft_kernel::Shape)> =
                s.model().bodies.iter().filter(|b| b.visible).map(|b| (b.id.0 as u64, b.shape.clone())).collect();
            let kernel = s.kernel().clone();
            let r = renderer(slot)?;
            let png = headless::render_png(r, kernel.as_ref(), &bodies, &o)?;
            if let Some(path) = p.get("path").and_then(Value::as_str) {
                std::fs::write(path, &png).map_err(|e| e.to_string())?;
            }
            Ok(json!({"pngBase64": base64(&png), "width": o.width, "height": o.height, "bodies": bodies.len()}))
        }
        other => Err(format!("method `{other}` is not available here")),
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
