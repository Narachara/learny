use dioxus::prelude::*;
use dioxus::document::Stylesheet;

static CSS: Asset = asset!("/assets/loading_modal.css");

#[component]
pub fn LoadingModal(message: String) -> Element {
    rsx! {
        Stylesheet { href: CSS }
        div { class: "loading-overlay",
            div { class: "loading-modal",
                div { class: "loading-spinner" }
                p { class: "loading-message", "{message}" }
            }
        }
    }
}
