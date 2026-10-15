//! Built-in commands registered at startup. Modelling commands arrive with Phases 2–3.

use std::sync::Arc;

use serde_json::{Value, json};

use crate::{CommandSpec, Registry, Session};

pub(crate) fn register(r: &mut Registry) {
    let cmds: Vec<(CommandSpec, crate::Handler)> = vec![
        (
            CommandSpec::new("app.commands", "List Commands", "All registered commands with their parameter schemas.")
                .params(json!({"type": "object", "properties": {"filter": {"type": "string"}}})),
            Arc::new(|s: &mut Session, p: &Value| {
                let filter = p.get("filter").and_then(Value::as_str).unwrap_or("").to_lowercase();
                let list: Vec<&CommandSpec> = s
                    .registry()
                    .specs()
                    .filter(|c| filter.is_empty() || c.id.contains(&filter) || c.label.to_lowercase().contains(&filter))
                    .collect();
                Ok(json!({ "commands": list }))
            }),
        ),
        (
            CommandSpec::new("app.about", "About FormCraft", "Version and geometry kernel."),
            Arc::new(|s: &mut Session, _: &Value| {
                Ok(json!({
                    "name": "FormCraft",
                    "version": env!("CARGO_PKG_VERSION"),
                    "kernel": s.kernel().name(),
                    "message": format!("FormCraft {} ({})", env!("CARGO_PKG_VERSION"), s.kernel().name()),
                }))
            }),
        ),
    ];
    for (spec, h) in cmds {
        if let Err(e) = r.register(spec, h) {
            log::error!("built-in command failed to register: {e}");
        }
    }
}
