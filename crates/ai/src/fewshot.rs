//! Worked examples (request → plan) shown to the model before the real request. They live in
//! `crates/ai/fewshot/*.json` and are embedded at build time; the closest ones to the request
//! are picked by word overlap. A test runs every example's plan on a real session.

use serde::Deserialize;

use crate::plan::Plan;

include!(concat!(env!("OUT_DIR"), "/fewshot_files.rs"));

#[derive(Debug, Clone, Deserialize)]
pub struct Example {
    #[serde(skip)]
    pub name: String,
    pub prompt: String,
    pub plan: Plan,
}

pub fn all() -> Vec<Example> {
    FILES
        .iter()
        .filter_map(|(name, text)| match serde_json::from_str::<Example>(text) {
            Ok(mut e) => {
                e.name = (*name).to_string();
                Some(e)
            }
            Err(e) => {
                log::error!("few-shot example {name}: {e}");
                None
            }
        })
        .collect()
}

fn words(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 3 && !w.chars().all(|c| c.is_ascii_digit()))
        .map(str::to_string)
        .collect()
}

/// The `k` examples sharing the most words with `prompt` (ties: file order).
pub fn pick(prompt: &str, k: usize) -> Vec<Example> {
    let want = words(prompt);
    let mut scored: Vec<(usize, Example)> = all()
        .into_iter()
        .map(|e| {
            let have = words(&format!("{} {}", e.prompt, e.plan.summary));
            (want.iter().filter(|w| have.contains(w)).count(), e)
        })
        .collect();
    scored.sort_by_key(|s| std::cmp::Reverse(s.0));
    scored.into_iter().take(k).map(|(_, e)| e).collect()
}
