use std::rc::Rc;
use std::collections::HashMap;
use dioxus::prelude::*;
use dioxus::history::{History, MemoryHistory};
use dioxus::router::components::HistoryProvider;
use crate::app::{Route, PENDING_TAB_ROUTE, TAB_TITLES, TabId};

/// A single browser-style tab: an independent instance of the whole app, each
/// with its own in-memory router/history. Kept mounted while inactive so its
/// navigation and page state persist when you switch away and back.
#[component]
pub fn TabInstance(id: usize) -> Element {
    // Make this tab's id reachable from routed pages so they can set the tab title.
    use_context_provider(|| TabId(id));
    // Consume (once) any route handed off when this tab was opened. New tabs
    // created via `TabsCtx::open` start here; the first/default tab starts home.
    let initial = use_hook(|| PENDING_TAB_ROUTE.write().take());
    rsx! {
        HistoryProvider {
            history: move |_| {
                let history = match initial.clone() {
                    Some(route) => MemoryHistory::with_initial_path(route),
                    None => MemoryHistory::default(),
                };
                Rc::new(history) as Rc<dyn History>
            },
            Router::<Route> {}
        }
    }
}

/// The strip of tabs plus the "new tab" button. Holds no state of its own — it
/// mutates the tab list / active id owned by the shell.
#[component]
pub fn TabStrip(
    tabs: Signal<Vec<usize>>,
    active: Signal<usize>,
    next_id: Signal<usize>,
    tab_names: Signal<HashMap<usize, String>>,
) -> Element {
    let current = *active.read();
    let multiple = tabs.read().len() > 1;

    rsx! {
        div { class: "tab-strip",
            for (pos, id) in tabs.read().iter().copied().enumerate() {
                {
                    let is_active = id == current;
                    // Manual rename wins; otherwise show the content-derived title.
                    let name = tab_names.read().get(&id).filter(|s| !s.is_empty()).cloned()
                        .or_else(|| TAB_TITLES.read().get(&id).cloned())
                        .unwrap_or_default();
                    rsx! {
                        div {
                            key: "{id}",
                            class: if is_active { "tab-item tab-item-active" } else { "tab-item" },
                            onclick: move |_| active.set(id),
                            input {
                                class: "tab-item-label",
                                value: "{name}",
                                placeholder: "Tab {pos + 1}",
                                // Clicking the field focuses the tab too, so editing
                                // never happens on a tab you can't see.
                                onclick: move |_| active.set(id),
                                oninput: move |e| { tab_names.write().insert(id, e.value()); },
                            }
                            if multiple {
                                button {
                                    class: "tab-item-close",
                                    title: "Close tab",
                                    onclick: move |e: Event<MouseData>| {
                                        e.stop_propagation();
                                        close_tab(tabs, active, tab_names, id);
                                    },
                                    "×"
                                }
                            }
                        }
                    }
                }
            }
            button {
                class: "tab-strip-new",
                title: "New tab",
                onclick: move |_| {
                    let id = *next_id.read();
                    next_id.set(id + 1);
                    tabs.write().push(id);
                    active.set(id);
                },
                "＋"
            }
        }
    }
}

/// Close tab `id`. Refuses to close the last remaining tab, and moves focus to a
/// neighbouring tab if the closed one was active.
fn close_tab(
    mut tabs: Signal<Vec<usize>>,
    mut active: Signal<usize>,
    mut tab_names: Signal<HashMap<usize, String>>,
    id: usize,
) {
    let mut list = tabs.write();
    if list.len() <= 1 {
        return;
    }
    let Some(idx) = list.iter().position(|x| *x == id) else {
        return;
    };
    list.remove(idx);
    tab_names.write().remove(&id);
    TAB_TITLES.write().remove(&id);
    if *active.read() == id {
        // Focus the tab that slid into this slot, else the previous one.
        let neighbour = list
            .get(idx)
            .or_else(|| idx.checked_sub(1).and_then(|i| list.get(i)))
            .copied();
        drop(list);
        if let Some(n) = neighbour {
            active.set(n);
        }
    }
}
