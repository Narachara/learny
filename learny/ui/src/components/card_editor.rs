use std::collections::HashMap;
use dioxus::prelude::*;
use dioxus::document::Stylesheet;
use domain_flashcard::{ Card, Block };
use crate::components::block_editor::InsertBlockKind;

static CSS: Asset = asset!("/assets/card_editor.css");
static BLOCKS_CSS: Asset = asset!("/assets/blocks.css");
use crate::components::{BlockEditor, MediaUploadOverlay};
use crate::app::Route;
use crate::api::{
    get_card,
    get_deck,
    add_card,
    update_card_metadata,
    save_card_blocks,
    pick_image,
    pick_audio,
    pick_video,
    pick_archive,
    get_all_tags,
    get_card_tags,
    get_card_tags_with_position,
    sync_card_tags,
    set_card_tag_position,
};

async fn create_block(
    kind: InsertBlockKind,
    mut uploading: Signal<Option<InsertBlockKind>>,
) -> Option<Block> {
    if matches!(kind, InsertBlockKind::Text) {
        return Some(Block::Text { value: "".into() });
    }
    uploading.set(Some(kind));
    let block = match kind {
        InsertBlockKind::Text => unreachable!(),
        InsertBlockKind::Image => {
            let path = pick_image().await;
            (!path.is_empty()).then(|| Block::Image { src: path, scale: None })
        }
        InsertBlockKind::Audio => {
            let path = pick_audio().await;
            (!path.is_empty()).then(|| Block::Audio { src: path })
        }
        InsertBlockKind::Video => {
            let path = pick_video().await;
            (!path.is_empty()).then(|| Block::Video { src: path })
        }
        InsertBlockKind::File => {
            let path = pick_archive().await;
            (!path.is_empty()).then(|| Block::File { path })
        }
    };
    uploading.set(None);
    block
}

#[derive(Clone, PartialEq, Copy)]
pub enum EditorMode {
    New {
        deck_id: i64,
    },
    Edit {
        card_id: i64,
    },
}

#[component]
pub fn CardEditorNew(deck_id: i64) -> Element {
    rsx! {
        CardEditor { mode: EditorMode::New { deck_id } }
    }
}

#[component]
pub fn CardEditorEdit(id: i64) -> Element {
    rsx! {
        CardEditor { mode: EditorMode::Edit { card_id: id } }
    }
}

#[component]
pub fn CardEditor(mode: EditorMode) -> Element {
    let nav = navigator();
    rsx! {
        div { class: "card-editor-page",
            CardEditorPanel {
                mode,
                on_saved: move |id| { nav.push(Route::CardView { id }); },
            }
        }
    }
}

/// Reusable editor panel — renders the full card editor UI without any page
/// wrapper or built-in navigation. The caller provides `on_saved` to decide
/// what happens after a successful save (navigate away, stay, switch mode…).
#[component]
pub fn CardEditorPanel(mode: EditorMode, on_saved: EventHandler<i64>) -> Element {
    let mut card = use_signal(|| None::<Card>);

    match mode {
        EditorMode::New { deck_id } => {
            card.set(Some(Card::new_empty(deck_id)));
        }
        EditorMode::Edit { card_id } => {
            use_effect(move || {
                spawn(async move {
                    let loaded = get_card(card_id).await;
                    card.set(Some(loaded));
                });
            });
        }
    }

    if card.read().is_none() {
        return rsx! {
            div { class: "loading", "Loading card..." }
        };
    }

    let c = card.read().as_ref().unwrap().clone();
    let deck_id = c.deck_id;
    let mut card_name = use_signal(|| c.name.clone());
    let mut deck_name = use_signal(|| String::new());

    use_effect(move || {
        spawn(async move {
            let deck = get_deck(deck_id).await;
            deck_name.set(deck.name);
        });
    });
    let mut front_blocks = use_signal(|| c.front_blocks.clone());
    let mut back_blocks = use_signal(|| c.back_blocks.clone());
    // Seed a new card's tags with any tags handed off from the knowledge map.
    let mut tag_chips = use_signal(|| match mode {
        EditorMode::New { .. } => std::mem::take(&mut *crate::app::PENDING_NEW_CARD_TAGS.write()),
        EditorMode::Edit { .. } => Vec::new(),
    });
    let mut tag_draft = use_signal(|| String::new());
    let mut all_tags: Signal<Vec<String>> = use_signal(|| Vec::new());
    let mut show_suggestions = use_signal(|| false);
    let mut suggestion_active_idx: Signal<Option<usize>> = use_signal(|| None);
    let mut uploading: Signal<Option<InsertBlockKind>> = use_signal(|| None);
    // A tag's position within its own progression (lower = earlier), keyed
    // by lowercase tag name so it survives the chip being a plain String.
    // Stored as the raw text the user typed (not a parsed i64) so a
    // controlled input never silently discards an in-progress edit (e.g.
    // a lone "-" while typing a negative number); parsed to i64 on save.
    let mut tag_positions: Signal<HashMap<String, String>> = use_signal(HashMap::new);

    use_effect(move || {
        spawn(async move {
            all_tags.set(get_all_tags().await.into_iter().map(|t| t.name).collect());
        });
    });

    if let EditorMode::Edit { card_id } = mode {
        use_effect(move || {
            spawn(async move {
                let infos = get_card_tags_with_position(card_id).await;
                if infos.is_empty() {
                    // Fallback: the positioned fetch failed or the card simply
                    // has no tags — load plain tags so chips still appear.
                    let tags = get_card_tags(card_id).await;
                    tag_chips.set(tags.into_iter().map(|t| t.name).collect());
                } else {
                    tag_chips.set(infos.iter().map(|i| i.tag.name.clone()).collect());
                    tag_positions.set(infos.iter().map(|i| (i.tag.name.to_lowercase(), i.position.to_string())).collect());
                }
            });
        });
    }

    let mut commit_draft = move || {
        let draft = tag_draft.read().trim().to_string();
        if !draft.is_empty() {
            let mut chips = tag_chips.write();
            if !chips.iter().any(|c: &String| c.eq_ignore_ascii_case(&draft)) {
                chips.push(draft);
            }
        }
        tag_draft.set(String::new());
    };

    rsx! {
        Stylesheet { href: CSS }
        Stylesheet { href: BLOCKS_CSS }

        div { class: "card-editor-titleblock",
            h1 {
                match mode {
                    EditorMode::New { .. } => "Create New Card",
                    EditorMode::Edit { .. } => "Edit Card",
                }
            }
            if !deck_name.read().is_empty() {
                span { class: "card-deck-chip", "{deck_name}" }
            }
        }

        if let Some(kind) = *uploading.read() {
            MediaUploadOverlay { kind }
        }

        div { class: "card-field",
            label { "Card Name" }
            input {
                value: "{card_name}",
                oninput: move |evt| card_name.set(evt.value()),
            }
        }

        div { class: "card-field",
            label { "Tags" }
            div { class: "tag-input-wrap",
                for (i, chip) in tag_chips.read().iter().cloned().enumerate() {
                    {
                        let key_name = chip.to_lowercase();
                        let pos_val = tag_positions.read().get(&key_name).cloned().unwrap_or_default();
                        let key_name_input = key_name.clone();
                        rsx! {
                            span { class: "tag-chip",
                                "{chip}"
                                input {
                                    class: "tag-chip-order",
                                    r#type: "text",
                                    inputmode: "numeric",
                                    title: "Order within this tag's progression (lower = earlier)",
                                    placeholder: "#",
                                    value: "{pos_val}",
                                    oninput: move |e| {
                                        tag_positions.write().insert(key_name_input.clone(), e.value());
                                    },
                                }
                                button {
                                    class: "tag-chip-remove",
                                    r#type: "button",
                                    onclick: move |_| { tag_chips.write().remove(i); },
                                    "×"
                                }
                            }
                        }
                    }
                }
                input {
                    class: "tag-draft-input",
                    autocomplete: "off",
                    value: "{tag_draft}",
                    placeholder: if tag_chips.read().is_empty() { "Add tags…" } else { "" },
                    oninput: move |e| {
                        let v = e.value();
                        suggestion_active_idx.set(None);
                        if v.ends_with(',') {
                            tag_draft.set(v.trim_end_matches(',').trim().to_string());
                            commit_draft();
                            show_suggestions.set(false);
                        } else {
                            tag_draft.set(v.clone());
                            show_suggestions.set(!v.trim().is_empty());
                        }
                    },
                    onkeydown: move |e| {
                        if e.key() == Key::ArrowDown {
                            e.prevent_default();
                            if *show_suggestions.read() {
                                let draft = tag_draft.read().to_lowercase();
                                let chips = tag_chips.read();
                                let count = all_tags.read().iter()
                                    .filter(|t| !t.is_empty() && t.to_lowercase().contains(&draft) && !chips.iter().any(|c| c.eq_ignore_ascii_case(t)))
                                    .take(8)
                                    .count();
                                if count > 0 {
                                    let cur = *suggestion_active_idx.read();
                                    let next = cur.map_or(0, |i| (i + 1).min(count - 1));
                                    suggestion_active_idx.set(Some(next));
                                }
                            }
                        } else if e.key() == Key::ArrowUp {
                            e.prevent_default();
                            let cur = *suggestion_active_idx.read();
                            match cur {
                                None | Some(0) => suggestion_active_idx.set(None),
                                Some(i) => suggestion_active_idx.set(Some(i - 1)),
                            }
                        } else if e.key() == Key::Enter {
                            let active = *suggestion_active_idx.read();
                            if let Some(idx) = active {
                                let draft = tag_draft.read().to_lowercase();
                                let chips_snap = tag_chips.read().clone();
                                let selected = all_tags.read().iter()
                                    .filter(|t| !t.is_empty() && t.to_lowercase().contains(&draft) && !chips_snap.iter().any(|c| c.eq_ignore_ascii_case(t)))
                                    .take(8)
                                    .nth(idx)
                                    .cloned();
                                if let Some(tag) = selected {
                                    let mut chips = tag_chips.write();
                                    if !chips.iter().any(|c| c.eq_ignore_ascii_case(&tag)) {
                                        chips.push(tag);
                                    }
                                    drop(chips);
                                    tag_draft.set(String::new());
                                    show_suggestions.set(false);
                                    suggestion_active_idx.set(None);
                                }
                            } else {
                                commit_draft();
                                show_suggestions.set(false);
                            }
                        } else if e.key() == Key::Escape {
                            show_suggestions.set(false);
                            suggestion_active_idx.set(None);
                        } else if e.key() == Key::Backspace && tag_draft.read().is_empty() {
                            let mut chips = tag_chips.write();
                            chips.pop();
                        }
                    },
                    onfocus: move |_| {
                        if !tag_draft.read().trim().is_empty() {
                            show_suggestions.set(true);
                        }
                    },
                    onblur: move |_| {
                        show_suggestions.set(false);
                        commit_draft();
                    },
                }

                {
                    let draft = tag_draft.read().to_lowercase();
                    let chips = tag_chips.read();
                    let suggestions: Vec<String> = all_tags.read().iter()
                        .filter(|t| {
                            !t.is_empty()
                            && t.to_lowercase().contains(&draft)
                            && !chips.iter().any(|c| c.eq_ignore_ascii_case(t))
                        })
                        .cloned()
                        .take(8)
                        .collect();

                    if *show_suggestions.read() && !suggestions.is_empty() {
                        rsx! {
                            div { class: "tag-suggestions",
                                for (si, suggestion) in suggestions.into_iter().enumerate() {
                                    button {
                                        class: if suggestion_active_idx.read().map_or(false, |i| i == si) { "tag-suggestion-item tag-suggestion-active" } else { "tag-suggestion-item" },
                                        r#type: "button",
                                        onmousedown: move |_| {
                                            let mut chips = tag_chips.write();
                                            if !chips.iter().any(|c| c.eq_ignore_ascii_case(&suggestion)) {
                                                chips.push(suggestion.clone());
                                            }
                                            tag_draft.set(String::new());
                                            show_suggestions.set(false);
                                            suggestion_active_idx.set(None);
                                        },
                                        "{suggestion}"
                                    }
                                }
                            }
                        }
                    } else {
                        rsx! {}
                    }
                }
            }
        }

        h2 { "Front Blocks" }

        for (i , block) in front_blocks.read().iter().cloned().enumerate() {
            BlockEditor {
                block,

                on_update: {
                    let mut front_blocks = front_blocks.clone();
                    let index = i;

                    move |new_block| {
                        front_blocks
                            .with_mut(|blocks| {
                                blocks[index] = new_block;
                            });
                    }
                },

                on_remove: {
                    let mut front_blocks = front_blocks.clone();
                    move |_| {
                        front_blocks.write().remove(i);
                    }
                },

                on_insert_above: {
                    let front_blocks = front_blocks.clone();
                    move |kind| {
                        let mut front_blocks = front_blocks.clone();
                        spawn(async move {
                            if let Some(block) = create_block(kind, uploading).await {
                                front_blocks.write().insert(i, block);
                            }
                        });
                    }
                },

                on_insert_below: {
                    let front_blocks = front_blocks.clone();
                    move |kind| {
                        let mut front_blocks = front_blocks.clone();
                        spawn(async move {
                            if let Some(block) = create_block(kind, uploading).await {
                                front_blocks.write().insert(i + 1, block);
                            }
                        });
                    }
                },
            }
        }

        if front_blocks.read().is_empty() {
        div { class: "add-block-buttons",
            button {
                class: "icon-btn", title: "Add text block",
                onclick: move |_| { front_blocks.write().push(Block::Text { value: "".into() }); },
                "T"
            }
            button {
                class: "icon-btn", title: "Add image block",
                onclick: move |_| {
                    spawn(async move {
                        uploading.set(Some(InsertBlockKind::Image));
                        let path = pick_image().await;
                        uploading.set(None);
                        if !path.is_empty() { front_blocks.write().push(Block::Image { src: path, scale: None }); }
                    });
                },
                "⬚"
            }
            button {
                class: "icon-btn", title: "Add audio block",
                onclick: move |_| {
                    spawn(async move {
                        uploading.set(Some(InsertBlockKind::Audio));
                        let path = pick_audio().await;
                        uploading.set(None);
                        if !path.is_empty() { front_blocks.write().push(Block::Audio { src: path }); }
                    });
                },
                "♪"
            }
            button {
                class: "icon-btn", title: "Add video block",
                onclick: move |_| {
                    spawn(async move {
                        uploading.set(Some(InsertBlockKind::Video));
                        let path = pick_video().await;
                        uploading.set(None);
                        if !path.is_empty() { front_blocks.write().push(Block::Video { src: path }); }
                    });
                },
                "▶"
            }
            button {
                class: "icon-btn", title: "Add file block",
                onclick: move |_| {
                    spawn(async move {
                        uploading.set(Some(InsertBlockKind::File));
                        let path = pick_archive().await;
                        uploading.set(None);
                        if !path.is_empty() { front_blocks.write().push(Block::File { path }); }
                    });
                },
                "⊟"
            }
        }
        }

        h2 { "Back Blocks" }

        for (i , block) in back_blocks.read().iter().cloned().enumerate() {
            BlockEditor {
                block,

                on_update: {
                    let mut back_blocks = back_blocks.clone();
                    move |new_block| {
                        back_blocks.write()[i] = new_block;
                    }
                },

                on_remove: {
                    let mut back_blocks = back_blocks.clone();
                    move |_| {
                        back_blocks.write().remove(i);
                    }
                },

                on_insert_above: {
                    let back_blocks = back_blocks.clone();
                    move |kind| {
                        let mut back_blocks = back_blocks.clone();
                        spawn(async move {
                            if let Some(block) = create_block(kind, uploading).await {
                                back_blocks.write().insert(i, block);
                            }
                        });
                    }
                },

                on_insert_below: {
                    let back_blocks = back_blocks.clone();
                    move |kind| {
                        let mut back_blocks = back_blocks.clone();
                        spawn(async move {
                            if let Some(block) = create_block(kind, uploading).await {
                                back_blocks.write().insert(i + 1, block);
                            }
                        });
                    }
                },
            }
        }

        if back_blocks.read().is_empty() {
        div { class: "add-block-buttons",
            button {
                class: "icon-btn", title: "Add text block",
                onclick: move |_| { back_blocks.write().push(Block::Text { value: "".into() }); },
                "T"
            }
            button {
                class: "icon-btn", title: "Add image block",
                onclick: move |_| {
                    spawn(async move {
                        uploading.set(Some(InsertBlockKind::Image));
                        let path = pick_image().await;
                        uploading.set(None);
                        if !path.is_empty() { back_blocks.write().push(Block::Image { src: path, scale: None }); }
                    });
                },
                "⬚"
            }
            button {
                class: "icon-btn", title: "Add audio block",
                onclick: move |_| {
                    spawn(async move {
                        uploading.set(Some(InsertBlockKind::Audio));
                        let path = pick_audio().await;
                        uploading.set(None);
                        if !path.is_empty() { back_blocks.write().push(Block::Audio { src: path }); }
                    });
                },
                "♪"
            }
            button {
                class: "icon-btn", title: "Add video block",
                onclick: move |_| {
                    spawn(async move {
                        uploading.set(Some(InsertBlockKind::Video));
                        let path = pick_video().await;
                        uploading.set(None);
                        if !path.is_empty() { back_blocks.write().push(Block::Video { src: path }); }
                    });
                },
                "▶"
            }
            button {
                class: "icon-btn", title: "Add file block",
                onclick: move |_| {
                    spawn(async move {
                        uploading.set(Some(InsertBlockKind::File));
                        let path = pick_archive().await;
                        uploading.set(None);
                        if !path.is_empty() { back_blocks.write().push(Block::File { path }); }
                    });
                },
                "⊟"
            }
        }
        }

        button {
            class: "button button-primary save-button",

            onclick: move |_| {
                let name = card_name.read().clone();
                let front = front_blocks.read().clone();
                let back = back_blocks.read().clone();
                let draft = tag_draft.read().trim().to_string();
                let mut tag_names = tag_chips.read().clone();
                if !draft.is_empty() && !tag_names.iter().any(|c| c.eq_ignore_ascii_case(&draft)) {
                    tag_names.push(draft);
                }
                let positions = tag_positions.read().clone();

                spawn(async move {
                    let saved_id = match mode {
                        EditorMode::New { deck_id } => {
                            let id = add_card(deck_id, name).await;
                            save_card_blocks(id, &front, &back).await;
                            if !tag_names.is_empty() {
                                let synced = sync_card_tags(id, tag_names).await;
                                for t in &synced {
                                    if let Some(pos) = positions.get(&t.name.to_lowercase()).and_then(|s| s.trim().parse::<i64>().ok()) {
                                        set_card_tag_position(t.id, id, pos).await;
                                    }
                                }
                            }
                            id
                        }

                        EditorMode::Edit { card_id } => {
                            update_card_metadata(card_id, name).await;
                            save_card_blocks(card_id, &front, &back).await;
                            let synced = sync_card_tags(card_id, tag_names).await;
                            for t in &synced {
                                if let Some(pos) = positions.get(&t.name.to_lowercase()).and_then(|s| s.trim().parse::<i64>().ok()) {
                                    set_card_tag_position(t.id, card_id, pos).await;
                                }
                            }
                            card_id
                        }
                    };
                    on_saved.call(saved_id);
                });
            },

            "Save Card"
        }
    }
}
