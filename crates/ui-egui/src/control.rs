//! Loopback control channel of a running app (`formcraft --control PORT`), used by
//! `formcraft-cli mcp --port PORT`. Newline-delimited JSON: `{id, method, params}` →
//! `{id, result}` or `{id, error}`. Only 127.0.0.1 is bound.
//!
//! Connection threads only parse and forward; every request is executed on the UI thread
//! (`FormCraftApp::serve_control`), so the session is never shared between threads.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use serde_json::{Value, json};

/// Requests longer than this are refused (hostile input).
const MAX_LINE: u64 = 16 * 1024 * 1024;
/// How long a connection waits for the UI thread (a screenshot needs a repaint).
const REPLY_TIMEOUT: Duration = Duration::from_secs(120);

pub type Reply = Sender<Result<Value, String>>;

pub struct Request {
    pub method: String,
    pub params: Value,
    pub reply: Reply,
}

/// Bind 127.0.0.1:`port` (0 = any free port) and accept connections in the background.
/// Returns the request queue and the bound port. `wake` is called after each request is queued
/// (the app passes `request_repaint`).
pub fn listen(port: u16, wake: impl Fn() + Send + Sync + Clone + 'static) -> Result<(Receiver<Request>, u16), String> {
    let listener =
        TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("control channel on 127.0.0.1:{port}: {e}"))?;
    let bound = listener.local_addr().map_err(|e| e.to_string())?.port();
    let (tx, rx) = channel();
    std::thread::Builder::new()
        .name("control-accept".into())
        .spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let (tx, wake) = (tx.clone(), wake.clone());
                let spawned = std::thread::Builder::new()
                    .name("control-conn".into())
                    .spawn(move || serve_connection(stream, &tx, &wake));
                if let Err(e) = spawned {
                    log::error!("control channel: {e}");
                }
            }
        })
        .map_err(|e| e.to_string())?;
    log::info!("control channel listening on 127.0.0.1:{bound}");
    Ok((rx, bound))
}

fn serve_connection(stream: TcpStream, tx: &Sender<Request>, wake: &dyn Fn()) {
    let Ok(mut out) = stream.try_clone() else { return };
    let mut input = BufReader::new(stream);
    loop {
        let mut line = String::new();
        match (&mut input).take(MAX_LINE).read_line(&mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        if !line.ends_with('\n') && line.len() as u64 >= MAX_LINE {
            let _ = writeln!(out, "{}", json!({"id": null, "error": "request too large"}));
            return;
        }
        if line.trim().is_empty() {
            continue;
        }
        let reply = handle_line(&line, tx, wake);
        if writeln!(out, "{reply}").and_then(|()| out.flush()).is_err() {
            return;
        }
    }
}

fn handle_line(line: &str, tx: &Sender<Request>, wake: &dyn Fn()) -> Value {
    let msg: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => return json!({"id": null, "error": format!("parse error: {e}")}),
    };
    let id = msg.get("id").cloned().unwrap_or(Value::Null);
    let Some(method) = msg.get("method").and_then(Value::as_str) else {
        return json!({"id": id, "error": "missing `method`"});
    };
    let (reply, wait) = channel();
    let req = Request { method: method.to_string(), params: msg.get("params").cloned().unwrap_or(json!({})), reply };
    if tx.send(req).is_err() {
        return json!({"id": id, "error": "the app is closing"});
    }
    wake();
    match wait.recv_timeout(REPLY_TIMEOUT) {
        Ok(Ok(result)) => json!({"id": id, "result": result}),
        Ok(Err(e)) => json!({"id": id, "error": e}),
        Err(_) => json!({"id": id, "error": "timed out waiting for the app (is the window minimized?)"}),
    }
}
