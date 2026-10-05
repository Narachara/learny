use axum::{
    body::Body,
    extract::{Extension, Multipart, Path},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};

/// 50 MB upload limit — applied per-route on the upload endpoint.
pub const UPLOAD_LIMIT: usize = 50 * 1024 * 1024;
use serde::Serialize;
use std::path::PathBuf;
use tokio::fs;
use uuid::Uuid;

/// Shared app-data directory, injected via Extension.
pub type DataDir = std::sync::Arc<PathBuf>;

// ---------------------------------------------------------------------------
// POST /api/files/upload
// Accepts a single file field ("file").
// Saves it under <app_data>/files/<uuid>.<ext> and returns the virtual path.
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct UploadResponse {
    pub path: String, // e.g. "files/uuid.png"
}

/// Content images (as opposed to fixed-size covers) keep their aspect ratio
/// but are capped at this many pixels on the long edge — plenty for any
/// on-screen display, and far smaller on disk than an uncompressed phone photo.
const MAX_CONTENT_DIM: u32 = 1600;

/// Downscale an oversized image to fit within `MAX_CONTENT_DIM`, re-encoding
/// in its original format. Returns `None` if the file isn't a recognised
/// image, is already small enough, or is a GIF (resizing would flatten any
/// animation, so animated GIFs are left untouched).
fn resize_if_oversized(bytes: &[u8], ext: &str) -> Option<Vec<u8>> {
    if ext == "gif" {
        return None;
    }
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

pub async fn upload(
    Extension(data_dir): Extension<DataDir>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None)    => break,
            Err(e)      => return (StatusCode::BAD_REQUEST, format!("multipart error: {e}")).into_response(),
        };
        let file_name = field
            .file_name()
            .unwrap_or("upload")
            .to_string();

        let ext = std::path::Path::new(&file_name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("bin")
            .to_lowercase();

        let uuid      = Uuid::new_v4().to_string();
        let stored    = format!("files/{}.{}", uuid, ext);
        let full_path = data_dir.join(&stored);

        // Ensure the files/ subdirectory exists.
        if let Some(parent) = full_path.parent() {
            if let Err(e) = fs::create_dir_all(parent).await {
                return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
            }
        }

        let bytes = match field.bytes().await {
            Ok(b)  => b,
            Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
        };

        // Non-image uploads (audio, video, archives, …) fail to decode and
        // pass through unchanged; oversized images are downscaled first.
        let raw = bytes.to_vec();
        let bytes = match tokio::task::spawn_blocking(move || {
            let resized = resize_if_oversized(&raw, &ext);
            resized.unwrap_or(raw)
        }).await {
            Ok(b)  => b,
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        };

        if let Err(e) = fs::write(&full_path, &bytes).await {
            return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
        }

        return (StatusCode::CREATED, Json(UploadResponse { path: stored })).into_response();
    }

    (StatusCode::BAD_REQUEST, "no file field in multipart").into_response()
}

// ---------------------------------------------------------------------------
// POST /api/files/upload-cover
// Same as upload but crops + resizes the image to COVER_W × COVER_H (JPEG).
// Used for deck and card cover images so stored files are always compact.
// ---------------------------------------------------------------------------

const COVER_W: u32 = 640;
const COVER_H: u32 = 320;

fn resize_to_cover(img: image::DynamicImage) -> Result<Vec<u8>, String> {
    use image::ImageEncoder;

    let (sw, sh) = (img.width(), img.height());
    let scale = f64::max(COVER_W as f64 / sw as f64, COVER_H as f64 / sh as f64);
    let nw = ((sw as f64 * scale).ceil() as u32).max(COVER_W);
    let nh = ((sh as f64 * scale).ceil() as u32).max(COVER_H);
    let resized = img.resize_exact(nw, nh, image::imageops::FilterType::Lanczos3);
    let x = nw.saturating_sub(COVER_W) / 2;
    let y = nh.saturating_sub(COVER_H) / 2;
    let cropped = resized.crop_imm(x, y, COVER_W, COVER_H);

    // Convert to RGB8 — JPEG does not support alpha channels.
    let rgb = cropped.into_rgb8();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85)
        .write_image(rgb.as_raw(), COVER_W, COVER_H, image::ExtendedColorType::Rgb8)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

pub async fn upload_cover(
    Extension(data_dir): Extension<DataDir>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None)    => break,
            Err(e)      => return (StatusCode::BAD_REQUEST, format!("multipart error: {e}")).into_response(),
        };

        let bytes = match field.bytes().await {
            Ok(b)  => b,
            Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
        };

        let img = match image::load_from_memory(&bytes) {
            Ok(i)  => i,
            Err(e) => return (StatusCode::BAD_REQUEST, format!("not a valid image: {e}")).into_response(),
        };

        let jpeg_bytes = match tokio::task::spawn_blocking(move || resize_to_cover(img)).await {
            Ok(Ok(b))  => b,
            Ok(Err(e)) => return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
            Err(e)     => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        };

        let stored    = format!("files/{}.jpg", Uuid::new_v4());
        let full_path = data_dir.join(&stored);

        if let Some(parent) = full_path.parent() {
            if let Err(e) = fs::create_dir_all(parent).await {
                return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
            }
        }

        if let Err(e) = fs::write(&full_path, &jpeg_bytes).await {
            return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
        }

        return (StatusCode::CREATED, Json(UploadResponse { path: stored })).into_response();
    }

    (StatusCode::BAD_REQUEST, "no file field in multipart").into_response()
}

// ---------------------------------------------------------------------------
// GET /files/*path
// Serves a file from the app-data directory.
// ---------------------------------------------------------------------------

pub async fn serve(
    Extension(data_dir): Extension<DataDir>,
    Path(virtual_path): Path<String>,
) -> impl IntoResponse {
    // Route is /files/*path so virtual_path is "uuid.ext"; files live under data_dir/files/
    let full_path = data_dir.join("files").join(&virtual_path);

    let bytes = match fs::read(&full_path).await {
        Ok(b)  => b,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };

    let mime = mime_guess::from_path(&full_path)
        .first_or_octet_stream()
        .to_string();

    (
        [(header::CONTENT_TYPE, mime)],
        Body::from(bytes),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// DELETE /api/files/*path
// ---------------------------------------------------------------------------

pub async fn delete(
    Extension(data_dir): Extension<DataDir>,
    Path(virtual_path): Path<String>,
) -> impl IntoResponse {
    let full_path = data_dir.join(&virtual_path);

    if !full_path.exists() {
        return StatusCode::NO_CONTENT.into_response();
    }

    match fs::remove_file(&full_path).await {
        Ok(())   => StatusCode::NO_CONTENT.into_response(),
        Err(e)   => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
