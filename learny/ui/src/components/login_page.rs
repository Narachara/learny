use dioxus::prelude::*;
use dioxus::document::Stylesheet;
use crate::api::{login, /*register,*/ UserInfo};

static CSS: Asset = asset!("/assets/auth.css");

// ---------------------------------------------------------------------------
// Submit logic lives outside the component so it can be shared between the
// button onclick and the Enter-key handler on each input field.
// ---------------------------------------------------------------------------

/// Reset the browser URL to "/" so the Router always lands on DeckList
/// after a fresh login, regardless of what URL was open before.
fn reset_url_to_home() {
    if let Some(win) = web_sys::window() {
        let _ = win.history().and_then(|h| {
            h.push_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some("/"))
        });
    }
}

fn submit_login(
    username:      Signal<String>,
    password:      Signal<String>,
    mut error_msg: Signal<Option<String>>,
    mut loading:   Signal<bool>,
    on_login:      EventHandler<UserInfo>,
) {
    let u = username.read().trim().to_string();
    let p = password.read().clone();
    if u.is_empty() || p.is_empty() {
        error_msg.set(Some("Please fill in all fields.".into()));
        return;
    }
    loading.set(true);
    error_msg.set(None);
    spawn(async move {
        match login(u, p).await {
            Some(user) => { reset_url_to_home(); on_login.call(user); }
            None => {
                error_msg.set(Some("Invalid username or password.".into()));
                loading.set(false);
            }
        }
    });
}

/*
fn submit_register(
    username:      Signal<String>,
    password:      Signal<String>,
    token:         Signal<String>,
    mut error_msg: Signal<Option<String>>,
    mut loading:   Signal<bool>,
    on_login:      EventHandler<UserInfo>,
) {
    let u = username.read().trim().to_string();
    let p = password.read().clone();
    let t = token.read().clone();
    if u.is_empty() || p.len() < 6 {
        error_msg.set(Some("Username required and password must be ≥ 6 characters.".into()));
        return;
    }
    loading.set(true);
    error_msg.set(None);
    spawn(async move {
        match register(u, p, t).await {
            Some(user) => { reset_url_to_home(); on_login.call(user); }
            None => {
                error_msg.set(Some("Registration failed. Username may already be taken or token invalid.".into()));
                loading.set(false);
            }
        }
    });
}
*/

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

#[component]
pub fn LoginPage(on_login: EventHandler<UserInfo>) -> Element {
    let mut username  = use_signal(String::new);
    let mut password  = use_signal(String::new);
    let error_msg = use_signal(|| Option::<String>::None);
    let loading       = use_signal(|| false);

    rsx! {
        Stylesheet { href: CSS }
        div { class: "auth-page",
            div { class: "auth-card",

                h1 { class: "auth-title", "learny" }
                p  { class: "auth-subtitle", "Sign in to continue" }

                div { class: "auth-fields",
                    input {
                        class: "auth-input",
                        r#type: "text",
                        placeholder: "Username or Email",
                        value: "{username}",
                        oninput: move |e| username.set(e.value()),
                        onkeydown: move |e| {
                            if e.key() == Key::Enter {
                                submit_login(username, password, error_msg, loading, on_login);
                            }
                        },
                        autofocus: true,
                    }
                    input {
                        class: "auth-input",
                        r#type: "password",
                        placeholder: "Password",
                        value: "{password}",
                        oninput: move |e| password.set(e.value()),
                        onkeydown: move |e| {
                            if e.key() == Key::Enter {
                                submit_login(username, password, error_msg, loading, on_login);
                            }
                        },
                    }
                }

                if let Some(msg) = error_msg.read().as_deref() {
                    p { class: "auth-error", "{msg}" }
                }

                button {
                    class: "button button-primary auth-submit",
                    disabled: *loading.read(),
                    onclick: move |_| submit_login(username, password, error_msg, loading, on_login),
                    if *loading.read() { "Signing in…" } else { "Sign In" }
                }

                /* REGISTRATION DISABLED — uncomment to re-enable
                button {
                    class: "auth-toggle",
                    onclick: move |_| {
                        let cur = *show_register.read();
                        show_register.set(!cur);
                        error_msg.set(None);
                    },
                    if *show_register.read() {
                        "Already have an account? Sign in"
                    } else {
                        "No account yet? Register"
                    }
                }
                */
            }
        }
    }
}
