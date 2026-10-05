use dioxus::prelude::*;
use crate::api::add_deck;

#[component]
pub fn CreateDeck(on_done: EventHandler<()>) -> Element {
    let mut deck_name = use_signal(|| "".to_string());

    let do_submit = move || {
        let name = deck_name.read().trim().to_string();
        if !name.is_empty() {
            spawn(async move {
                add_deck(name).await;
                on_done.call(());
            });
        }
    };

    rsx! {
        div { class: "dialog-backdrop",
            div { class: "dialog",
                h3 { class: "dialog-title", "New deck" }
                input {
                    class: "dialog-input",
                    placeholder: "Deck name…",
                    autofocus: true,
                    value: "{deck_name}",
                    oninput: move |ev| deck_name.set(ev.value()),
                    onkeydown: move |e| {
                        if e.key() == Key::Enter { do_submit(); }
                    },
                }
                div { class: "dialog-actions",
                    button {
                        class: "button button-secondary",
                        onclick: move |_| on_done.call(()),
                        "Cancel"
                    }
                    button {
                        class: "button button-primary",
                        onclick: move |_| do_submit(),
                        "Create"
                    }
                }
            }
        }
    }
}
