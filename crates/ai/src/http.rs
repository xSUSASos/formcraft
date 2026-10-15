//! Minimal HTTP/1.1 client for local LLM servers (Ollama, llama.cpp, LM Studio): `POST` a JSON
//! body, read a JSON reply. Plain `http://` only — cloud endpoints are out of the MVP.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use serde_json::Value;

/// Replies larger than this are refused.
const MAX_BODY: usize = 64 * 1024 * 1024;
const MAX_HEADER_LINES: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    pub host: String,
    pub port: u16,
    /// Base path without a trailing `/` (often empty).
    pub path: String,
}

impl Url {
    pub fn parse(url: &str) -> Result<Url, String> {
        let rest = url
            .trim()
            .strip_prefix("http://")
            .ok_or_else(|| format!("`{url}`: only http:// URLs of local servers are supported"))?;
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], rest[i..].trim_end_matches('/')),
            None => (rest, ""),
        };
        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) => (h, p.parse::<u16>().map_err(|_| format!("`{url}`: bad port"))?),
            None => (authority, 80),
        };
        if host.is_empty() {
            return Err(format!("`{url}`: no host"));
        }
        Ok(Url { host: host.to_string(), port, path: path.to_string() })
    }
}

/// `GET`/`POST` with an optional JSON body; returns the parsed JSON reply. Non-2xx statuses are
/// errors carrying the server's message.
pub fn request(url: &Url, method: &str, path: &str, body: Option<&Value>, timeout: Duration) -> Result<Value, String> {
    let addr = (url.host.as_str(), url.port)
        .to_socket_addrs()
        .map_err(|e| format!("{}:{}: {e}", url.host, url.port))?
        .next()
        .ok_or_else(|| format!("{}: no address", url.host))?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5))
        .map_err(|e| format!("cannot connect to {}:{} ({e}) — is the LLM server running?", url.host, url.port))?;
    stream.set_read_timeout(Some(timeout)).map_err(|e| e.to_string())?;
    stream.set_write_timeout(Some(Duration::from_secs(30))).map_err(|e| e.to_string())?;
    let payload = body.map(Value::to_string).unwrap_or_default();
    let head = format!(
        "{method} {}{path} HTTP/1.1\r\nHost: {}:{}\r\nUser-Agent: formcraft\r\nAccept: application/json\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        url.path,
        url.host,
        url.port,
        payload.len()
    );
    stream.write_all(head.as_bytes()).map_err(|e| e.to_string())?;
    stream.write_all(payload.as_bytes()).map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())?;
    let (status, bytes) = read_response(BufReader::new(stream))?;
    let text = String::from_utf8_lossy(&bytes);
    let json: Result<Value, _> = serde_json::from_str(&text);
    if !(200..300).contains(&status) {
        let msg = json
            .ok()
            .and_then(|v| {
                v.get("error")
                    .map(|e| e.get("message").and_then(Value::as_str).or(e.as_str()).unwrap_or("").to_string())
            })
            .unwrap_or_else(|| text.chars().take(300).collect());
        return Err(format!("HTTP {status}: {msg}"));
    }
    json.map_err(|e| format!("bad JSON from the server: {e}"))
}

/// Status code and body (Content-Length, chunked, or until close).
pub fn read_response(mut r: impl BufRead) -> Result<(u16, Vec<u8>), String> {
    let mut line = String::new();
    r.read_line(&mut line).map_err(|e| e.to_string())?;
    let status = line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| format!("not an HTTP reply: {}", line.trim()))?;
    let (mut length, mut chunked) = (None, false);
    for _ in 0..MAX_HEADER_LINES {
        line.clear();
        if r.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
            break;
        }
        let l = line.trim();
        if l.is_empty() {
            break;
        }
        if let Some((k, v)) = l.split_once(':') {
            let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
            if k == "content-length" {
                length = v.parse::<usize>().ok();
            } else if k == "transfer-encoding" && v.to_ascii_lowercase().contains("chunked") {
                chunked = true;
            }
        }
    }
    let mut body = Vec::new();
    if chunked {
        loop {
            line.clear();
            r.read_line(&mut line).map_err(|e| e.to_string())?;
            let size_hex = line.trim().split(';').next().unwrap_or("");
            let size = usize::from_str_radix(size_hex, 16).map_err(|_| format!("bad chunk size `{size_hex}`"))?;
            if size == 0 {
                break;
            }
            if body.len() + size > MAX_BODY {
                return Err("reply too large".into());
            }
            let start = body.len();
            body.resize(start + size, 0);
            r.read_exact(body.get_mut(start..).unwrap_or_default()).map_err(|e| e.to_string())?;
            line.clear();
            r.read_line(&mut line).map_err(|e| e.to_string())?;
        }
    } else if let Some(n) = length {
        if n > MAX_BODY {
            return Err("reply too large".into());
        }
        body.resize(n, 0);
        r.read_exact(&mut body).map_err(|e| e.to_string())?;
    } else {
        r.take(MAX_BODY as u64 + 1).read_to_end(&mut body).map_err(|e| e.to_string())?;
        if body.len() > MAX_BODY {
            return Err("reply too large".into());
        }
    }
    Ok((status, body))
}
