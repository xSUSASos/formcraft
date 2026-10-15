# FormCraft — instructions for agents

FormCraft is a clean-room parametric 3D CAD in Rust with a Fusion 360-style interface and a local
AI built into the workflow. Plan: `PLAN.md`. State and next task: `PROGRESS.md` (read it first).
Decisions: `docs/adr/`. Architecture and layering: `docs/architecture.md`.

## Start every session here
1. Read `PROGRESS.md` («Сейчас»), then the current phase in `PLAN.md`.
2. Work in 1–3 h steps; each ends green (`cargo xtask ci`) and is one conventional commit.
3. Update `PROGRESS.md` after every step. Decide yourself; record decisions as ADRs.

## Build (Windows dev machine)
- Default `target/` works (OCCT builds there). The repo path contains Cyrillic: FreeCADCmd,
  `ollama create -f` and cmd.exe batch files fail on it — use ASCII temp dirs for those.
- The first build compiles OpenCascade (30–60 min); later builds are incremental.
- STEP tests live in `crates/occt/tests/step.rs` (`harness = false`): libtest binaries crash at
  exit after STEP transfers, plain programs don't.
- `cargo xtask ci` = fmt, clippy -D warnings, tests, layers, cargo deny.

## Never crash
- No `unwrap/expect/panic!/todo!/unimplemented!/unreachable!` in non-test code (crate roots
  `#![deny(...)]`), `unsafe_code = "forbid"` except the C++ bridge in `crates/occt`.
- Every OCCT call goes through `cpp/bridge.cxx` inside `FC_GUARD` (C++ exception → `Err`).
- Input-derived numbers are hostile: validate (finite, positive, in range), `get()` not `[i]`.
- Every crash fix lands with a regression test (`hostile_inputs_are_errors_not_aborts`).

## Non-negotiables
- **Clean-room.** Fusion 360 layout and workflow only. No Autodesk code, icons, texts or assets;
  icons are drawn in code. No ArtCraft brand.
- **Everything is a command** with a JSON schema in the engine's runtime registry (UI, shortcut
  box, AI, MCP, CLI, plugins all call the same commands). Never hardcode command lists.
- **Layering** (`cargo xtask layers`): kernel/plugin-api → occt/sketch → doc → regen/tess → io →
  engine → ai/plugin/render → ui-egui/mcp → apps. Nothing below the UI layer uses egui/eframe/rfd.
- **Licenses:** MIT/Apache-2.0/BSD-family only; LGPL only for the kernel build (`occt-sys`).
- Ideas outside the MVP go to `docs/ideas.md`.

## Delegation
Simple, self-contained work (template tests, docs/learn, localisation, icons by spec, simple
panels by spec, few-shot examples) goes to Sonnet (`delegate-to-sonnet` skill). Review every
result with `cargo xtask ci` and the diff; log it in `PROGRESS.md`.
