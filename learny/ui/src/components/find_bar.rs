use dioxus::prelude::*;
use dioxus::document::Stylesheet;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

static CSS: Asset = asset!("/assets/find_bar.css");

/// Call WebKit/Chromium's built-in `window.find()` to jump to the next match on
/// the current page. Returns whether a match was found. web-sys doesn't expose
/// `window.find`, so we reach it through the JS reflection API.
fn page_find(query: &str, backwards: bool) -> bool {
    let Some(win) = web_sys::window() else { return false };
    let Ok(func) = js_sys::Reflect::get(&win, &JsValue::from_str("find")) else { return false };
    let Ok(func) = func.dyn_into::<js_sys::Function>() else { return false };
    // window.find(text, caseSensitive, backwards, wrapAround, wholeWord, searchInFrames, showDialog)
    let args = js_sys::Array::new();
    args.push(&JsValue::from_str(query));
    args.push(&JsValue::from_bool(false));     // caseSensitive
    args.push(&JsValue::from_bool(backwards)); // backwards
    args.push(&JsValue::from_bool(true));      // wrapAround
    args.push(&JsValue::from_bool(false));     // wholeWord
    args.push(&JsValue::from_bool(false));     // searchInFrames
    args.push(&JsValue::from_bool(false));     // showDialog
    func.apply(&win, &args).ok().and_then(|v| v.as_bool()).unwrap_or(false)
}

/// Highlight every occurrence of `query` on the page (case-insensitive) via the
/// CSS Custom Highlight API — no DOM mutation, so Dioxus' virtual DOM is
/// untouched. Styled by `::highlight(find-match)` in find_bar.css. Pass an
/// empty query to clear. No-op on engines without the API.
fn highlight_all(query: &str) {
    let Ok(q) = serde_json::to_string(query) else { return };
    let js = format!(
        r#"(() => {{
            if (!('highlights' in CSS)) return;
            CSS.highlights.delete('find-match');
            const q = {q}.toLowerCase();
            if (!q) return;
            const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT, {{
                acceptNode(n) {{
                    const p = n.parentElement;
                    return (!p || p.closest('script,style,.find-bar'))
                        ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT;
                }}
            }});
            const ranges = [];
            let node;
            while ((node = walker.nextNode())) {{
                const text = node.textContent.toLowerCase();
                let i = 0;
                while ((i = text.indexOf(q, i)) !== -1) {{
                    const r = new Range();
                    r.setStart(node, i);
                    r.setEnd(node, i + q.length);
                    ranges.push(r);
                    i += q.length;
                }}
            }}
            if (ranges.length) CSS.highlights.set('find-match', new Highlight(...ranges));
        }})()"#
    );
    let _ = js_sys::eval(&js);
}

/// Focus (and select) the find input, deferred a frame so it runs after Dioxus
/// has rendered the bar into the DOM.
fn focus_find_input() {
    let _ = js_sys::eval(
        "requestAnimationFrame(() => { \
             const el = document.querySelector('.find-bar-input'); \
             if (el) { el.focus(); el.select(); } \
         })",
    );
}

/// A lightweight in-page find bar, opened with Cmd-F (Ctrl-F on Win/Linux).
/// Enter = next match, Shift+Enter = previous, Esc = close.
#[component]
pub fn FindBar() -> Element {
    let mut visible = use_signal(|| false);
    let mut query = use_signal(String::new);
    let mut not_found = use_signal(|| false);

    // Attach a single global keydown listener for the app's lifetime.
    use_hook(move || {
        let closure = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(
            move |e: web_sys::KeyboardEvent| {
                if (e.meta_key() || e.ctrl_key()) && e.key().eq_ignore_ascii_case("f") {
                    e.prevent_default();
                    visible.set(true);
                    // Focus the input whether the bar just opened or was
                    // already showing (re-selects the current query), and
                    // restore highlights for a query kept from last time.
                    focus_find_input();
                    highlight_all(&query.peek());
                }
            },
        );
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            let _ = doc.add_event_listener_with_callback(
                "keydown",
                closure.as_ref().unchecked_ref(),
            );
        }
        // Leak the closure so it stays alive (the bar exists for the whole session).
        closure.forget();
    });

    let mut run_find = move |backwards: bool| {
        let q = query.read().clone();
        if !q.is_empty() {
            not_found.set(!page_find(&q, backwards));
        }
    };

    rsx! {
        Stylesheet { href: CSS }
        if *visible.read() {
            div { class: "find-bar",
                input {
                    class: if *not_found.read() { "find-bar-input find-bar-empty" } else { "find-bar-input" },
                    placeholder: "Find on page…",
                    autofocus: true,
                    value: "{query}",
                    onmounted: move |e| async move {
                        let _ = e.set_focus(true).await;
                    },
                    oninput: move |e| {
                        query.set(e.value());
                        not_found.set(false);
                        highlight_all(&e.value());
                    },
                    onkeydown: move |e| {
                        match e.key() {
                            Key::Enter => run_find(e.modifiers().shift()),
                            Key::Escape => {
                                visible.set(false);
                                highlight_all("");
                            }
                            _ => {}
                        }
                    },
                }
                button {
                    class: "find-bar-btn",
                    title: "Previous match (Shift+Enter)",
                    onclick: move |_| run_find(true),
                    "▲"
                }
                button {
                    class: "find-bar-btn",
                    title: "Next match (Enter)",
                    onclick: move |_| run_find(false),
                    "▼"
                }
                button {
                    class: "find-bar-btn",
                    title: "Close (Esc)",
                    onclick: move |_| { visible.set(false); highlight_all(""); },
                    "✕"
                }
            }
        }
    }
}
