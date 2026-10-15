use std::{cell::RefCell, rc::Rc};
use js_sys::{Array, Promise};
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Blob, BlobEvent, BlobPropertyBag, MediaRecorder, MediaRecorderOptions,
    MediaStream, MediaStreamConstraints, MediaStreamTrack, RecordingState,
};

/// Recording containers to try, best first: (MIME type, file extension).
/// WebKit (the macOS desktop webview, Safari) records MP4/AAC; Chromium and
/// Firefox fall through to WebM or Ogg with Opus.
const FORMATS: [(&str, &str); 3] = [
    ("audio/mp4", "m4a"),
    ("audio/webm;codecs=opus", "webm"),
    ("audio/ogg;codecs=opus", "ogg"),
];

/// A finished take: the encoded audio plus the extension to store it under.
pub struct Recording {
    pub blob: Blob,
    pub ext: &'static str,
}

/// A live microphone recording. Dropping it releases the microphone, so an
/// abandoned take never leaves the OS "mic in use" indicator on.
pub struct AudioRecorder {
    stream: MediaStream,
    recorder: MediaRecorder,
    chunks: Rc<RefCell<Vec<Blob>>>,
    mime: &'static str,
    ext: &'static str,
    _on_data: Closure<dyn FnMut(BlobEvent)>,
}

fn js_err(e: JsValue) -> String {
    e.dyn_ref::<js_sys::Error>()
        .map(|e| String::from(e.message()))
        .or_else(|| e.as_string())
        .unwrap_or_else(|| "unknown error".into())
}

impl AudioRecorder {
    /// Asks for the microphone and starts recording immediately.
    pub async fn start() -> Result<Self, String> {
        let window = web_sys::window().ok_or("no window")?;
        // `mediaDevices` is missing outside a secure context (plain-HTTP web
        // deployments), so report that rather than panicking on it.
        let devices = window
            .navigator()
            .media_devices()
            .map_err(|_| "Microphone access isn't available here.".to_string())?;

        let constraints = MediaStreamConstraints::new();
        constraints.set_audio(&JsValue::TRUE);
        let promise = devices.get_user_media_with_constraints(&constraints).map_err(js_err)?;
        let stream: MediaStream = JsFuture::from(promise).await.map_err(js_err)?.unchecked_into();

        let Some((mime, ext)) = FORMATS.into_iter().find(|(m, _)| MediaRecorder::is_type_supported(m))
        else {
            stop_tracks(&stream);
            return Err("This webview can't record audio in a supported format.".into());
        };

        let options = MediaRecorderOptions::new();
        options.set_mime_type(mime);
        let recorder = match MediaRecorder::new_with_media_stream_and_media_recorder_options(&stream, &options) {
            Ok(r) => r,
            Err(e) => { stop_tracks(&stream); return Err(js_err(e)); }
        };

        let chunks = Rc::new(RefCell::new(Vec::new()));
        let on_data = {
            let chunks = chunks.clone();
            Closure::<dyn FnMut(BlobEvent)>::new(move |e: BlobEvent| {
                if let Some(b) = e.data().filter(|b| b.size() > 0.0) {
                    chunks.borrow_mut().push(b);
                }
            })
        };
        recorder.set_ondataavailable(Some(on_data.as_ref().unchecked_ref()));

        let this = Self { stream, recorder, chunks, mime, ext, _on_data: on_data };
        this.recorder.start().map_err(js_err)?;
        Ok(this)
    }

    /// Stops recording and returns the take.
    pub async fn finish(self) -> Result<Recording, String> {
        // The final `dataavailable` fires before `stop`, so once this resolves
        // every chunk is in.
        let stopped = Promise::new(&mut |resolve, _| {
            self.recorder.set_onstop(Some(&resolve));
        });
        self.recorder.stop().map_err(js_err)?;
        let _ = JsFuture::from(stopped).await;

        let parts: Array = self.chunks.borrow().iter().collect();
        if parts.length() == 0 {
            return Err("No audio was captured — check the input device and try again.".into());
        }
        let bag = BlobPropertyBag::new();
        // Drop the codecs parameter: the blob's type is a plain container type.
        bag.set_type(self.mime.split(';').next().unwrap_or(self.mime));
        let blob = Blob::new_with_blob_sequence_and_options(&parts, &bag).map_err(js_err)?;
        Ok(Recording { blob, ext: self.ext })
    }
}

impl Drop for AudioRecorder {
    fn drop(&mut self) {
        // Detach handlers first: their closures are freed with `self`, and JS
        // calling a dropped closure throws.
        self.recorder.set_ondataavailable(None);
        self.recorder.set_onstop(None);
        if self.recorder.state() != RecordingState::Inactive {
            let _ = self.recorder.stop();
        }
        stop_tracks(&self.stream);
    }
}

fn stop_tracks(stream: &MediaStream) {
    for track in stream.get_tracks().iter() {
        track.unchecked_into::<MediaStreamTrack>().stop();
    }
}
