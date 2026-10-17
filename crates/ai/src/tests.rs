use std::io::{BufRead, BufReader, Read, Write};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use formcraft_engine::Session;
use serde_json::{Value, json};

use super::*;
use crate::backend::{ChatRequest, Message, Role};

fn generator(mock: Mock) -> (Arc<Mock>, Generator) {
    let m = Arc::new(mock);
    let mut g = Generator::new(m.clone());
    g.log = false;
    (m, g)
}

const GOOD: &str = r#"{"summary": "Plate 40x30", "parameters": [{"name": "plate_t", "expression": 4}],
  "steps": [{"command": "solid.box", "params": {"size": [40, 30, "plate_t"]}},
            {"command": "solid.hole", "params": {"points": [[20, 15, 4]], "diameter": 6}}]}"#;

const BAD_FILLET: &str = r#"{"summary": "Plate 40x30", "parameters": [],
  "steps": [{"command": "solid.box", "params": {"size": [40, 30, 4]}},
            {"command": "solid.fillet", "params": {"edges": "vertical", "radius": 500}}]}"#;

#[test]
fn every_fewshot_example_builds_a_valid_body() {
    let all = fewshot::all();
    assert!(all.len() >= 3, "few-shot examples are embedded");
    for e in all {
        let mut s = Session::new();
        let added = plan::apply(&mut s, &e.plan, "example").unwrap_or_else(|err| panic!("{}: {err}", e.name));
        assert!(!added.is_empty(), "{}", e.name);
        assert!(!s.model().bodies.is_empty(), "{}: no body", e.name);
        for b in &s.model().bodies {
            assert!(s.kernel().is_valid(&b.shape), "{}: invalid body", e.name);
        }
        assert!(!e.plan.parameters.is_empty(), "{}: examples teach named parameters", e.name);
    }
}

#[test]
fn fewshot_pick_prefers_similar_requests() {
    let p = fewshot::pick("a flange with six bolt holes on a circle", 1);
    assert_eq!(p[0].name, "flange");
    assert_eq!(fewshot::pick("anything", 2).len(), 2);
}

#[test]
fn parse_tolerates_wrappers() {
    let fenced = format!("Sure!\n```json\n{GOOD}\n```\nDone.");
    assert_eq!(plan::parse(&fenced).unwrap().steps.len(), 2);
    let think = format!("<think>maybe {{not json}}</think>{GOOD}");
    assert_eq!(plan::parse(&think).unwrap().summary, "Plate 40x30");
    let arr = r#"[{"command": "solid.box", "params": {"size": [1, 2, 3]}}]"#;
    assert_eq!(plan::parse(arr).unwrap().steps[0].command, "solid.box");
    assert!(plan::parse("no json here").is_err());
    assert!(plan::parse("{\"steps\": [ {\"command\": ").is_err());
    assert!(plan::parse(r#"{"steps": "solid.box"}"#).is_err());
}

#[test]
fn validate_rejects_what_the_assistant_may_not_do() {
    let s = Session::new();
    let r = s.registry();
    let p = |t: &str| plan::parse(t).unwrap();
    assert!(plan::validate(&p(GOOD), r).is_ok());
    let unknown = plan::validate(&p(r#"{"steps": [{"command": "solid.teleport", "params": {}}]}"#), r).unwrap_err();
    assert_eq!(unknown.step, 1);
    assert!(plan::validate(&p(r#"{"steps": [{"command": "doc.save", "params": {"path": "x"}}]}"#), r).is_err());
    assert!(plan::validate(&p(r#"{"steps": []}"#), r).is_err());
    let bad_name = r#"{"parameters": [{"name": "mm", "expression": 1}], "steps": [{"command": "solid.box", "params": {"size": [1,1,1]}}]}"#;
    assert!(plan::validate(&p(bad_name), r).is_err());
    let bad_params = r#"{"steps": [{"command": "solid.box", "params": [1, 2]}]}"#;
    assert!(plan::validate(&p(bad_params), r).is_err());
}

#[test]
fn repair_loop_feeds_the_error_back() {
    let (mock, g) = generator(Mock::replies([BAD_FILLET, GOOD]));
    let base = Session::new();
    let mut seen = Vec::new();
    let p = g
        .generate(
            &base,
            &AiRequest { prompt: "plate 40x30".into(), use_selection: false },
            &mut |pr| seen.push(pr.clone()),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(p.attempts.len(), 2);
    assert!(p.attempts[0].error.as_deref().unwrap_or("").contains("solid.fillet"));
    assert!(matches!(seen[1], Progress::Repairing { attempt: 2, of: 3, .. }));
    let reqs = mock.requests.lock().unwrap();
    let last = reqs[1].messages.last().unwrap();
    assert_eq!(last.role, Role::User);
    assert!(last.content.contains("step 2 (solid.fillet)"), "{}", last.content);
    assert!(reqs[0].schema.is_some(), "structured output requested");
    // the preview has the plate with a hole; the base is untouched
    assert_eq!(p.preview.model().bodies.len(), 1);
    assert!(base.document().features.is_empty());
}

#[test]
fn gives_up_after_three_attempts() {
    let (mock, g) = generator(Mock::with(|_| Ok(BAD_FILLET.to_string())));
    let base = Session::new();
    let e = g
        .generate(&base, &AiRequest { prompt: "x".into(), use_selection: false }, &mut |_| {}, &AtomicBool::new(false))
        .err()
        .unwrap();
    assert!(matches!(e, AiError::Failed { attempts: 3, .. }), "{e}");
    assert_eq!(mock.request_count(), 3);
    let cancelled =
        g.generate(&base, &AiRequest { prompt: "x".into(), use_selection: false }, &mut |_| {}, &AtomicBool::new(true));
    assert!(matches!(cancelled, Err(AiError::Cancelled)));
    let offline =
        Generator { log: false, ..Generator::new(Arc::new(Mock::with(|_| Err("connection refused".into())))) };
    let e = offline.generate(
        &base,
        &AiRequest { prompt: "x".into(), use_selection: false },
        &mut |_| {},
        &AtomicBool::new(false),
    );
    assert!(matches!(e, Err(AiError::Backend(_))));
}

#[test]
fn ai_generate_is_one_undo_step_and_a_group() {
    let mut s = Session::new();
    s.execute("solid.box", &json!({"corner": [100, 0, 0], "size": [5, 5, 5]})).unwrap();
    let (_, mut g) = generator(Mock::replies([GOOD]));
    g.log = false;
    commands::register(&mut s, Arc::new(g)).unwrap();
    let r = s.execute("ai.generate", &json!({"prompt": "a 40 by 30 plate, 4 thick, 6 mm hole in the middle"})).unwrap();
    assert_eq!(r["features"].as_array().unwrap().len(), 2);
    let d = s.document();
    assert_eq!(d.parameter("plate_t").map(|p| p.expr.as_str()), Some("4"));
    let groups: Vec<_> = d.features.iter().filter_map(|f| f.group).collect();
    assert_eq!(groups.len(), 2);
    assert_eq!(d.groups[&groups[0]], "Plate 40x30");
    assert_eq!(s.undo_label(), Some("AI: Plate 40x30"));
    s.undo().unwrap();
    assert_eq!(s.document().features.len(), 1);
    assert!(s.document().parameter("plate_t").is_none());
    assert!(s.execute("ai.generate", &json!({"prompt": "  "})).is_err());
}

#[test]
fn explain_describes_features() {
    let mut s = Session::new();
    s.execute("doc.parameter", &json!({"name": "T", "expression": 5})).unwrap();
    s.execute("sketch.create", &json!({"rectangles": [{"corner": [0, 0], "size": [20, 10]}]})).unwrap();
    s.execute("solid.extrude", &json!({"distance": "T"})).unwrap();
    commands::register(&mut s, Arc::new(generator(Mock::replies(Vec::<String>::new())).1)).unwrap();
    let r = s.execute("ai.explain", &json!({"feature": "Extrude1"})).unwrap();
    let t = r["text"].as_str().unwrap();
    assert!(t.contains("Extrude1 — Extrude"), "{t}");
    assert!(t.contains("distance: T"), "{t}");
    assert!(t.contains("without errors"), "{t}");
    assert!(s.execute("ai.explain", &json!({"feature": "Nope"})).is_err());
    let st = s.execute("ai.status", &json!({})).unwrap();
    assert_eq!(st["ok"], true);
}

#[test]
fn prompt_comes_from_the_registry() {
    let mut s = Session::new();
    let sys = prompt::system_prompt(s.registry());
    for id in ["solid.extrude", "solid.hole", "sketch.create", "doc.parameter", "solid.fillet"] {
        assert!(sys.contains(id), "{id} missing");
    }
    assert!(!sys.contains("doc.save"), "non-AI commands are not offered");
    assert!(sys.contains("vertical"), "selectors are explained");
    assert!(sys.contains("size: [number|expr x3] (required)"), "compact schema rendering");
    // a command registered at runtime (a plugin) is offered at once
    s.registry_mut()
        .register(
            formcraft_engine::CommandSpec::new("gear.spur", "Spur Gear", "Involute spur gear.")
                .params(json!({"type": "object", "properties": {"teeth": {"type": "integer"}}}))
                .ai()
                .source(formcraft_engine::Source::Plugin("gear".into())),
            Arc::new(|_, _| Ok(json!({}))),
        )
        .unwrap();
    assert!(prompt::system_prompt(s.registry()).contains("gear.spur — Involute spur gear."));
    let schema = prompt::output_schema(s.registry());
    let ids = schema["properties"]["steps"]["items"]["properties"]["command"]["enum"].as_array().unwrap();
    assert!(ids.contains(&json!("gear.spur")));
}

#[test]
fn context_mentions_the_design_and_selection() {
    let mut s = Session::new();
    s.execute("doc.parameter", &json!({"name": "L", "expression": 60})).unwrap();
    s.execute("solid.box", &json!({"size": ["L", 40, 5]})).unwrap();
    let body = s.model().bodies[0].id;
    let top = s.kernel().faces(&s.model().bodies[0].shape).unwrap().into_iter().find(|f| f.normal.z > 0.9).unwrap();
    s.selection_mut().set(formcraft_engine::Pick::Face { body, index: top.index });
    let c = pipeline::context(&s, true);
    assert!(c.contains("L = 60"), "{c}");
    assert!(c.contains("bbox [0.0, 0.0, 0.0] to [60.0, 40.0, 5.0]"), "{c}");
    assert!(c.contains("{\"near\": [30.0, 20.0, 5.0]}"), "{c}");
    assert_eq!(pipeline::context(&Session::new(), true), "");
}

#[test]
fn http_reads_length_chunked_and_close_delimited_bodies() {
    let r = |s: &str| http::read_response(BufReader::new(s.as_bytes()));
    assert_eq!(r("HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}").unwrap(), (200, b"{}".to_vec()));
    let chunked = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\n{\"a\r\n4\r\n\":1}\r\n0\r\n\r\n";
    assert_eq!(r(chunked).unwrap().1, b"{\"a\":1}".to_vec());
    assert_eq!(r("HTTP/1.0 404 Not Found\r\n\r\nnope").unwrap(), (404, b"nope".to_vec()));
    assert!(r("garbage").is_err());
    assert!(r("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\n").is_err());
    assert!(r("HTTP/1.1 200 OK\r\nContent-Length: 999999999999\r\n\r\n").is_err());
    assert_eq!(
        http::Url::parse("http://127.0.0.1:11434/").unwrap(),
        http::Url { host: "127.0.0.1".into(), port: 11434, path: String::new() }
    );
    assert_eq!(http::Url::parse("http://localhost:8080/v1").unwrap().path, "/v1");
    assert!(http::Url::parse("https://api.example.com").is_err());
    assert!(http::Url::parse("http://h:99999").is_err());
}

/// A fake Ollama on a loopback port: checks the request and answers like `/api/chat`.
#[test]
fn ollama_backend_speaks_the_chat_api() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut c, _) = listener.accept().unwrap();
        let mut r = BufReader::new(c.try_clone().unwrap());
        let mut line = String::new();
        r.read_line(&mut line).unwrap();
        assert!(line.starts_with("POST /api/chat "), "{line}");
        let mut len = 0;
        loop {
            line.clear();
            r.read_line(&mut line).unwrap();
            if line.trim().is_empty() {
                break;
            }
            if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                len = v.trim().parse().unwrap();
            }
        }
        let mut body = vec![0; len];
        r.read_exact(&mut body).unwrap();
        let req: Value = serde_json::from_slice(&body).unwrap();
        let reply = json!({"model": req["model"], "message": {"role": "assistant", "content": GOOD},
                           "prompt_eval_count": 1200, "eval_count": 90});
        let text = reply.to_string();
        write!(c, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{text}", text.len())
            .unwrap();
        req
    });
    let cfg = AiConfig { url: format!("http://127.0.0.1:{port}"), model: "test-model".into(), ..AiConfig::default() };
    let b = cfg.build().unwrap();
    let reply = b
        .chat(&ChatRequest {
            messages: vec![Message::system("s"), Message::user("u")],
            schema: Some(json!({"type": "object"})),
            temperature: 0.1,
            max_tokens: 100,
        })
        .unwrap();
    assert_eq!(reply.text, GOOD);
    assert_eq!(reply.output_tokens, Some(90));
    let req = server.join().unwrap();
    assert_eq!(req["model"], "test-model");
    assert_eq!(req["stream"], false);
    assert_eq!(req["format"], json!({"type": "object"}));
    assert_eq!(req["messages"][1]["role"], "user");
    // nothing listening → a clear error, not a hang
    let dead = AiConfig { url: format!("http://127.0.0.1:{port}"), ..AiConfig::default() }.build().unwrap();
    assert!(dead.status().is_err());
}

#[test]
fn log_dates() {
    assert_eq!(log::date(0), (1970, 1, 1));
    assert_eq!(log::date(11016), (2000, 2, 29));
    assert_eq!(log::date(20742), (2026, 10, 16));
}

#[test]
fn bench_references_match_their_expectations() {
    let cases = bench::cases().expect("cases.json parses");
    assert!(cases.len() >= 2);
    let mut ids = std::collections::HashSet::new();
    for c in &cases {
        assert!(ids.insert(c.id.clone()), "duplicate id {}", c.id);
        bench::check_reference(c).unwrap_or_else(|e| panic!("{}: {e}", c.id));
    }
}

#[test]
fn bench_scores_with_an_oracle_and_rejects_wrong_shapes() {
    let cases: Vec<bench::Case> = bench::cases().unwrap().into_iter().take(2).collect();
    let refs: Vec<(String, String)> =
        cases.iter().map(|c| (c.prompt.clone(), serde_json::to_string(&c.reference).unwrap())).collect();
    let oracle = Mock::with(move |req: &ChatRequest| {
        let last = req.messages.last().map(|m| m.content.clone()).unwrap_or_default();
        refs.iter().find(|(p, _)| last.contains(p.as_str())).map(|(_, r)| r.clone()).ok_or("unknown prompt".into())
    });
    let (_, g) = generator(oracle);
    let report = bench::run(&g, &cases, &mut |_, _| {});
    assert_eq!(report.valid_pct, 100.0, "{:#?}", report.cases);
    assert_eq!(report.matched_pct, 100.0, "{:#?}", report.cases);
    assert_eq!(report.rebuilt_pct, 100.0, "{:#?}", report.cases);
    // a valid but wrong part: valid, not matched
    let (_, g) = generator(Mock::with(|_| Ok(GOOD.to_string())));
    let report = bench::run(&g, &cases, &mut |_, _| {});
    assert_eq!(report.valid_pct, 100.0);
    assert_eq!(report.matched_pct, 0.0);
}

/// Failure modes seen in ai-bench with qwen2.5-coder:7b.
#[test]
fn bench_failure_modes_are_handled() {
    // valid JSON followed by junk
    let junk = format!("{GOOD}}}\n// done");
    assert_eq!(plan::parse(&junk).unwrap().summary, "Plate 40x30");
    // feature.edit is not offered while the design is empty
    let s = Session::new();
    assert!(!prompt::system_prompt_for(s.registry(), false).contains("feature.edit"));
    assert!(prompt::system_prompt_for(s.registry(), true).contains("feature.edit"));
    let ids =
        prompt::output_schema_for(s.registry(), false)["properties"]["steps"]["items"]["properties"]["command"]["enum"]
            .clone();
    assert!(!ids.as_array().unwrap().contains(&json!("feature.edit")));
    // a plan with only a sketch is sent back for repair
    let sketch_only = r#"{"summary": "Plate", "steps": [{"command": "sketch.create", "params": {"rectangles": [{"corner": [0, 0], "size": [10, 10]}]}}]}"#;
    let (mock, g) = generator(Mock::replies([sketch_only, GOOD]));
    let p = g
        .generate(&s, &AiRequest { prompt: "plate".into(), use_selection: false }, &mut |_| {}, &AtomicBool::new(false))
        .unwrap();
    assert_eq!(p.attempts.len(), 2);
    let reqs = mock.requests.lock().unwrap();
    assert!(reqs[1].messages.last().unwrap().content.contains("no solid body"));
}

/// The real failure from the Phase 5 acceptance prompt: correct steps, then a repetition loop
/// until the token limit. The complete steps are kept.
#[test]
fn truncated_replies_are_salvaged() {
    let cut = r#"{"parameters":[{"expression":100,"name":"flange_d"},{"expression":10,"name":"flange_t"}],"steps":[{"command":"sketch.create","params":{"circles":[{"center":[0,0],"diameter":"flange_d"}],"plane":"XY"}},{"command":"solid.extrude","params":{"distance":"flange_t"}},{"command":"solid.hole","params":{"diameter":9,"points":[[40,0,"flange_t"],[0,40,"flange_t"]]}},{"command":"solid.hole","params":{"diameter":9,"points":[["40 * cos(0)", "40 * sin(0)", "flange_t"], ["40 * cos(0)", "40 * sin"#;
    let p = plan::salvage(cut).expect("salvaged");
    assert_eq!(p.steps.len(), 3);
    // complete repeated steps after the good ones are dropped too
    let looped = r#"{"steps":[{"command":"solid.box","params":{"size":[10,10,2]}},{"command":"solid.hole","params":{"diameter":3,"points":[[5,5,2]]}},{"command":"solid.hole","params":{"diameter":3,"points":[[1,1,2]]}},{"command":"solid.hole","params":{"diameter":3,"points":[[2,2"#;
    assert_eq!(plan::salvage(looped).map(|p| p.steps.len()), Some(2));
    assert_eq!(p.parameters.len(), 2);
    assert!(plan::salvage("no json").is_none());
    assert!(plan::salvage(r#"{"steps": [{"command": "solid.b"#).is_none());
    // braces inside strings do not confuse it
    let tricky = r#"{"summary": "a } b", "steps": [{"command": "solid.box", "params": {"size": [1, 2, 3]}}, {"command": "solid.box", "params": {"size": [1"#;
    assert_eq!(plan::salvage(tricky).map(|p| p.steps.len()), Some(1));
}
