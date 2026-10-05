use tauri::{
  plugin::{Builder, TauriPlugin},
  Manager, Runtime,
};

use serde::{Deserialize, Serialize};

/// DTO returned by the file-picker commands.
/// Serialised to JSON on the native side, deserialised by the UI.
#[derive(Debug, Serialize, Deserialize)]
pub struct FileResponse {
    pub path: String,
}

mod desktop;

mod commands;
mod error;

pub use error::{Error, Result};

use desktop::FsAdapter;

/// Extensions to [`tauri::App`], [`tauri::AppHandle`] and [`tauri::Window`]
/// to access the fs_adapter APIs.
pub trait FsAdapterExt<R: Runtime> {
  fn fs_adapter(&self) -> &FsAdapter<R>;
}

impl<R: Runtime, T: Manager<R>> FsAdapterExt<R> for T {
  fn fs_adapter(&self) -> &FsAdapter<R> {
    self.state::<FsAdapter<R>>().inner()
  }
}

/// Initializes the plugin.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
  Builder::new("fs-adapter")
    .invoke_handler(tauri::generate_handler![
      commands::pick_image,
      commands::pick_audio,
      commands::pick_video,
      commands::pick_archive
    ])
    .setup(|app, api| {
      let adapter = desktop::init(app, api)?;

      app.manage(adapter);
      Ok(())
    })
    .build()
}