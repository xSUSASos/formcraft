//! Fusion-style user interface on egui. The UI is thin: panels read engine state and act
//! through commands (`Session::execute`).
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod app;
mod dialog;
pub mod icons;
pub mod layout;
mod sketch_mode;
pub mod theme;
mod timeline;
mod toolbar;
mod viewcube;
mod viewport;

pub use app::{AppOptions, FormCraftApp};

/// Open the main window and run until it closes.
pub fn run(options: AppOptions) -> Result<(), String> {
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("FormCraft")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 560.0]),
        renderer: eframe::Renderer::Wgpu,
        persist_window: options.screenshot.is_none(),
        ..Default::default()
    };
    eframe::run_native("FormCraft", native, Box::new(move |cc| Ok(Box::new(FormCraftApp::new(cc, options)))))
        .map_err(|e| e.to_string())
}
