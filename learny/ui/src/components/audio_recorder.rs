use dioxus::prelude::*;
use dioxus::document::Stylesheet;
use gloo_timers::future::TimeoutFuture;

use crate::recorder::{AudioRecorder, Recording};

static MEDIA_CSS: Asset = asset!("/assets/media_upload.css");
static CSS: Asset = asset!("/assets/audio_recorder.css");

/// Monochrome microphone glyph, sized and colored like the text glyphs used
/// for the other block buttons (an emoji would render in full color).
#[component]
pub fn MicIcon() -> Element {
    rsx! {
        svg {
            class: "mic-icon",
            width: "1em", height: "1em", view_box: "0 0 24 24",
            fill: "none", stroke: "currentColor", stroke_width: "2",
            stroke_linecap: "round", stroke_linejoin: "round",
            "aria-hidden": "true",
            rect { x: "9", y: "3", width: "6", height: "11", rx: "3" }
            path { d: "M5 11a7 7 0 0 0 14 0" }
            path { d: "M12 18v3" }
        }
    }
}

/// Modal that records from the microphone as soon as it opens.
///
/// `on_done` gets the take on Stop, or None on Cancel / failure. Unmounting
/// drops the recorder, which releases the microphone either way.
#[component]
pub fn AudioRecorderOverlay(on_done: EventHandler<Option<Recording>>) -> Element {
    let mut recorder = use_signal(|| None::<AudioRecorder>);
    let mut error = use_signal(|| None::<String>);
    let mut elapsed = use_signal(|| 0u32);
    let mut stopping = use_signal(|| false);

    use_hook(|| {
        spawn(async move {
            match AudioRecorder::start().await {
                Ok(r) => {
                    recorder.set(Some(r));
                    loop {
                        TimeoutFuture::new(1000).await;
                        elapsed += 1;
                    }
                }
                Err(e) => error.set(Some(e)),
            }
        });
    });

    let stop = move |_| {
        let Some(r) = recorder.write().take() else { return };
        stopping.set(true);
        spawn(async move {
            match r.finish().await {
                Ok(take) => on_done.call(Some(take)),
                Err(e) => { stopping.set(false); error.set(Some(e)); }
            }
        });
    };
    let cancel = move |_| {
        recorder.write().take();
        on_done.call(None);
    };

    let is_recording = recorder.read().is_some();
    let secs = elapsed();
    let (title, hint) = match (error.read().clone(), is_recording, stopping()) {
        (Some(e), _, _) => ("Can't record".to_string(), e),
        (None, _, true) => ("Finishing…".to_string(), String::new()),
        (None, true, _) => (format!("Recording  {}:{:02}", secs / 60, secs % 60), "Stop to add it as an audio block.".to_string()),
        (None, false, _) => ("Waiting for microphone…".to_string(), "Allow microphone access to start recording.".to_string()),
    };

    rsx! {
        Stylesheet { href: MEDIA_CSS }
        Stylesheet { href: CSS }
        div { class: "media-import-overlay", role: "dialog", aria_label: "Record audio",
            div { class: "media-import-card",
                div { class: if is_recording { "media-import-glyph recorder-live" } else { "media-import-glyph recorder-idle" }, MicIcon {} }
                p { class: "media-import-title recorder-title", "{title}" }
                if !hint.is_empty() {
                    p { class: "media-import-hint", "{hint}" }
                }
                div { class: "recorder-actions",
                    // Cancel waits for the permission prompt to settle: a stream
                    // granted after the dialog closed would keep the mic open.
                    if is_recording || error.read().is_some() {
                        button { class: "icon-btn", title: "Discard recording", onclick: cancel, "✕" }
                    }
                    if is_recording {
                        button { class: "icon-btn recorder-stop", title: "Stop and add", onclick: stop, "■" }
                    }
                }
            }
        }
    }
}
