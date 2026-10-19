use serde::de::DeserializeOwned;
use tauri::{ plugin::PluginApi, AppHandle, Runtime, Manager };
use std::fs;
use std::path::{ Path, PathBuf };
use std::process::Command;
use std::time::{Duration, Instant, SystemTime};
use futures::channel::oneshot;
use tauri_plugin_dialog::{ DialogExt, FileDialogBuilder, FilePath };
use crate::FileResponse;
use uuid::Uuid;

pub fn init<R: Runtime, C: DeserializeOwned>(
    app: &AppHandle<R>,
    _api: PluginApi<R, C>
) -> crate::Result<FsAdapter<R>> {
    Ok(FsAdapter(app.clone()))
}

pub enum PickKind {
    Image,
    Audio,
    Video,
    Archive,
}

impl PickKind {
    fn dialog_filter(&self) -> (&'static str, &'static [&'static str]) {
        match self {
            PickKind::Image   => ("Images",   &["png", "jpg", "jpeg", "webp"]),
            PickKind::Audio   => ("Audio",    &["mp3", "m4a", "wav", "ogg", "aac"]),
            PickKind::Video   => ("Video",    &["mp4", "mov", "m4v", "webm"]),
            PickKind::Archive => ("Archives", &["zip", "tar", "gz", "7z"]),
        }
    }

    fn default_extension(&self, picked: &Path) -> String {
        picked
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or(match self {
                PickKind::Image   => "png",
                PickKind::Audio   => "mp3",
                PickKind::Video   => "mp4",
                PickKind::Archive => "zip",
            })
            .to_string()
    }
}


/// Copies a picked file into app data. On macOS this first attempts an APFS
/// clonefile — constant-time and no extra disk space, so attaching even a
/// multi-hundred-MB video is instant — falling back to a byte copy when
/// cloning isn't possible (non-APFS volume, cross-volume, etc.).
/// Content images are capped at this many pixels on the long edge — plenty
/// for on-screen display, and far smaller on disk than an uncompressed photo
/// straight off a phone.
const MAX_CONTENT_DIM: u32 = 1600;

/// Downscale an oversized image to fit within `MAX_CONTENT_DIM`, re-encoding
/// in its original format. Returns `None` if it's already small enough (the
/// caller falls back to a byte-for-byte copy).
fn resize_if_oversized(bytes: &[u8], ext: &str) -> Option<Vec<u8>> {
    let img = image::load_from_memory(bytes).ok()?;
    let (w, h) = (img.width(), img.height());
    if w.max(h) <= MAX_CONTENT_DIM {
        return None;
    }
    let scale = MAX_CONTENT_DIM as f64 / w.max(h) as f64;
    let nw = ((w as f64 * scale).round() as u32).max(1);
    let nh = ((h as f64 * scale).round() as u32).max(1);
    let resized = img.resize(nw, nh, image::imageops::FilterType::Lanczos3);

    let format = image::ImageFormat::from_extension(ext)?;
    let mut out = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut out);
    let ok = if format == image::ImageFormat::Jpeg {
        use image::ImageEncoder;
        let rgb = resized.into_rgb8();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, 85)
            .write_image(rgb.as_raw(), nw, nh, image::ExtendedColorType::Rgb8)
            .is_ok()
    } else {
        resized.write_to(&mut cursor, format).is_ok()
    };
    ok.then_some(out)
}

fn copy_asset(src: &Path, dst: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::ffi::OsStrExt;
        if let (Ok(c_src), Ok(c_dst)) = (
            std::ffi::CString::new(src.as_os_str().as_bytes()),
            std::ffi::CString::new(dst.as_os_str().as_bytes()),
        ) {
            if unsafe { libc::clonefile(c_src.as_ptr(), c_dst.as_ptr(), 0) } == 0 {
                return Ok(());
            }
        }
    }
    fs::copy(src, dst).map(|_| ())
}

/// Access to the fs_adapter APIs.
pub struct FsAdapter<R: Runtime>(pub AppHandle<R>);

impl<R: Runtime> FsAdapter<R> {
    pub async fn pick_file(
        &self,
        kind: PickKind,
    ) -> crate::Result<Option<FileResponse>> {
        let app = self.0.clone();

        let picked_path: Option<PathBuf> = {
            let (tx, rx) = oneshot::channel();

            let (label, extensions) = kind.dialog_filter();

            FileDialogBuilder::new(app.dialog().clone())
                .add_filter(label, extensions)
                .pick_file(move |file| {
                    let _ = tx.send(file);
                });

            match rx.await? {
                Some(FilePath::Path(path)) => Some(path),
                Some(_) => {
                    return Err("Unsupported file path type".into());
                }
                None => None, // user cancelled
            }
        };

        let Some(picked_path) = picked_path else {
            return Ok(None);
        };

        // --- app data dir ---
        let app_data_dir = app.path().app_data_dir()?;
        let files_dir = app_data_dir.join("files");
        fs::create_dir_all(&files_dir)?;

        let extension = kind.default_extension(&picked_path);
        let file_name = format!("{}.{}", Uuid::new_v4(), extension);
        let target_path = files_dir.join(&file_name);

        // Off the async runtime: a large fallback copy (or image decode/resize)
        // must not stall other pending commands while it runs.
        let is_image = matches!(kind, PickKind::Image);
        {
            let target_path = target_path.clone();
            let extension = extension.clone();
            tauri::async_runtime::spawn_blocking(move || -> std::io::Result<()> {
                if is_image {
                    let bytes = fs::read(&picked_path)?;
                    match resize_if_oversized(&bytes, &extension) {
                        Some(resized) => return fs::write(&target_path, resized),
                        None => return fs::write(&target_path, bytes),
                    }
                }
                copy_asset(&picked_path, &target_path)
            })
            .await
            .map_err(tauri::Error::from)??;
        }

        let virtual_path = format!("files/{}", file_name);

        Ok(Some(FileResponse {
            path: virtual_path,
        }))
    }

    pub async fn pick_image(&self) -> crate::Result<Option<FileResponse>> {
        self.pick_file(PickKind::Image).await
    }

    pub async fn pick_audio(&self) -> crate::Result<Option<FileResponse>> {
        self.pick_file(PickKind::Audio).await
    }

    pub async fn pick_video(&self) -> crate::Result<Option<FileResponse>> {
        self.pick_file(PickKind::Video).await
    }

    pub async fn pick_archive(&self) -> crate::Result<Option<FileResponse>> {
        self.pick_file(PickKind::Archive).await
    }

    /// Writes recorded audio bytes into app data. Only the containers the
    /// webview's MediaRecorder can produce are accepted, so the extension
    /// can't be used to smuggle an arbitrary file name in.
    pub async fn save_recording(
        &self,
        bytes: Vec<u8>,
        ext: &str,
    ) -> crate::Result<FileResponse> {
        if !matches!(ext, "m4a" | "webm" | "ogg") {
            return Err(crate::Error::StringError(format!("unsupported recording format: {ext:?}")));
        }

        let files_dir = self.0.path().app_data_dir()?.join("files");
        fs::create_dir_all(&files_dir)?;

        let file_name = format!("{}.{}", Uuid::new_v4(), ext);
        let target_path = files_dir.join(&file_name);
        tauri::async_runtime::spawn_blocking(move || fs::write(&target_path, bytes))
            .await
            .map_err(tauri::Error::from)??;

        Ok(FileResponse { path: format!("files/{}", file_name) })
    }

    /// Uses macOS's genuine area/window selection and recording toolbar, then
    /// converts the result to a compact 960x540 M4V for a card video block.
    pub async fn record_screen(&self) -> crate::Result<Option<FileResponse>> {
        #[cfg(not(target_os = "macos"))]
        return Err(crate::Error::StringError("Screen recording is currently available on macOS only.".into()));

        #[cfg(target_os = "macos")]
        {
            let files_dir = self.0.path().app_data_dir()?.join("files");
            fs::create_dir_all(&files_dir)?;
            let id = Uuid::new_v4();
            let raw_name = format!("{id}.recording.mov");
            let raw_path = files_dir.join(&raw_name);
            let final_name = format!("{id}.m4v");
            let final_path = files_dir.join(&final_name);
            let downloads_dir = self.0.path().download_dir()?;
            let started_at = SystemTime::now();

            tauri::async_runtime::spawn_blocking(move || -> crate::Result<Option<FileResponse>> {
                // The toolbar is the only reliable native video selection UI
                // on current macOS. It saves to the user's configured location
                // (Downloads here), so we import the newly-created movie below.
                let mut capture = Command::new("/usr/sbin/screencapture")
                    .args(["-i", "-J", "video", "-v", "-g", "-U", "-x"])
                    .arg(&raw_path)
                    .spawn()
                    .map_err(|e| crate::Error::StringError(format!("Couldn't start macOS screen recording: {e}")))?;

                // `-U` ignores the output argument. Copy only a movie created
                // just after this request, and never remove the user's original
                // Downloads copy. Do not wait on the toolbar launcher: it can
                // outlive the saved recording and otherwise holds the editor's
                // insertion task open indefinitely.
                if fs::metadata(&raw_path).map(|m| m.len()).unwrap_or(0) == 0 {
                    let deadline = Instant::now() + Duration::from_secs(60 * 60);
                    let source = loop {
                        let earliest = started_at.checked_sub(Duration::from_secs(5)).unwrap_or(started_at);
                        let settled_before = SystemTime::now()
                            .checked_sub(Duration::from_secs(1))
                            .unwrap_or(started_at);
                        let newest = fs::read_dir(&downloads_dir).ok().into_iter().flatten()
                        .filter_map(|entry| entry.ok())
                        .filter_map(|entry| {
                            let path = entry.path();
                            let ext = path.extension()?.to_str()?.to_ascii_lowercase();
                            matches!(ext.as_str(), "mov" | "m4v" | "mp4").then_some(path)
                        })
                        .filter_map(|path| {
                            let modified = fs::metadata(&path).ok()?.modified().ok()?;
                            (modified >= earliest && modified <= settled_before).then_some((modified, path))
                        })
                        .max_by_key(|(modified, _)| *modified)
                        .map(|(_, path)| path);
                        if let Some(source) = newest { break source; }
                        if Instant::now() >= deadline {
                            let _ = capture.kill();
                            return Ok(None);
                        }
                        std::thread::sleep(Duration::from_millis(500));
                    };
                    copy_asset(&source, &raw_path)?;
                    if capture.try_wait()?.is_none() {
                        let _ = capture.kill();
                        let _ = capture.wait();
                    }
                }

                let converted = Command::new("/usr/bin/avconvert")
                    .args(["--source"]).arg(&raw_path)
                    .args(["--preset", "Preset960x540", "--output"]).arg(&final_path)
                    .args(["--replace", "--disableMetadataFilter"])
                    .status()
                    .ok();
                if !converted.is_some_and(|status| status.success())
                    || fs::metadata(&final_path).map(|m| m.len()).unwrap_or(0) == 0 {
                    let _ = fs::remove_file(&final_path);
                    // A failed optional compression pass must not turn a
                    // completed recording into a lost take. Keep the movie;
                    // it remains playable and can be recompressed later.
                    return Ok(Some(FileResponse { path: format!("files/{raw_name}") }));
                }
                let _ = fs::remove_file(&raw_path);
                Ok(Some(FileResponse { path: format!("files/{final_name}") }))
            }).await.map_err(tauri::Error::from)?
        }
    }
    

    pub async fn pick_import_file(
        &self,
    ) -> crate::Result<Option<Vec<u8>>> {
        let app = self.0.clone();
        let (tx, rx) = oneshot::channel();

        FileDialogBuilder::new(app.dialog().clone())
            .pick_file(move |file| {
                let _ = tx.send(file);
            });

        let Some(FilePath::Path(path)) = rx.await?
        else {
            return Ok(None);
        };

        let bytes = fs::read(path)?;
        Ok(Some(bytes))
    }
}
