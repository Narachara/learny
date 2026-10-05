use dioxus::prelude::*;
use urlencoding::encode;
use domain_flashcard::*;
use crate::api::{ download_file };
use pulldown_cmark::{Parser, html, Options};


// Markdown rendering is only reached from the server build's `render_markdown`;
// the Tauri build renders markdown natively in the backend.
#[cfg_attr(not(feature = "server"), allow(dead_code))]
fn protect_math(input: &str) -> (String, Vec<String>) {
    let mut placeholders = Vec::new();
    let mut output = String::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '`' {
            output.push(chars[i]);
            i += 1;
            while i < chars.len() {
                let c = chars[i];
                output.push(c);
                i += 1;
                if c == '`' { break; }
            }
        } else if chars[i] == '$' && i + 1 < chars.len() && chars[i + 1] == '$' {
            // Display math $$...$$
            i += 2;
            let mut math = String::new();
            while i < chars.len() {
                if chars[i] == '$' && i + 1 < chars.len() && chars[i + 1] == '$' {
                    i += 2;
                    break;
                }
                math.push(chars[i]);
                i += 1;
            }
            let idx = placeholders.len();
            placeholders.push(format!("$${}$$", math));
            output.push_str(&format!("MATHPLACEHOLDER{}END", idx));
        } else if chars[i] == '$' {
            // Inline math $...$ — scan forward for closing $ on same line
            let start = i + 1;
            if start < chars.len() && chars[start] != ' ' && chars[start] != '\n' {
                let mut j = start;
                let mut found = false;
                while j < chars.len() && chars[j] != '\n' {
                    if chars[j] == '$' {
                        if j > start && chars[j - 1] != ' ' {
                            found = true;
                        }
                        break;
                    }
                    j += 1;
                }
                if found {
                    let math: String = chars[start..j].iter().collect();
                    let idx = placeholders.len();
                    placeholders.push(format!("${}$", math));
                    output.push_str(&format!("MATHPLACEHOLDER{}END", idx));
                    i = j + 1;
                } else {
                    output.push('$');
                    i += 1;
                }
            } else {
                output.push('$');
                i += 1;
            }
        } else {
            output.push(chars[i]);
            i += 1;
        }
    }

    (output, placeholders)
}

#[cfg_attr(not(feature = "server"), allow(dead_code))]
fn restore_math(html: &str, placeholders: &[String]) -> String {
    let mut output = html.to_string();
    for (idx, math) in placeholders.iter().enumerate() {
        let placeholder = format!("MATHPLACEHOLDER{}END", idx);
        output = output.replace(&placeholder, math);
    }
    output
}

#[cfg_attr(not(feature = "server"), allow(dead_code))]
pub fn parse_markdown(input: &str) -> String {
    let (protected, placeholders) = protect_math(input);

    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_GFM);

    let parser = Parser::new_ext(&protected, options);
    let mut output = String::new();
    html::push_html(&mut output, parser);

    restore_math(&output, &placeholders)
}


#[component]
pub fn MarkdownBlock(value: String) -> Element {
    let mut html = use_signal(String::new);

    use_effect(use_reactive!(|value| {
        spawn(async move {
            let rendered = crate::api::render_markdown(value).await;
            html.set(rendered);
        });
    }));

    use_effect(move || {
        let _ = html.read(); // subscribe so this re-runs after html is committed to the DOM
        let _ = dioxus::document::eval("window.renderMath && window.renderMath(); window.hljs && window.hljs.highlightAll();");
    });

    rsx!(
        div { class: "block-text", dangerous_inner_html: "{html}" }
    )
}


pub fn image_url_from_virtual_path(virtual_path: &str) -> String {
    let encoded = virtual_path
        .split('/')
        .map(|seg| encode(seg))
        .collect::<Vec<_>>()
        .join("/");

    #[cfg(feature = "server")]
    return format!("/{}", encoded); // virtual_path already starts with "files/"

    #[cfg(not(feature = "server"))]
    {
        // Tauri exposes custom URI schemes differently per platform:
        //   macOS/Linux: appimg://<path>
        //   Windows: http://appimg.localhost/<path>
        // The webview compiles to wasm, so target_os is always "wasm" — detect at
        // runtime from the webview user agent instead.
        if is_http_localhost_platform() {
            format!("http://appimg.localhost/{}", encoded)
        } else {
            format!("appimg://{}", encoded)
        }
    }
}

#[cfg(not(feature = "server"))]
fn is_http_localhost_platform() -> bool {
    web_sys::window()
        .and_then(|w| w.navigator().user_agent().ok())
        .map(|ua| ua.contains("Windows"))
        .unwrap_or(false)
}

pub fn render_block(block: &Block) -> Element {
    match block {
        Block::Text { value } => rsx! {
            MarkdownBlock { value: value.clone() }
        },

        Block::Image { src, scale } => {
            let url = image_url_from_virtual_path(&src);
            let width = scale.unwrap_or(100);
            rsx!(
                img { class: "block-image", src: "{url}", style: "width: {width}%" }
            )
        },

        Block::Audio { src } => {
            let url = image_url_from_virtual_path(&src);
            rsx!(
                audio { class: "block-audio", controls: true, src: "{url}" }
            )
        },

        Block::Video { src } => {
            let url = image_url_from_virtual_path(&src);
            rsx!(
                // Only fetch the metadata/header up front; the body streams
                // on demand once the user presses play.
                video { class: "block-video", controls: true, preload: "metadata", src: "{url}" }
            )
        },

        Block::File { path } => {
            let path0: String = path.clone();

            rsx!(
                button {
                    class: "button button-primary",
                    onclick: move |_| {
                        let path = path0.clone();
                        spawn(async move {
                            let _ = download_file(path).await;
                        });
                    },
                    "Download Exercise File"
                }
            )
        }
    }
}
