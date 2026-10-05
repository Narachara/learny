use tauri::{AppHandle, Runtime };
use crate::FsAdapterExt;
use crate::FileResponse;


#[tauri::command]
pub(crate) async fn pick_image<R: Runtime>(
    app: AppHandle<R>,
) -> crate::Result<Option<FileResponse>> {
    app.fs_adapter().pick_image().await
}

#[tauri::command]
pub(crate) async fn pick_audio<R: Runtime>(
    app: AppHandle<R>,
) -> crate::Result<Option<FileResponse>> {
    app.fs_adapter().pick_audio().await
}

#[tauri::command]
pub(crate) async fn pick_video<R: Runtime>(
    app: AppHandle<R>,
) -> crate::Result<Option<FileResponse>> {
    app.fs_adapter().pick_video().await
}

#[tauri::command]
pub(crate) async fn pick_archive<R: Runtime>(
    app: AppHandle<R>,
) -> crate::Result<Option<FileResponse>> {
    app.fs_adapter().pick_archive().await
}