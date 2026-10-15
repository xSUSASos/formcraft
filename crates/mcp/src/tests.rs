use serde_json::{Value, json};

use super::*;

fn server() -> Server {
    Server::new(Box::new(Headless::new()))
}

fn rpc(s: &mut Server, id: u64, method: &str, params: Value) -> Value {
    let line = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string();
    let out = s.handle_line(&line).expect("a reply");
    serde_json::from_str(&out).expect("json")
}

fn tool(s: &mut Server, name: &str, args: Value) -> Value {
    rpc(s, 9, "tools/call", json!({"name": name, "arguments": args}))
}

fn tool_json(s: &mut Server, name: &str, args: Value) -> Value {
    let r = tool(s, name, args);
    assert!(r["result"]["isError"].is_null(), "{name}: {r}");
    serde_json::from_str(r["result"]["content"][0]["text"].as_str().expect("text")).expect("json text")
}

#[test]
fn lifecycle_and_tool_list() {
    let mut s = server();
    let init = rpc(&mut s, 1, "initialize", json!({"protocolVersion": "2025-06-18", "capabilities": {}}));
    assert_eq!(init["result"]["serverInfo"]["name"], "formcraft");
    assert!(s.handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
    assert!(s.is_initialized());
    let tools = rpc(&mut s, 2, "tools/list", json!({}));
    let names: Vec<&str> =
        tools["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    for n in ["execute", "list_commands", "inspect_model", "render", "list_selectors"] {
        assert!(names.contains(&n), "{n} missing");
    }
    assert!(!names.contains(&"screenshot"), "no UI in headless mode");
}

#[test]
fn builds_and_inspects_a_bracket() {
    let mut s = server();
    let script = "doc.parameter {\"name\": \"L\", \"expression\": 60}\n\
        sketch.create {\"rectangles\": [{\"corner\": [0, 0], \"size\": [\"L\", 40]}]}\n\
        solid.extrude {\"distance\": 5}\n\
        solid.fillet {\"edges\": \"vertical\", \"radius\": 3}\n\
        solid.hole {\"points\": [[10, 20, 5], [50, 20, 5]], \"diameter\": 6.6}";
    tool_json(&mut s, "run_script", json!({"text": script}));
    let m = tool_json(&mut s, "inspect_model", json!({}));
    let b = &m["bodies"][0];
    assert_eq!(b["bbox"]["size"], json!([60.0, 40.0, 5.0]));
    assert_eq!(m["errors"], 0);
    let pi = std::f64::consts::PI;
    let want = (2400.0 - (4.0 - pi) * 9.0 - 2.0 * pi * 3.3 * 3.3) * 5.0;
    assert!((b["volume"].as_f64().unwrap() - want).abs() < 1e-3);
    // a command error is a tool error, not a protocol error
    let bad = tool(&mut s, "execute", json!({"command": "solid.fillet", "params": {"radius": 500}}));
    assert_eq!(bad["result"]["isError"], true);
    let filt = tool_json(&mut s, "list_commands", json!({"filter": "extrude"}));
    assert_eq!(filt[0]["id"], "solid.extrude");
}

#[test]
fn render_returns_png_or_a_clear_error() {
    let mut s = server();
    tool_json(&mut s, "execute", json!({"command": "solid.box", "params": {"size": [10, 20, 5]}}));
    let r = tool(&mut s, "render", json!({"width": 64, "height": 48, "view": "top"}));
    let c = &r["result"]["content"][0];
    if r["result"]["isError"] == true {
        assert!(c["text"].as_str().unwrap().contains("rendering is not available"), "{r}");
    } else {
        assert_eq!(c["type"], "image");
        assert!(c["data"].as_str().unwrap().starts_with("iVBORw0KGgo"), "PNG signature in base64");
    }
}

#[test]
fn protocol_errors() {
    let mut s = server();
    let r: Value = serde_json::from_str(&s.handle_line("not json").unwrap()).unwrap();
    assert_eq!(r["error"]["code"], -32700);
    let r = rpc(&mut s, 3, "nope", json!({}));
    assert_eq!(r["error"]["code"], -32601);
    let r = rpc(&mut s, 4, "tools/call", json!({"name": "teleport"}));
    assert_eq!(r["error"]["code"], -32602);
    let r = rpc(&mut s, 5, "tools/call", json!({"name": "execute", "arguments": [1, 2]}));
    assert_eq!(r["error"]["code"], -32602);
    let r: Value = serde_json::from_str(&s.handle_line("[]").unwrap()).unwrap();
    assert_eq!(r["error"]["code"], -32600);
    assert!(s.handle_line("   ").is_none());
    let r = rpc(&mut s, 6, "resources/read", json!({"uri": "formcraft://selectors"}));
    assert!(r["result"]["contents"][0]["text"].as_str().unwrap().contains("vertical"));
}

#[test]
fn base64_matches_rfc4648() {
    assert_eq!(backend::base64(b""), "");
    assert_eq!(backend::base64(b"f"), "Zg==");
    assert_eq!(backend::base64(b"fo"), "Zm8=");
    assert_eq!(backend::base64(b"foo"), "Zm9v");
    assert_eq!(backend::base64(b"foobar"), "Zm9vYmFy");
}
