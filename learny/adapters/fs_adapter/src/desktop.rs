use serde::de::DeserializeOwned;
use tauri::{ plugin::PluginApi, AppHandle, Runtime, Manager };
use std::fs;
use std::path::{ Path, PathBuf };
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