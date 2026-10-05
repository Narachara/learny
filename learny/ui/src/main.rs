mod components;
mod app;
pub mod api;

#[cfg(feature = "tauri")]
mod tauri_api;

#[cfg(feature = "server")]
mod web_api;

use app::App;
use dioxus::prelude::*;
use dioxus_logger::tracing::Level;

fn main() {
    dioxus_logger::init(Level::INFO).unwrap();
    launch(App);
}