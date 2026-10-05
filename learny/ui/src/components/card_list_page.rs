use dioxus::prelude::*;
use dioxus::document::Stylesheet;
use std::collections::{HashMap, HashSet};
use domain_flashcard::{Card, Deck};
use domain_tagging::Tag;
use crate::app::{Route, TabId, TAB_TITLES};
use crate::api::{ get_cards, get_deck, get_decks, move_cards, delete_card, reset_deck_progress, get_deck_tags, get_card_tag_pairs, sync_card_tags };
use crate::components::block_view::image_url_from_virtual_path;

static CSS: Asset = asset!("/assets/card_list.css");

#[component]
pub fn CardListPage(id: i64) -> Element {
    let nav = navigator();
    let tab = use_context::<TabId>().0;
    let mut cards = use_signal(|| Vec::<Card>::new());
    let mut deck = use_signal(|| None::<Deck>);
    // How the list is ordered, and whether the tag filter is showing.
    let mut view_mode = use_signal(|| "progress".to_string());
    let mut search_query = use_signal(String::new);
    let mut tag_search_query = use_signal(String::new);

    // Reset progress confirmation
    let mut confirm_reset = use_signal(|| false);

    // Move mode state
    let mut move_mode = use_signal(|| false);
    let mut selected_ids: Signal<HashSet<i64>> = use_signal(|| HashSet::new());
    let mut all_decks = use_signal(|| Vec::<Deck>::new());
    let mut target_deck_id: Signal<Option<i64>> = use_signal(|| None);
    let mut moving = use_signal(|| false);

    // Delete mode state (batch delete; shares selected_ids with move mode).
    let mut delete_mode = use_signal(|| false);
    let mut confirm_batch_delete = use_signal(|| false);
    let mut deleting = use_signal(|| false);

    // Untag mode state (strip every tag off the selected cards; shares
    // selected_ids with move and delete mode).
    let mut untag_mode = use_signal(|| false);
    let mut confirm_untag = use_signal(|| false);
    let mut untagging = use_signal(|| false);

    // Tag mode state
    let mut deck_tags: Signal<Vec<Tag>> = use_signal(|| Vec::new());
    let mut selected_tag_ids: Signal<HashSet<i64>> = use_signal(|| HashSet::new());
    // card_id -> set of tag_ids, loaded once when entering tag mode
    let mut card_tag_map: Signal<HashMap<i64, HashSet<i64>>> = use_signal(|| HashMap::new());

    use_future(move || async move {
        let loaded_deck = get_deck(id).await;
        // Name this tab after the deck.
        TAB_TITLES.write().insert(tab, loaded_deck.name.clone());
        deck.set(Some(loaded_deck));
        let loaded_cards = get_cards(id).await;
        cards.set(loaded_cards);
    });

    let active_tags = selected_tag_ids.read();
    let tag_map = card_tag_map.read();
    let current_mode = view_mode.read().clone();
    let query = search_query.read().trim().to_lowercase();
    let mut card_views: Vec<(i64, String, u8, u32, i64, Option<String>)> = cards
        .read()
        .iter()
        .filter(|c| {
            if active_tags.is_empty() { return true; }
            tag_map.get(&c.id)
                .map(|card_tags| active_tags.iter().any(|tid| card_tags.contains(tid)))
                .unwrap_or(false)
        })
        .filter(|c| query.is_empty() || c.name.to_lowercase().contains(&query))
        .map(|c| (c.id, c.name.clone(), c.progress_percent(), c.times_seen, c.created_at, c.cover_image.clone()))
        .collect();
    if current_mode == "date" {
        card_views.sort_by_key(|(_, _, _, _, created_at, _)| *created_at);
    } else {
        card_views.sort_by_key(|(_, _, progress, times_seen, _, _)| (*progress, std::cmp::Reverse(*times_seen)));
    }

    // Ids of the cards currently on screen — select-all works on what the tag
    // filter and search actually left visible, not the whole deck.
    let visible_ids: Vec<i64> = card_views.iter().map(|(cid, ..)| *cid).collect();
    let all_visible_selected = !visible_ids.is_empty()
        && visible_ids.iter().all(|cid| selected_ids.read().contains(cid));

    rsx! {
        Stylesheet { href: CSS }
        div { class: "card-list-page",

            div { class: "cardlist-toolbar",
                // Left: back + reset
                button {
                    class: "icon-btn icon-btn-lg",
                    title: "Back to decks",
                    onclick: move |_| {
                        nav.push(Route::DeckList);
                    },
                    "‹"
                }
                button {
                    class: "icon-btn icon-btn-lg danger desktop-only",
                    title: "Reset deck progress",
                    onclick: move |_| confirm_reset.set(true),
                    "↻"
                }
                if let Some(d) = deck.read().as_ref() {
                    span { class: "cardlist-deck-name", "{d.name}" }
                }

                input {
                    class: "cardlist-search-input",
                    r#type: "search",
                    placeholder: "Search cards…",
                    value: "{search_query}",
                    oninput: move |e| search_query.set(e.value()),
                }

                // Right: actions
                div { class: "cardlist-toolbar-right",
                    div { class: "cardlist-view-picker",
                        button {
                            class: if *view_mode.read() == "progress" { "icon-btn active" } else { "icon-btn" },
                            title: "Sort by progress (weakest first)",
                            onclick: move |_| {
                                view_mode.set("progress".into());
                                selected_tag_ids.write().clear();
                            },
                            "◒"
                        }
                        button {
                            class: if *view_mode.read() == "date" { "icon-btn active" } else { "icon-btn" },
                            title: "Sort by date added (oldest first)",
                            onclick: move |_| {
                                view_mode.set("date".into());
                                selected_tag_ids.write().clear();
                            },
                            "◷"
                        }
                        button {
                            class: if *view_mode.read() == "tags" { "icon-btn active" } else { "icon-btn" },
                            title: "Filter by tag",
                            onclick: move |_| {
                                view_mode.set("tags".into());
                                selected_tag_ids.write().clear();
                                spawn(async move {
                                    let (tags, pairs) = (get_deck_tags(id).await, get_card_tag_pairs(id).await);
                                    deck_tags.set(tags);
                                    let mut map: HashMap<i64, HashSet<i64>> = HashMap::new();
                                    for (card_id, tag_id) in pairs {
                                        map.entry(card_id).or_default().insert(tag_id);
                                    }
                                    card_tag_map.set(map);
                                });
                            },
                            "⌘"
                        }
                    }

                    button {
                        class: if *move_mode.read() { "icon-btn icon-btn-lg active desktop-only" } else { "icon-btn icon-btn-lg desktop-only" },
                        title: "Move cards to another deck",
                        onclick: move |_| {
                            let entering = !*move_mode.read();
                            move_mode.set(entering);
                            selected_ids.write().clear();
                            target_deck_id.set(None);
                            if entering {
                                delete_mode.set(false);
                                untag_mode.set(false);
                                spawn(async move {
                                    all_decks.set(get_decks().await);
                                });
                            }
                        },
                        "↔"
                    }

                    button {
                        class: if *delete_mode.read() { "icon-btn icon-btn-lg danger active desktop-only" } else { "icon-btn icon-btn-lg danger desktop-only" },
                        title: "Delete cards",
                        onclick: move |_| {
                            let entering = !*delete_mode.read();
                            delete_mode.set(entering);
                            selected_ids.write().clear();
                            if entering {
                                move_mode.set(false);
                                untag_mode.set(false);
                                target_deck_id.set(None);
                            }
                        },
                        "×"
                    }

                    button {
                        class: if *untag_mode.read() { "icon-btn icon-btn-lg active desktop-only" } else { "icon-btn icon-btn-lg desktop-only" },
                        title: "Remove all tags from cards",
                        onclick: move |_| {
                            let entering = !*untag_mode.read();
                            untag_mode.set(entering);
                            selected_ids.write().clear();
                            if entering {
                                move_mode.set(false);
                                delete_mode.set(false);
                                target_deck_id.set(None);
                                // The tag map drives the "n tag(s)" hint, so make
                                // sure it's loaded even outside the tag filter.
                                spawn(async move {
                                    let pairs = get_card_tag_pairs(id).await;
                                    let mut map: HashMap<i64, HashSet<i64>> = HashMap::new();
                                    for (card_id, tag_id) in pairs {
                                        map.entry(card_id).or_default().insert(tag_id);
                                    }
                                    card_tag_map.set(map);
                                });
                            }
                        },
                        "⊘"
                    }

                    button {
                        class: "icon-btn icon-btn-lg primary desktop-only",
                        title: "Add card",
                        onclick: move |_| {
                            nav.push(Route::CardEditorNew {
                                deck_id: id,
                            });
                        },
                        "＋"
                    }
                }
            }

            // Move-mode action bar
            if *move_mode.read() {
                div { class: "move-action-bar",
                    span { class: "move-action-hint", "{selected_ids.read().len()} card(s) selected" }
                    button {
                        class: "button button-secondary",
                        disabled: visible_ids.is_empty(),
                        onclick: {
                            let visible = visible_ids.clone();
                            move |_| {
                                let mut ids = selected_ids.write();
                                if all_visible_selected {
                                    for cid in &visible { ids.remove(cid); }
                                } else {
                                    ids.extend(visible.iter().copied());
                                }
                            }
                        },
                        if all_visible_selected { "Deselect all" } else { "Select all" }
                    }
                    select {
                        class: "move-deck-select",
                        onchange: move |e| {
                            target_deck_id.set(e.value().parse::<i64>().ok());
                        },
                        option { value: "", "— Select destination deck —" }
                        for d in all_decks.read().iter() {
                            if d.id != id {
                                option { value: "{d.id}", "{d.name}" }
                            }
                        }
                    }
                    button {
                        class: "button button-primary",
                        disabled: selected_ids.read().is_empty() || target_deck_id.read().is_none() || *moving.read(),
                        onclick: move |_| {
                            let ids: Vec<i64> = selected_ids.read().iter().copied().collect();
                            let Some(target) = *target_deck_id.read() else { return };
                            moving.set(true);
                            spawn(async move {
                                move_cards(ids, target).await;
                                let loaded = get_cards(id).await;
                                cards.set(loaded);
                                selected_ids.write().clear();
                                target_deck_id.set(None);
                                move_mode.set(false);
                                moving.set(false);
                            });
                        },
                        if *moving.read() {
                            "Moving…"
                        } else {
                            "Move"
                        }
                    }
                    button {
                        class: "button button-secondary",
                        onclick: move |_| {
                            move_mode.set(false);
                            selected_ids.write().clear();
                            target_deck_id.set(None);
                        },
                        "Cancel"
                    }
                }
            }

            // Delete-mode action bar
            if *delete_mode.read() {
                div { class: "move-action-bar",
                    span { class: "move-action-hint", "{selected_ids.read().len()} card(s) selected" }
                    button {
                        class: "button button-secondary",
                        disabled: visible_ids.is_empty(),
                        onclick: {
                            let visible = visible_ids.clone();
                            move |_| {
                                let mut ids = selected_ids.write();
                                if all_visible_selected {
                                    for cid in &visible { ids.remove(cid); }
                                } else {
                                    ids.extend(visible.iter().copied());
                                }
                            }
                        },
                        if all_visible_selected { "Deselect all" } else { "Select all" }
                    }
                    button {
                        class: "button button-danger",
                        disabled: selected_ids.read().is_empty() || *deleting.read(),
                        onclick: move |_| confirm_batch_delete.set(true),
                        if *deleting.read() {
                            "Deleting…"
                        } else {
                            "Delete selected"
                        }
                    }
                    button {
                        class: "button button-secondary",
                        onclick: move |_| {
                            delete_mode.set(false);
                            selected_ids.write().clear();
                        },
                        "Cancel"
                    }
                }
            }

            // Untag-mode action bar
            if *untag_mode.read() {
                div { class: "move-action-bar",
                    span { class: "move-action-hint",
                        {
                            let sel = selected_ids.read();
                            let map = card_tag_map.read();
                            let tagged = sel.iter().filter(|cid| {
                                map.get(cid).map(|t| !t.is_empty()).unwrap_or(false)
                            }).count();
                            format!("{} card(s) selected, {} tagged", sel.len(), tagged)
                        }
                    }
                    button {
                        class: "button button-secondary",
                        disabled: visible_ids.is_empty(),
                        onclick: {
                            let visible = visible_ids.clone();
                            move |_| {
                                let mut ids = selected_ids.write();
                                if all_visible_selected {
                                    for cid in &visible { ids.remove(cid); }
                                } else {
                                    ids.extend(visible.iter().copied());
                                }
                            }
                        },
                        if all_visible_selected { "Deselect all" } else { "Select all" }
                    }
                    button {
                        class: "button button-danger",
                        disabled: selected_ids.read().is_empty() || *untagging.read(),
                        onclick: move |_| confirm_untag.set(true),
                        if *untagging.read() {
                            "Removing…"
                        } else {
                            "Remove all tags"
                        }
                    }
                    button {
                        class: "button button-secondary",
                        onclick: move |_| {
                            untag_mode.set(false);
                            selected_ids.write().clear();
                        },
                        "Cancel"
                    }
                }
            }

            // Tag picker panel (shown when the tag filter is active)
            if *view_mode.read() == "tags" {
                div { class: "tag-picker-bar",
                    span { class: "tag-picker-hint", "Filter by tag:" }
                    if !deck_tags.read().is_empty() {
                        input {
                            class: "tag-search-input",
                            placeholder: "Filter tags…",
                            value: "{tag_search_query}",
                            oninput: move |e| tag_search_query.set(e.value()),
                        }
                    }
                    div { class: "tag-chips",
                        for tag in deck_tags.read().iter().filter(|t| {
                            let q = tag_search_query.read();
                            q.trim().is_empty() || t.name.to_lowercase().contains(&q.trim().to_lowercase())
                        }) {
                            {
                                let tag_id = tag.id;
                                let tag_name = tag.name.clone();
                                let is_selected = selected_tag_ids.read().contains(&tag_id);
                                rsx! {
                                    button {
                                        key: "{tag_id}",
                                        class: if is_selected { "tag-chip tag-chip-selected" } else { "tag-chip" },
                                        onclick: move |_| {
                                            let mut ids = selected_tag_ids.write();
                                            if ids.contains(&tag_id) {
                                                ids.remove(&tag_id);
                                            } else {
                                                ids.insert(tag_id);
                                            }
                                        },
                                        "{tag_name}"
                                    }
                                }
                            }
                        }
                        if deck_tags.read().is_empty() {
                            span { class: "tag-picker-empty", "No tags in this deck yet." }
                        }
                    }
                }
            }

            div { class: "cards-container",
                for (card_id , card_name , progress , _ , _ , cover) in card_views {
                    {
                        let is_selected = selected_ids.read().contains(&card_id);
                        let in_select_mode = *move_mode.read() || *delete_mode.read() || *untag_mode.read();
                        let card_class = if in_select_mode && is_selected {
                            "card-preview card-selected"
                        } else {
                            "card-preview"
                        };
                        rsx! {
                            div {
                                key: "{card_id}",
                                class: card_class,
                                onclick: move |_| {
                                    if in_select_mode {
                                        let mut ids = selected_ids.write();
                                        if ids.contains(&card_id) {
                                            ids.remove(&card_id);
                                        } else {
                                            ids.insert(card_id);
                                        }
                                    } else {
                                        nav.push(Route::CardView { id: card_id });
                                    }
                                },

                                if in_select_mode {
                                    div { class: "card-select-overlay",
                                        if is_selected {
                                            "✓"
                                        } else {
                                            ""
                                        }
                                    }
                                }

                                if let Some(ref src) = cover {
                                    img {
                                        class: "card-cover",
                                        src: "{image_url_from_virtual_path(src)}",
                                        alt: "",
                                    }
                                } else {
                                    div { class: "card-cover-placeholder" }
                                }

                                div { class: "card-body",
                                    p { class: "card-title", "{card_name}" }
                                    div { class: "card-progress",
                                        div { class: "card-progress-bar", style: "width: {progress}%;" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Confirmation dialog for progress reset
        if *confirm_reset.read() {
            div { class: "dialog-backdrop",
                div { class: "dialog",
                    h3 { class: "dialog-title", "Reset progress?" }
                    p { class: "dialog-body",
                        "This will set all scores back to zero for every card in this deck. This cannot be undone."
                    }
                    div { class: "dialog-actions",
                        button {
                            class: "button button-danger",
                            onclick: move |_| {
                                confirm_reset.set(false);
                                spawn(async move {
                                    reset_deck_progress(id).await;
                                    cards.set(get_cards(id).await);
                                });
                            },
                            "Reset"
                        }
                        button {
                            class: "button button-secondary",
                            onclick: move |_| confirm_reset.set(false),
                            "Cancel"
                        }
                    }
                }
            }
        }

        // Confirmation dialog for batch delete
        if *confirm_batch_delete.read() {
            div { class: "dialog-backdrop",
                div { class: "dialog",
                    h3 { class: "dialog-title", "Delete cards?" }
                    p { class: "dialog-body",
                        "This will permanently delete the {selected_ids.read().len()} selected card(s). This cannot be undone."
                    }
                    div { class: "dialog-actions",
                        button {
                            class: "button button-danger",
                            onclick: move |_| {
                                confirm_batch_delete.set(false);
                                let ids: Vec<i64> = selected_ids.read().iter().copied().collect();
                                deleting.set(true);
                                spawn(async move {
                                    for cid in ids {
                                        delete_card(cid).await;
                                    }
                                    cards.set(get_cards(id).await);
                                    selected_ids.write().clear();
                                    delete_mode.set(false);
                                    deleting.set(false);
                                });
                            },
                            "Delete"
                        }
                        button {
                            class: "button button-secondary",
                            onclick: move |_| confirm_batch_delete.set(false),
                            "Cancel"
                        }
                    }
                }
            }
        }

        // Confirmation dialog for batch tag removal
        if *confirm_untag.read() {
            div { class: "dialog-backdrop",
                div { class: "dialog",
                    h3 { class: "dialog-title", "Remove all tags?" }
                    p { class: "dialog-body",
                        "This will detach every tag from the {selected_ids.read().len()} selected card(s). The tags themselves are kept, and the cards are not deleted."
                    }
                    div { class: "dialog-actions",
                        button {
                            class: "button button-danger",
                            onclick: move |_| {
                                confirm_untag.set(false);
                                let ids: Vec<i64> = selected_ids.read().iter().copied().collect();
                                untagging.set(true);
                                spawn(async move {
                                    for cid in ids {
                                        // An empty tag list detaches everything on the card.
                                        sync_card_tags(cid, Vec::new()).await;
                                    }
                                    // Tag membership changed, so the deck's tag list
                                    // and the card→tag map both need reloading.
                                    let (tags, pairs) = (get_deck_tags(id).await, get_card_tag_pairs(id).await);
                                    deck_tags.set(tags);
                                    let mut map: HashMap<i64, HashSet<i64>> = HashMap::new();
                                    for (card_id, tag_id) in pairs {
                                        map.entry(card_id).or_default().insert(tag_id);
                                    }
                                    card_tag_map.set(map);
                                    cards.set(get_cards(id).await);
                                    selected_ids.write().clear();
                                    untag_mode.set(false);
                                    untagging.set(false);
                                });
                            },
                            "Remove tags"
                        }
                        button {
                            class: "button button-secondary",
                            onclick: move |_| confirm_untag.set(false),
                            "Cancel"
                        }
                    }
                }
            }
        }
    }
}
