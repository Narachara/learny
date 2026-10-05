use dioxus::prelude::*;
use dioxus::document::Stylesheet;
use gloo_timers::future::TimeoutFuture;

use crate::components::block_editor::InsertBlockKind;

static CSS: Asset = asset!("/assets/media_upload.css");

/// Copy shown per media kind: (glyph, title, hint).
fn copy_for(kind: InsertBlockKind) -> (&'static str, &'static str, &'static str) {
    match kind {
        InsertBlockKind::Video => (
            "▶",
            "Adding video",
            "Pick a file, then it is copied into your library. Large videos can take a while — the card stays open.",
        ),
        InsertBlockKind::Audio => (
            "♪",
            "Adding audio",
            "Pick a file, then it is copied into your library.",
        ),
        InsertBlockKind::Image => (
            "⬚",
            "Adding image",
            "Pick a file — large images are resized as they are imported.",
        ),
        InsertBlockKind::File => (
            "⊟",
            "Adding file",
            "Pick a file, then it is copied into your library.",
        ),
        // Text blocks never reach the overlay (they are created instantly).
        InsertBlockKind::Text => ("T", "Adding block", "One moment…"),
    }
}

/// Full-screen overlay covering the pick + copy of a media block.
///
/// Neither the desktop copy nor the web upload reports bytes transferred,
/// so there is no honest percentage to show: the bar is indeterminate and
/// an elapsed counter (after 3s) tells the user it is still moving.
#[component]
pub fn MediaUploadOverlay(kind: InsertBlockKind) -> Element {
    let (glyph, title, hint) = copy_for(kind);
    let mut elapsed = use_signal(|| 0u32);

    use_effect(move || {
        spawn(async move {
            loop {
                TimeoutFuture::new(1000).await;
                elapsed.set(elapsed() + 1);
            }
        });
    });

    rsx! {
        Stylesheet { href: CSS }
        div { class: "media-import-overlay", role: "alertdialog", aria_busy: "true",
            div { class: "media-import-card",
                div { class: "media-import-glyph", "{glyph}" }
                p { class: "media-import-title", "{title}" }
                p { class: "media-import-hint", "{hint}" }
                div { class: "media-import-track" }
                if *elapsed.read() >= 3 {
                    p { class: "media-import-elapsed", "{elapsed}s elapsed" }
                }
            }
        }
    }
}
