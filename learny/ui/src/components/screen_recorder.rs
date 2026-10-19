use dioxus::prelude::*;

use crate::api::record_screen;

/// Monochrome screen-recording glyph matching the editor's existing icons.
#[component]
pub fn ScreenRecordIcon() -> Element {
    rsx! {
        svg {
            class: "mic-icon", width: "1em", height: "1em", view_box: "0 0 24 24",
            fill: "none", stroke: "currentColor", stroke_width: "2",
            stroke_linecap: "round", stroke_linejoin: "round", "aria-hidden": "true",
            rect { x: "3", y: "4", width: "18", height: "13", rx: "2" }
            circle { cx: "12", cy: "10.5", r: "2.5", fill: "currentColor", stroke: "none" }
            path { d: "M8 21h8" }
            path { d: "M12 17v4" }
        }
    }
}

/// Starts the macOS picker and turns the completed take into a video block.
/// It deliberately renders nothing: macOS owns the area-selection UI, and a
/// Learny modal would remain above the editor for the entire capture.
#[component]
pub fn ScreenRecorderOverlay(on_done: EventHandler<Option<String>>) -> Element {
    use_hook(|| {
        spawn(async move {
            match record_screen().await {
                Ok(path) if !path.is_empty() => on_done.call(Some(path)),
                Ok(_) => on_done.call(None),
                Err(message) => {
                    // The editor stays usable if macOS denies capture or the
                    // user cancels. Keep the concrete error in the app log
                    // until the editor grows a non-blocking toast surface.
                    dioxus_logger::tracing::error!("screen recording failed: {message}");
                    on_done.call(None);
                }
            }
        });
    });
    rsx! {}
}
