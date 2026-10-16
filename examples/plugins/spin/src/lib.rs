//! A plugin that misbehaves on purpose (tests of the host's sandbox and limits).

wit_bindgen::generate!({
    path: "../../../crates/plugin-api/wit",
    world: "plugin",
});

use formcraft::plugin::host::execute;

struct Spin;

fn cmd(id: &str, label: &str) -> CommandSpec {
    CommandSpec {
        id: id.into(),
        label: label.into(),
        description: label.into(),
        params_schema: "{\"type\": \"object\"}".into(),
        placement: None,
        shortcut: None,
        ai: false,
    }
}

impl Guest for Spin {
    fn init() -> Result<(Vec<CommandSpec>, Vec<FeatureSpec>), String> {
        Ok((
            vec![
                cmd("spin.forever", "Loop forever"),
                cmd("spin.hog", "Allocate 2 GB"),
                cmd("spin.panic", "Panic"),
                cmd("spin.box", "Make a box through host.execute"),
            ],
            Vec::new(),
        ))
    }

    fn run_command(id: String, _params: String) -> Result<String, String> {
        match id.as_str() {
            "spin.forever" => {
                let mut x: u64 = 0;
                loop {
                    x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
                    if x == 42 {
                        return Ok(x.to_string());
                    }
                }
            }
            "spin.hog" => {
                let v: Vec<u8> = vec![7; 2 << 30];
                Ok(v.len().to_string())
            }
            "spin.panic" => panic!("boom"),
            "spin.box" => execute("solid.box", "{\"size\": [10, 10, 10]}"),
            other => Err(format!("unknown command {other}")),
        }
    }

    fn regenerate(kind: String, _params: String) -> Result<Solid, String> {
        Err(format!("no feature {kind}"))
    }
}

export!(Spin);
