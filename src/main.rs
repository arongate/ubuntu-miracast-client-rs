//! Ubuntu Miracast Client — Rust implementation
//!
//! A desktop application for screen and application casting to Miracast-compatible
//! devices using Wi-Fi Direct, RTSP/WFD negotiation, and GStreamer streaming.

// Allow dead code during early development — modules are defined but not fully wired yet.
#![allow(dead_code)]

mod config;
mod discovery;
mod rtsp;
mod streaming;
mod ui;
mod wfd;

use gtk4::prelude::*;
use libadwaita as adw;
use tracing_subscriber::EnvFilter;

const APP_ID: &str = "com.github.eddypepy.MiracastClient";

fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tracing::info!("Starting Miracast Client v{}", env!("CARGO_PKG_VERSION"));

    // Initialize GStreamer
    gstreamer::init()?;

    // Create and run GTK application
    let app = adw::Application::builder().application_id(APP_ID).build();

    app.connect_activate(ui::build_ui);

    let exit_code = app.run();
    std::process::exit(exit_code.into());
}
