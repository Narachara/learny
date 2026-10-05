/// Unified API surface for UI components.
/// Exactly one feature flag must be enabled at compile time.
///
///   cargo build --features tauri   → Tauri invoke bridge (desktop)
///   cargo build --features web     → HTTP fetch bridge (web server)
///
/// Components import from `crate::api` and never reference
/// `tauri_api` or `web_api` directly.

#[cfg(feature = "tauri")]
pub use crate::tauri_api::*;

#[cfg(feature = "server")]
pub use crate::web_api::*;


#[cfg(not(any(feature = "tauri", feature = "server")))]
compile_error!("Enable exactly one feature: `tauri` or `server`");

#[cfg(all(feature = "tauri", feature = "server"))]
compile_error!("Features `tauri` and `server` are mutually exclusive");
