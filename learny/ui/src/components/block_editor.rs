use domain_flashcard::*;
use dioxus::prelude::*;
use crate::api::{delete_block_from_app_data};
use crate::components::block_view::{image_url_from_virtual_path, MarkdownBlock};

/// Display widths offered for an image block, as a percentage of its container.
const IMAGE_SCALES: [u8; 4] = [25, 50, 75, 100];

/// Which kind of block the insert menu should create.
#[derive(Clone, Copy, PartialEq)]
pub enum InsertBlockKind {
    Text,
    Image,
    Audio,
    Video,
    File,
}


#[component]
pub fn BlockEditor(
    block: Block,
    on_update: EventHandler<Block>,
    on_remove: EventHandler<()>,
    on_insert_above: EventHandler<InsertBlockKind>,
    on_insert_below: EventHandler<InsertBlockKind>,
) -> Element {
    let insert_menu = |handler: EventHandler<InsertBlockKind>| -> Element {
        rsx!(
            div { class: "insert-menu",
                button { title: "Insert text",  onclick: move |_| handler.call(InsertBlockKind::Text),  "T" }
                button { title: "Insert image", onclick: move |_| handler.call(InsertBlockKind::Image), "⬚" }
                button { title: "Insert audio", onclick: move |_| handler.call(InsertBlockKind::Audio), "♪" }
                button { title: "Insert video", onclick: move |_| handler.call(InsertBlockKind::Video), "▶" }
                button { title: "Insert file",  onclick: move |_| handler.call(InsertBlockKind::File),  "⊟" }
            }
        )
    };
    
    match block {
        Block::Text { value } => rsx!(
            div { class: "block-editor text-editor",
                {insert_menu(on_insert_above.clone())}
                div { class: "text-editor-body",
                    textarea {
                        value: "{value}",
                        oninput: move |evt| on_update.call(Block::Text { value: evt.value() }),
                    }
                    if !value.trim().is_empty() {
                        div { class: "text-preview",
                            MarkdownBlock { value: value.clone() }
                        }
                    }
                }
                {insert_menu(on_insert_below.clone())}
                button { class: "icon-btn block-remove-btn", title: "Remove block",
                    onclick: move |_| on_remove.call(()), "✕" }
            }
        ),

        Block::Image { src, scale } => rsx!(
            div { class: "block-editor image-editor",
                {insert_menu(on_insert_above.clone())}
                img {
                    src: "{image_url_from_virtual_path(&src)}",
                    class: "image-preview",
                    style: "width: {scale.unwrap_or(100)}%",
                }
                div { class: "image-size-controls",
                    for pct in IMAGE_SCALES {
                        {
                            let src = src.clone();
                            let selected = scale.unwrap_or(100) == pct;
                            rsx! {
                                button {
                                    class: if selected { "icon-btn image-size-btn selected" } else { "icon-btn image-size-btn" },
                                    title: "Show this image at {pct}% width",
                                    onclick: move |_| on_update.call(Block::Image {
                                        src: src.clone(),
                                        // Full width is the default — store it as
                                        // absent rather than as an explicit 100.
                                        scale: (pct != 100).then_some(pct),
                                    }),
                                    "{pct}%"
                                }
                            }
                        }
                    }
                }
                {insert_menu(on_insert_below.clone())}
                button { class: "icon-btn block-remove-btn danger", title: "Remove image",
                    onclick: move |_| {
                        let src = src.clone();
                        let on_remove = on_remove.clone();
                        spawn(async move { delete_block_from_app_data(src).await; on_remove.call(()); });
                    }, "✕" }
            }
        ),

        Block::Audio { src } => rsx!(
            div { class: "block-editor audio-editor",
                {insert_menu(on_insert_above.clone())}
                audio { controls: true, src: "{image_url_from_virtual_path(&src)}" }
                {insert_menu(on_insert_below.clone())}
                button { class: "icon-btn block-remove-btn danger", title: "Remove audio",
                    onclick: move |_| {
                        let src = src.clone();
                        let on_remove = on_remove.clone();
                        spawn(async move { delete_block_from_app_data(src).await; on_remove.call(()); });
                    }, "✕" }
            }
        ),

        Block::Video { src } => rsx!(
            div { class: "block-editor video-editor",
                {insert_menu(on_insert_above.clone())}
                video { controls: true, preload: "metadata", src: "{image_url_from_virtual_path(&src)}" }
                {insert_menu(on_insert_below.clone())}
                button { class: "icon-btn block-remove-btn danger", title: "Remove video",
                    onclick: move |_| {
                        let src = src.clone();
                        let on_remove = on_remove.clone();
                        spawn(async move { delete_block_from_app_data(src).await; on_remove.call(()); });
                    }, "✕" }
            }
        ),

        Block::File { path } => rsx!(
            div { class: "block-editor file-editor",
                {insert_menu(on_insert_above.clone())}
                p { "📎 {path}" }
                {insert_menu(on_insert_below.clone())}
                button { class: "icon-btn block-remove-btn danger", title: "Remove file",
                    onclick: move |_| {
                        let path = path.clone();
                        let on_remove = on_remove.clone();
                        spawn(async move { delete_block_from_app_data(path).await; on_remove.call(()); });
                    }, "✕" }
            }
        ),
    }
}
