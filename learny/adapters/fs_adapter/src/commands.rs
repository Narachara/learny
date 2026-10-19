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

#[tauri::command]
pub(crate) async fn record_screen<R: Runtime>(app: AppHandle<R>) -> crate::Result<Option<FileResponse>> {
    app.fs_adapter().record_screen().await
}

/// Stores an in-app microphone recording. The audio arrives as the raw IPC
/// body (no JSON number-array round trip); its container extension rides in
/// the `x-ext` header since a raw body has no room for named arguments.
#[tauri::command]
pub(crate) async fn save_recording<R: Runtime>(
    app: AppHandle<R>,
    request: tauri::ipc::Request<'_>,
) -> crate::Result<FileResponse> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("expected raw audio body".into());
    };
    let ext = request
        .headers()
        .get("x-ext")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    app.fs_adapter().save_recording(bytes.clone(), ext).await
}
