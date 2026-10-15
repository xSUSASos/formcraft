use std::sync::Arc;

use serde_json::json;

use super::*;

fn session() -> Session {
    Session::new()
}

#[test]
fn builtins_are_registered() {
    let mut s = session();
    let v = s.execute("app.commands", &json!({})).unwrap();
    let ids: Vec<&str> = v["commands"].as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&"app.about"));
    assert_eq!(s.execute("app.about", &Value::Null).unwrap()["kernel"], "occt");
}

#[test]
fn runtime_registration_and_unload() {
    let mut s = session();
    let n = s.registry().len();
    let spec = CommandSpec::new("gear.make", "Gear", "Spur gear").source(Source::Plugin("gear".into()));
    s.registry_mut().register(spec, Arc::new(|_, p| Ok(json!({"z": p["z"]})))).unwrap();
    assert_eq!(s.execute("gear.make", &json!({"z": 20})).unwrap()["z"], 20);
    assert_eq!(s.registry().search("gea").first().map(|c| c.id.as_str()), Some("gear.make"));
    assert_eq!(s.registry_mut().unregister_source(&Source::Plugin("gear".into())), vec!["gear.make".to_string()]);
    assert_eq!(s.registry().len(), n);
    assert!(matches!(s.execute("gear.make", &json!({})), Err(EngineError::UnknownCommand(_))));
}

#[test]
fn registration_is_validated() {
    let mut r = Registry::default();
    let h: Handler = Arc::new(|_, _| Ok(Value::Null));
    assert!(r.register(CommandSpec::new("Bad Id", "x", ""), h.clone()).is_err());
    assert!(r.register(CommandSpec::new("a..b", "x", ""), h.clone()).is_err());
    assert!(r.register(CommandSpec::new("ok.id", "x", "").params(json!([])), h.clone()).is_err());
    r.register(CommandSpec::new("ok.id", "x", ""), h.clone()).unwrap();
    assert!(r.register(CommandSpec::new("ok.id", "dup", ""), h).is_err(), "duplicates are refused");
}

#[test]
fn panicking_command_is_contained() {
    let mut s = session();
    s.registry_mut()
        .register(CommandSpec::new("test.boom", "Boom", ""), Arc::new(|_, _| -> Result<Value> { panic!("boom") }))
        .unwrap();
    let e = s.execute("test.boom", &json!({})).unwrap_err();
    assert!(matches!(e, EngineError::Internal(_, ref m) if m == "boom"), "{e:?}");
    assert!(!s.log().last().unwrap().ok);
    // still usable
    assert!(s.execute("app.about", &json!({})).is_ok());
}

#[test]
fn params_must_be_an_object() {
    let mut s = session();
    assert!(matches!(s.execute("app.about", &json!([1, 2])), Err(EngineError::InvalidParams(_))));
}

#[test]
fn search_ranks_label_prefix_first() {
    let mut r = Registry::default();
    let h: Handler = Arc::new(|_, _| Ok(Value::Null));
    r.register(CommandSpec::new("solid.extrude", "Extrude", "Push a profile into a body"), h.clone()).unwrap();
    r.register(CommandSpec::new("solid.fillet", "Fillet", "Round edges, e.g. after extrude"), h).unwrap();
    let hits: Vec<&str> = r.search("ext").iter().map(|c| c.id.as_str()).collect();
    assert_eq!(hits, vec!["solid.extrude", "solid.fillet"]);
    assert!(r.search("   ").is_empty());
}
