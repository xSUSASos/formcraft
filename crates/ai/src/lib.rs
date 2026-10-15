//! Built-in AI: LLM backends, prompts generated from the command registry, validation and repair loop
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod backend;
pub mod commands;
pub mod fewshot;
pub mod http;
pub mod log;
pub mod pipeline;
pub mod plan;
pub mod prompt;

use std::sync::Arc;

pub use backend::{AiConfig, LlmBackend, Mock};
pub use pipeline::{AiError, AiRequest, Generator, Progress, Proposal};
pub use plan::Plan;

/// Register the assistant's commands (`ai.generate`, `ai.explain`, `ai.status`) with the
/// backend from `config`. A bad config is reported, the rest of the app works without AI.
pub fn install(s: &mut formcraft_engine::Session, config: &AiConfig) -> Result<Arc<Generator>, String> {
    let generator = Arc::new(Generator::new(config.build()?));
    commands::register(s, generator.clone()).map_err(|e| e.to_string())?;
    Ok(generator)
}

#[cfg(test)]
mod tests;
