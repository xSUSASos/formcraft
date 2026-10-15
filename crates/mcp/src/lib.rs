//! FormCraft MCP server: JSON-RPC 2.0 over stdio (newline-delimited). Tools reach an in-process
//! headless session or a running app through its loopback control channel. Agents build models
//! with the same commands as the UI, inspect exact geometry and look at PNG renders.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod backend;
mod server;
mod tools;

pub use backend::{Backend, Headless, Remote};
pub use server::{PROTOCOL_VERSION, Server};

#[cfg(test)]
mod tests;
