use dioxus::prelude::*;
use dioxus::document::Stylesheet;
use std::collections::HashSet;
use domain_flashcard::*;
use domain_tagging::Tag;
use crate::app::{Route, TabsCtx};
use crate::components::{ CreateDeck, LoadingModal };
use crate::api::{
    get_decks, export_deck, import_deck, delete_deck, rename_deck,
    pick_cover_image, set_deck_cover,
    get_all_tags_full, get_cards_by_tags, search_cards,
};
use crate::components::block_view::image_url_from_virtual_path;

static CSS: Asset = asset!("/assets/deck_list.css");


#[component]
pub fn DeleteDeck(deck_id: i64, on_done: EventHandler<()>) -> Element {
    rsx! {
        div { class: "dialog-backdrop",
            div { class: "dialog",
                h3 { class: "dialog-title", "Delete this deck?" }
                p { class: "dialog-body",
                    "All cards in this deck will be permanently deleted. This cannot be undone."
                }
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
                                let _ = delete_deck(deck_id).await;
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


/// Global card-search panel. Kept as its own component so that typing in the
/// search box only re-renders this subtree — not the whole deck grid (which is
/// what made typing lag).
#[component]
fn DeckSearch(deck_names: std::collections::HashMap<i64, String>) -> Element {
    let tabs_ctx = use_context::<TabsCtx>();
    let mut query = use_signal(String::new);
    let mut results: Signal<Vec<Card>> = use_signal(Vec::new);
    let mut loading = use_signal(|| false);
    // Whether a search has actually been run for the current query (drives the
    // empty/"press Enter" hint vs. the results view).
    let mut searched = use_signal(|| false);

    rsx! {
        div { class: "deck-search-panel",
            div { class: "deck-search-input-row",
                input {
                    class: "deck-search-input",
                    placeholder: "Search card names and content…",
                    autofocus: true,
                    value: "{query}",
                    oninput: move |e| {
                        query.set(e.value());
                        // Typing invalidates the previous result set until the
                        // user presses Enter again.
                        searched.set(false);
                        results.write().clear();
                    },
                    onkeydown: move |e| {
                        if e.key() == Key::Enter {
                            let q = query.read().trim().to_string();
                            if q.is_empty() {
                                results.write().clear();
                                searched.set(false);
                            } else {
                                loading.set(true);
                                searched.set(true);
                                spawn(async move {
                                    let r = search_cards(q).await;
                                    results.set(r);
                                    loading.set(false);
                                });
                            }
                        }
                    },
                }
                if *loading.read() {
                    span { class: "deck-search-spinner", "…" }
                }
            }

            if !*searched.read() {
                p { class: "tag-filter-hint", "Type a query and press Enter to search all card names and content." }
            } else if *loading.read() {
                p { class: "tag-filter-hint", "Searching…" }
            } else if results.read().is_empty() {
                p { class: "tag-filter-hint", "No cards found." }
            } else {
                p { class: "tag-filter-hint", "{results.read().len()} result(s)" }
                div { class: "tag-filtered-cards",
                    for card in results.read().iter().cloned() {
                        {
                            let deck_name = deck_names.get(&card.deck_id).cloned().unwrap_or_default();
                            rsx! {
                                div {
                                    class: "tag-filtered-card-row",
                                    onclick: move |_| { tabs_ctx.open(Route::CardView { id: card.id }); },
                                    span { class: "tag-filtered-card-deck", "{deck_name}" }
                                    span { class: "tag-filtered-card-name", "{card.name}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn DeckList() -> Element {
    let nav = navigator();
    let tabs_ctx = use_context::<TabsCtx>();
    let mut creating = use_signal(|| false);
    let mut deleting: Signal<Option<i64>> = use_signal(|| None);
    let mut decks = use_signal(|| Vec::<Deck>::new());
    let mut renaming: Signal<Option<i64>> = use_signal(|| None);
    let mut rename_value = use_signal(String::new);
    let mut loading_msg: Signal<Option<String>> = use_signal(|| None);

    // Global search state — the panel itself is the DeckSearch child component,
    // which owns the query/results so typing doesn't re-render the deck grid.
    let mut show_search = use_signal(|| false);

    // Global tag filter state
    let mut show_tag_filter = use_signal(|| false);
    let mut all_tags: Signal<Vec<Tag>> = use_signal(Vec::new);
    let mut tag_search_query = use_signal(String::new);
    let mut selected_tag_ids: Signal<HashSet<i64>> = use_signal(HashSet::new);
    let mut filtered_cards: Signal<Vec<Card>> = use_signal(Vec::new);
    let mut tags_loading = use_signal(|| false);

    use_future(move || async move {
        let loaded = get_decks().await;
        decks.set(loaded);
    });

    let deck_views: Vec<(i64, String, u32, Option<String>)> = decks
        .read()
        .iter()
        .map(|d| (d.id, d.name.clone(), d.card_count, d.cover_image.clone()))
        .collect();

    // Deck id → name map for displaying cards' deck names
    let deck_name_map: std::collections::HashMap<i64, String> = decks
        .read()
        .iter()
        .map(|d| (d.id, d.name.clone()))
        .collect();

    rsx! {
        Stylesheet { href: CSS }
        div { class: "page",

            if let Some(msg) = loading_msg.read().clone() {
                LoadingModal { message: msg }
            }

            // ----------------------------------------------------------------
            // Toolbar
            // ----------------------------------------------------------------
            div { class: "deckpage-toolbar",
                div { class: "deckpage-toolbar-left",
                    button {
                        class: if *show_search.read() {
                            "toolbar-icon-btn toolbar-icon-btn-active"
                        } else {
                            "toolbar-icon-btn"
                        },
                        title: "Search cards",
                        onclick: move |_| {
                            let next = !*show_search.read();
                            show_search.set(next);
                        },
                        "◌"
                    }
                    button {
                        class: if *show_tag_filter.read() {
                            "toolbar-icon-btn toolbar-icon-btn-active"
                        } else {
                            "toolbar-icon-btn"
                        },
                        title: "Filter by tag",
                        onclick: move |_| {
                            let next = !*show_tag_filter.read();
                            show_tag_filter.set(next);
                            if next {
                                tags_loading.set(true);
                                spawn(async move {
                                    let tags = get_all_tags_full().await;
                                    // Remove any selected tag that no longer exists
                                    let live_ids: std::collections::HashSet<i64> =
                                        tags.iter().map(|t| t.id).collect();
                                    selected_tag_ids.write().retain(|id| live_ids.contains(id));
                                    all_tags.set(tags);
                                    tags_loading.set(false);
                                    // Refresh filtered cards if tags are still selected
                                    let ids: Vec<i64> = selected_tag_ids.read().iter().copied().collect();
                                    if !ids.is_empty() {
                                        filtered_cards.set(get_cards_by_tags(ids).await);
                                    }
                                });
                            } else {
                                selected_tag_ids.write().clear();
                                filtered_cards.write().clear();
                                tag_search_query.set(String::new());
                            }
                        },
                        "⌘"
                    }
                    button {
                        class: "toolbar-icon-btn desktop-only",
                        title: "Knowledge map",
                        onclick: move |_| { nav.push(Route::KnowledgeMapPage); },
                        "◎"
                    }
                }

                div { class: "deckpage-toolbar-right desktop-only",
                    button {
                        class: "toolbar-icon-btn",
                        title: "Import deck",
                        onclick: move |_| {
                            loading_msg.set(Some("Importing deck…".into()));
                            spawn(async move {
                                let new_deck_id = import_deck().await;
                                loading_msg.set(None);
                                if new_deck_id > 0 {
                                    nav.push(Route::CardListPage { id: new_deck_id });
                                }
                                let loaded = get_decks().await;
                                decks.set(loaded);
                            });
                        },
                        "↓"
                    }
                    button {
                        class: "toolbar-icon-btn",
                        title: "Add deck",
                        onclick: move |_| creating.set(true),
                        "＋"
                    }
                }
            }

            // ----------------------------------------------------------------
            // Global card search panel (own component to isolate re-renders)
            // ----------------------------------------------------------------
            if *show_search.read() {
                DeckSearch { deck_names: deck_name_map.clone() }
            }

            // ----------------------------------------------------------------
            // Global tag filter panel
            // ----------------------------------------------------------------
            if *show_tag_filter.read() {
                div { class: "deck-tag-filter-panel",
                    if *tags_loading.read() {
                        p { class: "tag-filter-hint", "Loading tags…" }
                    } else if all_tags.read().is_empty() {
                        p { class: "tag-filter-hint", "No tags yet." }
                    } else {
                        input {
                            class: "tag-search-input",
                            placeholder: "Filter tags…",
                            value: "{tag_search_query}",
                            oninput: move |e| tag_search_query.set(e.value()),
                        }
                        div { class: "tag-filter-chips",
                            for tag in all_tags.read().iter().cloned().filter(|t| {
                                let q = tag_search_query.read();
                                q.trim().is_empty() || t.name.to_lowercase().contains(&q.trim().to_lowercase())
                            }) {
                                {
                                    let is_sel = selected_tag_ids.read().contains(&tag.id);
                                    rsx! {
                                        button {
                                            class: if is_sel { "tag-chip tag-chip-selected" } else { "tag-chip" },
                                            onclick: move |_| {
                                                {
                                                    let mut sel = selected_tag_ids.write();
                                                    if sel.contains(&tag.id) {
                                                        sel.remove(&tag.id);
                                                    } else {
                                                        sel.insert(tag.id);
                                                    }
                                                }
                                                let ids: Vec<i64> = selected_tag_ids.read().iter().copied().collect();
                                                spawn(async move {
                                                    if ids.is_empty() {
                                                        filtered_cards.set(Vec::new());
                                                    } else {
                                                        let cards = get_cards_by_tags(ids).await;
                                                        filtered_cards.set(cards);
                                                    }
                                                });
                                            },
                                            "{tag.name}"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if !selected_tag_ids.read().is_empty() {
                        if filtered_cards.read().is_empty() {
                            p { class: "tag-filter-hint", "No cards match the selected tags." }
                        } else {
                            div { class: "tag-filtered-cards",
                                for card in filtered_cards.read().iter().cloned() {
                                    {
                                        let deck_name = deck_name_map.get(&card.deck_id).cloned().unwrap_or_default();
                                        rsx! {
                                            div {
                                                class: "tag-filtered-card-row",
                                                onclick: move |_| { tabs_ctx.open(Route::CardView { id: card.id }); },
                                                span { class: "tag-filtered-card-deck", "{deck_name}" }
                                                span { class: "tag-filtered-card-name", "{card.name}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ----------------------------------------------------------------
            // Deck grid
            // ----------------------------------------------------------------
            div { class: "deck-list",

                for (id , name, card_count, cover) in deck_views {

                    div { class: "deck-card",

                        // Cover image — full-width at top, clickable to open deck
                        div {
                            class: "deck-cover-area",
                            onclick: move |_| { nav.push(Route::CardListPage { id }); },
                            if let Some(ref src) = cover {
                                img {
                                    class: "deck-cover",
                                    src: "{image_url_from_virtual_path(src)}",
                                    alt: "",
                                }
                            } else {
                                div { class: "deck-cover-placeholder" }
                            }

                            div { class: "deck-cover-actions desktop-only",
                                button {
                                    class: "deck-cover-btn",
                                    title: "Set cover",
                                    onclick: move |e| {
                                        e.stop_propagation();
                                        spawn(async move {
                                            let path = pick_cover_image().await;
                                            if !path.is_empty() {
                                                set_deck_cover(id, Some(path)).await;
                                                let loaded = get_decks().await;
                                                decks.set(loaded);
                                            }
                                        });
                                    },
                                    "Set cover"
                                }

                                if cover.is_some() {
                                    button {
                                        class: "deck-cover-btn",
                                        title: "Remove cover",
                                        onclick: move |e| {
                                            e.stop_propagation();
                                            spawn(async move {
                                                set_deck_cover(id, None).await;
                                                let loaded = get_decks().await;
                                                decks.set(loaded);
                                            });
                                        },
                                        "Remove cover"
                                    }
                                }
                            }
                        }

                        // Deck name + card count
                        button {
                            class: "deck-main",
                            onclick: move |_| { nav.push(Route::CardListPage { id }); },
                            span { class: "deck-name", "{name}" }
                            span { class: "deck-card-count",
                                if card_count == 1 { "1 card" } else { "{card_count} cards" }
                            }
                        }

                        // Action buttons — icon-only with tooltips
                        div { class: "deck-actions desktop-only",
                            button {
                                class: "deck-action-btn",
                                title: "Rename",
                                onclick: move |_| {
                                    renaming.set(Some(id));
                                    rename_value.set(name.clone());
                                },
                                "✏"
                            }

                            button {
                                class: "deck-action-btn",
                                title: "Export",
                                onclick: move |_| {
                                    loading_msg.set(Some("Preparing export…".into()));
                                    spawn(async move {
                                        export_deck(id).await;
                                        loading_msg.set(None);
                                    });
                                },
                                "↑"
                            }

                            button {
                                class: "deck-action-btn danger",
                                title: "Delete",
                                onclick: move |_| deleting.set(Some(id)),
                                "✕"
                            }
                        }
                        if deleting.read().as_ref() == Some(&id) {
                            DeleteDeck {
                                deck_id: id,
                                on_done: move |_| {
                                    deleting.set(None);
                                    spawn(async move {
                                        let loaded = get_decks().await;
                                        decks.set(loaded);
                                    });
                                },
                            }
                        }
                    }

                    // Rename dialog — same modal style as CreateDeck/DeleteDeck.
                    if renaming.read().as_ref() == Some(&id) {
                        div { class: "dialog-backdrop",
                            div { class: "dialog",
                                h3 { class: "dialog-title", "Rename deck" }
                                input {
                                    class: "dialog-input",
                                    placeholder: "Deck name…",
                                    autofocus: true,
                                    value: "{rename_value}",
                                    oninput: move |e| rename_value.set(e.value()),
                                    onkeydown: move |e| {
                                        if e.key() == Key::Enter {
                                            let new_name = rename_value.read().clone();
                                            renaming.set(None);
                                            spawn(async move {
                                                let _ = rename_deck(new_name, id).await;
                                                let loaded = get_decks().await;
                                                decks.set(loaded);
                                            });
                                        }
                                    },
                                }
                                div { class: "dialog-actions",
                                    button {
                                        class: "button button-secondary",
                                        onclick: move |_| renaming.set(None),
                                        "Cancel"
                                    }
                                    button {
                                        class: "button button-primary",
                                        onclick: move |_| {
                                            let new_name = rename_value.read().clone();
                                            renaming.set(None);
                                            spawn(async move {
                                                let _ = rename_deck(new_name, id).await;
                                                let loaded = get_decks().await;
                                                decks.set(loaded);
                                            });
                                        },
                                        "Save"
                                    }
                                }
                            }
                        }
                    }
                }

                if *creating.read() {
                    CreateDeck {
                        on_done: move |_| {
                            creating.set(false);
                            spawn(async move {
                                let loaded = get_decks().await;
                                decks.set(loaded);
                            });
                        },
                    }
                }
            }
        }
    }
}
