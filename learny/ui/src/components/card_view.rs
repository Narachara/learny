use dioxus::prelude::*;
use dioxus::document::Stylesheet;
use std::collections::HashMap;
use domain_flashcard::{ Card };
use domain_tagging::CardTagInfo;
use crate::components::block_view::render_block;
use crate::app::{ Route, TabsCtx, TabId, TAB_TITLES };
use crate::api::{ get_card, get_deck, get_decks, get_card_tags_with_position, get_cards_by_tags, delete_card, update_score, pick_image, set_card_cover, save_card_notes, move_cards };
use crate::components::block_view::image_url_from_virtual_path;

static CSS: Asset = asset!("/assets/card_view.css");
static BLOCKS_CSS: Asset = asset!("/assets/blocks.css");

/// Write `text` to the system clipboard via the browser/webview clipboard API.
async fn copy_to_clipboard(text: &str) {
    let identifier = format!("Card_id = {text}");
    if let Some(window) = web_sys::window() {
        let clipboard = window.navigator().clipboard();
        let _ = wasm_bindgen_futures::JsFuture::from(
            clipboard.write_text(&identifier)
        ).await;
    }
}

#[component]
pub fn DeleteCard(card_id: i64, on_done: EventHandler<()>) -> Element {
    rsx! {
        div { class: "dialog-backdrop",
            div { class: "dialog",
                h3 { class: "dialog-title", "Delete this card?" }
                p { class: "dialog-body", "This cannot be undone." }
                div { class: "dialog-actions",
                    button {
                        class: "button button-secondary",
                        onclick: move |_| on_done.call(()),
                        "Cancel"
                    }
                    button {
                        class: "button button-danger",
                        onclick: move |_| {
                            spawn(async move {
                                let _ = delete_card(card_id).await;
                                on_done.call(());
                            });
                        },
                        "Delete"
                    }
                }
            }
        }
    }
}

#[component]
pub fn CardView(id: i64) -> Element {
    // Navigating between cards lands on the same `CardView` route component, so
    // the router reuses the instance and its hooks/signals would carry over from
    // the previous card. Keying the inner view by `id` forces a fresh mount each
    // time the card changes, so every tab loads its own card.
    rsx! {
        CardViewInner { key: "{id}", id }
    }
}

#[component]
fn CardViewInner(id: i64) -> Element {
    let mut show_answer = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut copied = use_signal(|| false);

    // Move-to-deck state.
    let mut show_move = use_signal(|| false);
    let mut move_target: Signal<Option<i64>> = use_signal(|| None);
    let mut moving = use_signal(|| false);
    let nav = navigator();
    let tabs_ctx = use_context::<TabsCtx>();
    let tab = use_context::<TabId>().0;

    let mut card_signal = use_signal(|| Card::new_empty(id));
    let mut easy_note = use_signal(|| String::new());
    let mut difficult_note = use_signal(|| String::new());
    let mut deck_name = use_signal(|| String::new());
    // Tags with their position in each tag's progression, so every chip can
    // show its order number.
    let mut tags = use_signal(|| Vec::<CardTagInfo>::new());

    // Tag dropdown: which tag is expanded, the related cards across all decks,
    // and a deck-id → name map to label each row.
    let mut open_tag: Signal<Option<i64>> = use_signal(|| None);
    let mut related_cards = use_signal(|| Vec::<Card>::new());
    let mut related_loading = use_signal(|| false);
    let mut deck_names = use_signal(|| HashMap::<i64, String>::new());

    use_effect(move || {
        spawn(async move {
            let loaded = get_card(id).await;
            easy_note.set(loaded.what_was_easy.clone().unwrap_or_default());
            difficult_note.set(loaded.what_was_difficult.clone().unwrap_or_default());
            let deck = get_deck(loaded.deck_id).await;
            deck_name.set(deck.name);
            tags.set(get_card_tags_with_position(id).await);
            deck_names.set(get_decks().await.into_iter().map(|d| (d.id, d.name)).collect());
            // Name this tab after the card.
            TAB_TITLES.write().insert(tab, loaded.name.clone());
            card_signal.set(loaded);
        });
    });

    let card = card_signal.read();
    let deck_id = card.deck_id;
    let cover = card.cover_image.clone();

    // Other decks to move this card to (everything except its current deck).
    let mut deck_options: Vec<(i64, String)> = deck_names.read().iter()
        .filter(|(k, _)| **k != deck_id)
        .map(|(k, v)| (*k, v.clone()))
        .collect();
    deck_options.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));

    rsx! {
        Stylesheet { href: CSS }
        Stylesheet { href: BLOCKS_CSS }
        div { class: "card-view",

            // Top bar: back + title + actions
            div { class: "card-topbar",
                button {
                    class: "icon-btn icon-btn-lg",
                    title: "Back to deck",
                    onclick: move |_| {
                        nav.push(Route::CardListPage { id: deck_id });
                    },
                    "‹"
                }

                div { class: "card-titleblock",
                    h1 { class: "card-title", "{card.name}" }
                    if !deck_name.read().is_empty() {
                        button {
                            class: "card-deck-chip",
                            title: "Back to deck",
                            onclick: move |_| {
                                nav.push(Route::CardListPage { id: deck_id });
                            },
                            "{deck_name}"
                        }
                    }
                }

                div { class: "card-topbar-actions",
                    button {
                        class: "icon-btn icon-btn-lg",
                        title: "Copy card ID to clipboard",
                        onclick: move |_| {
                            spawn(async move {
                                copy_to_clipboard(&id.to_string()).await;
                                copied.set(true);
                                gloo_timers::future::TimeoutFuture::new(1800).await;
                                copied.set(false);
                            });
                        },
                        "⧉"
                    }
                    button {
                        class: "icon-btn icon-btn-lg desktop-only",
                        title: "Edit card",
                        onclick: move |_| {
                            nav.push(Route::CardEditorEdit { id });
                        },
                        "✐"
                    }
                    button {
                        class: if *show_move.read() { "icon-btn icon-btn-lg active desktop-only" } else { "icon-btn icon-btn-lg desktop-only" },
                        title: "Move card to another deck",
                        onclick: move |_| {
                            let next = !*show_move.read();
                            show_move.set(next);
                            move_target.set(None);
                        },
                        "↔"
                    }
                    button {
                        class: "icon-btn icon-btn-lg desktop-only",
                        title: "Set cover image",
                        onclick: move |_| {
                            spawn(async move {
                                let path = pick_image().await;
                                if !path.is_empty() {
                                    set_card_cover(id, Some(path)).await;
                                    let loaded = get_card(id).await;
                                    card_signal.set(loaded);
                                }
                            });
                        },
                        "▧"
                    }
                    if cover.is_some() {
                        button {
                            class: "icon-btn icon-btn-lg danger desktop-only",
                            title: "Remove cover image",
                            onclick: move |_| {
                                spawn(async move {
                                    set_card_cover(id, None).await;
                                    let loaded = get_card(id).await;
                                    card_signal.set(loaded);
                                });
                            },
                            "✕"
                        }
                    }
                    button {
                        class: "icon-btn icon-btn-lg danger desktop-only",
                        title: "Delete card",
                        onclick: move |_| deleting.set(true),
                        "×"
                    }
                }
            }

            // Move-to-deck bar.
            if *show_move.read() {
                div { class: "card-move-bar",
                    span { class: "card-move-hint", "Move this card to:" }
                    select {
                        class: "move-deck-select",
                        onchange: move |e| move_target.set(e.value().parse::<i64>().ok()),
                        option { value: "", "— Select destination deck —" }
                        for (did , dname) in deck_options.iter().cloned() {
                            option { value: "{did}", "{dname}" }
                        }
                    }
                    button {
                        class: "button button-primary",
                        disabled: move_target.read().is_none() || *moving.read(),
                        onclick: move |_| {
                            let Some(target) = *move_target.read() else { return };
                            moving.set(true);
                            spawn(async move {
                                move_cards(vec![id], target).await;
                                // Reflect the card's new deck in this view.
                                let loaded = get_card(id).await;
                                let d = get_deck(loaded.deck_id).await;
                                deck_name.set(d.name);
                                card_signal.set(loaded);
                                moving.set(false);
                                show_move.set(false);
                                move_target.set(None);
                            });
                        },
                        if *moving.read() { "Moving…" } else { "Move" }
                    }
                    button {
                        class: "button button-secondary",
                        onclick: move |_| { show_move.set(false); move_target.set(None); },
                        "Cancel"
                    }
                }
            }

            // Tags — click to reveal related cards across all decks; clicking a
            // card opens it in a new tab while this dropdown stays open.
            if !tags.read().is_empty() {
                div { class: "card-tags",
                    for info in tags.read().iter().cloned() {
                        {
                            let tag = info.tag;
                            let pos = info.position;
                            let tag_id = tag.id;
                            let is_open = *open_tag.read() == Some(tag_id);
                            // The raw stored position, so this always agrees
                            // with the editor's `#` field. 0 is a real slot —
                            // tags start their progression there.
                            let pos_title = format!("#{pos} in the \"{}\" progression", tag.name);
                            rsx! {
                                button {
                                    key: "{tag_id}",
                                    class: if is_open { "tag-chip tag-chip-link tag-chip-open" } else { "tag-chip tag-chip-link" },
                                    title: "Show related \"{tag.name}\" cards",
                                    onclick: move |_| {
                                        if *open_tag.read() == Some(tag_id) {
                                            open_tag.set(None);
                                        } else {
                                            open_tag.set(Some(tag_id));
                                            related_loading.set(true);
                                            spawn(async move {
                                                related_cards.set(get_cards_by_tags(vec![tag_id]).await);
                                                related_loading.set(false);
                                            });
                                        }
                                    },
                                    span { class: "tag-chip-num", title: "{pos_title}", "{pos}" }
                                    span { class: "tag-chip-label", "{tag.name}" }
                                }
                            }
                        }
                    }

                    if let Some(open_id) = *open_tag.read() {
                        div { class: "tag-cards-dropdown",
                            div { class: "tag-cards-dropdown-head",
                                span { class: "tag-cards-dropdown-title",
                                    {
                                        let name = tags
                                            .read()
                                            .iter()
                                            .find(|i| i.tag.id == open_id)
                                            .map(|i| i.tag.name.clone())
                                            .unwrap_or_default();
                                        rsx! { "Cards tagged \"{name}\"" }
                                    }
                                }
                                button {
                                    class: "tag-cards-dropdown-close",
                                    title: "Close",
                                    onclick: move |_| open_tag.set(None),
                                    "✕"
                                }
                            }
                            if *related_loading.read() {
                                p { class: "tag-cards-empty", "Loading…" }
                            } else if related_cards.read().is_empty() {
                                p { class: "tag-cards-empty", "No related cards." }
                            } else {
                                div { class: "tag-cards-list",
                                    for rc in related_cards.read().iter().cloned() {
                                        {
                                            let card_id = rc.id;
                                            let deck_label = deck_names.read().get(&rc.deck_id).cloned().unwrap_or_default();
                                            let is_current = card_id == id;
                                            rsx! {
                                                button {
                                                    key: "{card_id}",
                                                    class: if is_current { "tag-card-row tag-card-row-current" } else { "tag-card-row" },
                                                    title: "Open in a new tab",
                                                    onclick: move |_| {
                                                        tabs_ctx.open(Route::CardView { id: card_id });
                                                    },
                                                    span { class: "tag-card-deck", "{deck_label}" }
                                                    span { class: "tag-card-name", "{rc.name}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Cover image (decorative only)
            if let Some(ref src) = cover {
                img {
                    class: "card-cover",
                    src: "{image_url_from_virtual_path(src)}",
                    alt: "",
                }
            }

            // Study area
            div { class: "card-study",

                div { class: "card-surface",
                    for block in &card.front_blocks {
                        {render_block(block)}
                    }
                }

                if !*show_answer.read() {
                    div { class: "show-answer-container",
                        button {
                            class: "show-answer-btn",
                            onclick: move |_| show_answer.set(true),
                            "Reveal answer"
                        }
                    }
                }

                if *show_answer.read() {
                    div { class: "answer-surface",
                        for block in &card.back_blocks {
                            {render_block(block)}
                        }
                    }

                    div { class: "card-notes",
                        div { class: "card-note-field card-note-easy",
                            div { class: "card-note-label",
                                span { class: "card-note-icon", "✓" }
                                "What was easy?"
                            }
                            textarea {
                                value: "{easy_note}",
                                oninput: move |e| easy_note.set(e.value()),
                                placeholder: "What clicked for you…",
                            }
                        }
                        div { class: "card-note-field card-note-difficult",
                            div { class: "card-note-label",
                                span { class: "card-note-icon", "?" }
                                "What was difficult?"
                            }
                            textarea {
                                value: "{difficult_note}",
                                oninput: move |e| difficult_note.set(e.value()),
                                placeholder: "What you want to revisit…",
                            }
                        }
                    }

                    div { class: "card-rating",
                        button {
                            class: "rating-btn rating-btn-bad",
                            title: "Didn't know it",
                            onclick: move |_| {
                                let easy = easy_note.read().clone();
                                let diff = difficult_note.read().clone();
                                spawn(async move {
                                    save_card_notes(
                                            id,
                                            if easy.is_empty() { None } else { Some(easy) },
                                            if diff.is_empty() { None } else { Some(diff) },
                                        )
                                        .await;
                                    let _ = update_score(id, false).await;
                                    nav.push(Route::CardListPage { id: deck_id });
                                });
                            },
                            "✗"
                        }
                        button {
                            class: "rating-btn rating-btn-good",
                            title: "Got it!",
                            onclick: move |_| {
                                let easy = easy_note.read().clone();
                                let diff = difficult_note.read().clone();
                                spawn(async move {
                                    save_card_notes(
                                            id,
                                            if easy.is_empty() { None } else { Some(easy) },
                                            if diff.is_empty() { None } else { Some(diff) },
                                        )
                                        .await;
                                    let _ = update_score(id, true).await;
                                    nav.push(Route::CardListPage { id: deck_id });
                                });
                            },
                            "✓"
                        }
                    }
                }
            }
        }

        if *copied.read() {
            div { class: "copy-toast", "Card ID copied to clipboard" }
        }

        if *deleting.read() {
            DeleteCard {
                card_id: id,
                on_done: move |_| {
                    deleting.set(false);
                    nav.push(Route::CardListPage { id: deck_id });
                },
            }
        }
    }
}
