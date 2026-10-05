use tauri::Manager;
use tauri_plugin_dialog::{DialogExt, FileDialogBuilder, FilePath};
use tauri_plugin_fs_adapter::FsAdapterExt;
use futures::channel::oneshot;
use domain_flashcard::{Block, Card, Deck};
use domain_tagging::{CardTagInfo, Tag};
use crate::Services;

fn to_str(e: impl std::fmt::Display) -> String {
    e.to_string()
}

// ---------------------------------------------------------------------------
// Deck commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn add_deck(
    state: tauri::State<'_, Services>,
    name: String,
) -> Result<i64, String> {
    let svc = state;
    let deck = svc.flashcard.create_deck(&name).map_err(to_str)?;
    Ok(deck.id)
}

#[tauri::command]
pub fn get_decks(
    state: tauri::State<'_, Services>,
) -> Result<Vec<Deck>, String> {
    let svc = state;
    svc.flashcard.get_decks().map_err(to_str)
}

#[tauri::command]
pub fn rename_deck(
    state: tauri::State<'_, Services>,
    deck_id: i64,
    name: String,
) -> Result<(), String> {
    let svc = state;
    svc.flashcard.rename_deck(deck_id, &name).map_err(to_str)?;
    Ok(())
}

#[tauri::command]
pub fn delete_deck(
    state: tauri::State<'_, Services>,
    deck_id: i64,
) -> Result<(), String> {
    let svc = state;
    svc.flashcard.delete_deck(deck_id).map_err(to_str)
}

// ---------------------------------------------------------------------------
// Card commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn add_card(
    state: tauri::State<'_, Services>,
    deck_id: i64,
    name: String,
) -> Result<i64, String> {
    let svc = state;
    let card = svc.flashcard.add_card(deck_id, &name).map_err(to_str)?;
    Ok(card.id)
}

#[tauri::command]
pub fn get_card(
    state: tauri::State<'_, Services>,
    id: i64,
) -> Result<Card, String> {
    let svc = state;
    svc.flashcard.get_card(id).map_err(to_str)
}

#[tauri::command]
pub fn get_cards(
    state: tauri::State<'_, Services>,
    deck_id: i64,
) -> Result<Vec<Card>, String> {
    let svc = state;
    svc.flashcard.get_cards(deck_id).map_err(to_str)
}

#[tauri::command]
pub fn search_cards(
    state: tauri::State<'_, Services>,
    query: String,
) -> Result<Vec<Card>, String> {
    let svc = state;
    svc.flashcard.search_cards(&query).map_err(to_str)
}

#[tauri::command]
pub fn save_card_blocks(
    state: tauri::State<'_, Services>,
    card_id: i64,
    front: Vec<Block>,
    back: Vec<Block>,
) -> Result<(), String> {
    let svc = state;
    svc.flashcard.save_blocks(card_id, front, back).map_err(to_str)
}

#[tauri::command]
pub fn update_card_metadata(
    state: tauri::State<'_, Services>,
    id: i64,
    name: String,
) -> Result<(), String> {
    let svc = state;
    svc.flashcard.rename_card(id, &name).map_err(to_str)?;
    Ok(())
}

#[tauri::command]
pub fn delete_card(
    state: tauri::State<'_, Services>,
    id: i64,
) -> Result<(), String> {
    let svc = state;
    svc.flashcard.delete_card(id).map_err(to_str)
}

#[tauri::command]
pub fn update_score(
    state: tauri::State<'_, Services>,
    card_id: i64,
    correct: bool,
) -> Result<Card, String> {
    let svc = state;
    svc.study.rate_card(card_id, correct).map_err(to_str)
}

// ---------------------------------------------------------------------------
// Cover image commands
// ---------------------------------------------------------------------------

/// Set or clear the cover image for a deck.
/// `virtual_path` is e.g. "files/uuid.png"; pass null/None to remove.
#[tauri::command]
pub fn set_deck_cover(
    state: tauri::State<'_, Services>,
    deck_id: i64,
    virtual_path: Option<String>,
) -> Result<(), String> {
    let svc = state;
    svc.flashcard.set_deck_cover(deck_id, virtual_path).map_err(to_str)
}

/// Set or clear the cover image for a card.
/// `virtual_path` is e.g. "files/uuid.png"; pass null/None to remove.
#[tauri::command]
pub fn set_card_cover(
    state: tauri::State<'_, Services>,
    card_id: i64,
    virtual_path: Option<String>,
) -> Result<(), String> {
    let svc = state;
    svc.flashcard.set_card_cover(card_id, virtual_path).map_err(to_str)
}

// ---------------------------------------------------------------------------
// Notes commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn save_card_notes(
    state: tauri::State<'_, Services>,
    card_id: i64,
    easy: Option<String>,
    difficult: Option<String>,
) -> Result<(), String> {
    let svc = state;
    svc.flashcard.save_card_notes(card_id, easy, difficult).map_err(to_str)
}

// ---------------------------------------------------------------------------
// Tag commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_all_tags_full(
    state: tauri::State<'_, Services>,
) -> Result<Vec<Tag>, String> {
    let svc = state;
    svc.tagging.get_tags_in_use().map_err(to_str)
}

#[tauri::command]
pub fn get_all_tags(
    state: tauri::State<'_, Services>,
) -> Result<Vec<Tag>, String> {
    let svc = state;
    svc.tagging.get_all_tags().map_err(to_str)
}

#[tauri::command]
pub fn create_tag(
    state: tauri::State<'_, Services>,
    name: String,
) -> Result<Tag, String> {
    let svc = state;
    svc.tagging.create_tag(&name).map_err(to_str)
}

#[tauri::command]
pub fn rename_tag(
    state: tauri::State<'_, Services>,
    tag_id: i64,
    name: String,
) -> Result<Tag, String> {
    let svc = state;
    svc.tagging.rename_tag(tag_id, &name).map_err(to_str)
}

#[tauri::command]
pub fn delete_tag(
    state: tauri::State<'_, Services>,
    tag_id: i64,
) -> Result<(), String> {
    let svc = state;
    svc.tagging.delete_tag(tag_id).map_err(to_str)
}

#[tauri::command]
pub fn get_cards_by_tags(
    state: tauri::State<'_, Services>,
    tag_ids: Vec<i64>,
) -> Result<Vec<Card>, String> {
    let svc = state;
    svc.tagging.cards_by_tags(&tag_ids).map_err(to_str)
}

#[tauri::command]
pub fn get_card_tags(
    state: tauri::State<'_, Services>,
    card_id: i64,
) -> Result<Vec<Tag>, String> {
    let svc = state;
    svc.tagging.tags_for_card(card_id).map_err(to_str)
}

#[tauri::command]
pub fn get_card_tags_with_position(
    state: tauri::State<'_, Services>,
    card_id: i64,
) -> Result<Vec<CardTagInfo>, String> {
    let svc = state;
    svc.tagging.tags_for_card_with_position(card_id).map_err(to_str)
}

#[tauri::command]
pub fn get_card_tag_positions(
    state: tauri::State<'_, Services>,
    tag_ids: Vec<i64>,
) -> Result<Vec<(i64, i64, i64)>, String> {
    let svc = state;
    svc.tagging.card_tag_positions(&tag_ids).map_err(to_str)
}

#[tauri::command]
pub fn set_card_tag_position(
    state: tauri::State<'_, Services>,
    tag_id: i64,
    card_id: i64,
    position: i64,
) -> Result<(), String> {
    let svc = state;
    svc.tagging.set_card_tag_position(tag_id, card_id, position).map_err(to_str)
}

#[tauri::command]
pub fn sync_card_tags(
    state: tauri::State<'_, Services>,
    card_id: i64,
    tag_names: Vec<String>,
) -> Result<Vec<Tag>, String> {
    let svc = state;
    svc.tagging.sync_tags_for_card(card_id, tag_names).map_err(to_str)
}

#[tauri::command]
pub fn add_tag_edge(
    state: tauri::State<'_, Services>,
    parent_id: i64,
    child_id: i64,
) -> Result<(), String> {
    let svc = state;
    svc.tagging.add_tag_edge(parent_id, child_id).map_err(to_str)
}

#[tauri::command]
pub fn remove_tag_edge(
    state: tauri::State<'_, Services>,
    parent_id: i64,
    child_id: i64,
) -> Result<(), String> {
    let svc = state;
    svc.tagging.remove_tag_edge(parent_id, child_id).map_err(to_str)
}

#[tauri::command]
pub fn get_tag_edges(
    state: tauri::State<'_, Services>,
) -> Result<Vec<(i64, i64)>, String> {
    let svc = state;
    svc.tagging.get_tag_edges().map_err(to_str)
}

#[tauri::command]
pub fn assign_tag_to_deck(
    state: tauri::State<'_, Services>,
    tag_id: i64,
    deck_id: i64,
) -> Result<(), String> {
    let svc = state;
    svc.tagging.assign_tag_to_deck(tag_id, deck_id).map_err(to_str)
}

#[tauri::command]
pub fn unassign_tag_from_deck(
    state: tauri::State<'_, Services>,
    tag_id: i64,
    deck_id: i64,
) -> Result<(), String> {
    let svc = state;
    svc.tagging.unassign_tag_from_deck(tag_id, deck_id).map_err(to_str)
}

#[tauri::command]
pub fn get_tag_deck_pairs(
    state: tauri::State<'_, Services>,
) -> Result<Vec<(i64, i64)>, String> {
    let svc = state;
    svc.tagging.get_tag_deck_pairs().map_err(to_str)
}

#[tauri::command]
pub fn detach_tag_from_cards(
    state: tauri::State<'_, Services>,
    tag_id: i64,
) -> Result<(), String> {
    let svc = state;
    svc.tagging.detach_tag_from_cards(tag_id).map_err(to_str)
}

#[tauri::command]
pub fn set_tag_position(
    state: tauri::State<'_, Services>,
    tag_id: i64,
    x: f64,
    y: f64,
) -> Result<(), String> {
    let svc = state;
    svc.tagging.set_tag_position(tag_id, x, y).map_err(to_str)
}

#[tauri::command]
pub fn clear_tag_position(
    state: tauri::State<'_, Services>,
    tag_id: i64,
) -> Result<(), String> {
    let svc = state;
    svc.tagging.clear_tag_position(tag_id).map_err(to_str)
}

#[tauri::command]
pub fn reset_tag_map(
    state: tauri::State<'_, Services>,
) -> Result<(), String> {
    let svc = state;
    svc.tagging.reset_tag_map().map_err(to_str)
}

// ---------------------------------------------------------------------------
// Deck detail commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_deck(
    state: tauri::State<'_, Services>,
    deck_id: i64,
) -> Result<Deck, String> {
    let svc = state;
    svc.flashcard.get_deck(deck_id).map_err(to_str)
}

#[tauri::command]
pub fn reset_deck_progress(
    state: tauri::State<'_, Services>,
    deck_id: i64,
) -> Result<(), String> {
    let svc = state;
    svc.flashcard.reset_deck_progress(deck_id).map_err(to_str)
}

#[tauri::command]
pub fn move_cards(
    state: tauri::State<'_, Services>,
    card_ids: Vec<i64>,
    target_deck_id: i64,
) -> Result<(), String> {
    let svc = state;
    svc.flashcard.move_cards(card_ids, target_deck_id).map_err(to_str)
}

#[tauri::command]
pub fn get_deck_tags(
    state: tauri::State<'_, Services>,
    deck_id: i64,
) -> Result<Vec<Tag>, String> {
    let svc = state;
    svc.tagging.tags_for_deck(deck_id).map_err(to_str)
}

#[tauri::command]
pub fn get_card_tag_pairs(
    state: tauri::State<'_, Services>,
    deck_id: i64,
) -> Result<Vec<(i64, i64)>, String> {
    let svc = state;
    svc.tagging.card_tag_pairs_for_deck(deck_id).map_err(to_str)
}

// ---------------------------------------------------------------------------
// File commands (no services — pure filesystem)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn download_file(
    app: tauri::AppHandle,
    virtual_path: String,
) -> Result<(), String> {
    let app_data_dir = app.path().app_data_dir().map_err(to_str)?;
    let source = app_data_dir.join(&virtual_path);

    let Some(file_name) = source.file_name() else {
        return Err("Invalid path".into());
    };

    let (tx, rx) = oneshot::channel();
    FileDialogBuilder::new(app.dialog().clone())
        .set_file_name(file_name.to_string_lossy())
        .save_file(move |file| {
            let _ = tx.send(file);
        });

    let dest = rx.await.map_err(to_str)?;

    let Some(FilePath::Path(dest_path)) = dest else {
        return Ok(()); // user cancelled
    };

    std::fs::copy(&source, &dest_path).map_err(to_str)?;
    Ok(())
}

#[tauri::command]
pub async fn delete_block_from_app_data(
    app: tauri::AppHandle,
    virtual_path: String,
) -> Result<(), String> {
    let app_data_dir = app.path().app_data_dir().map_err(to_str)?;
    let full_path = app_data_dir.join(&virtual_path);

    if !full_path.exists() {
        return Ok(());
    }

    std::fs::remove_file(&full_path)
        .map_err(|e| format!("Failed to delete {:?}: {}", full_path, e))
}

// ---------------------------------------------------------------------------
// Deck export / import — transport only; the archive format lives in
// application::deck_transfer.
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn export_deck(
    app: tauri::AppHandle,
    deck_id: i64,
) -> Result<(), String> {
    // Dialog first: the user picks a destination while nothing has been
    // built yet, so cancelling costs nothing.
    let (tx, rx) = oneshot::channel();

    FileDialogBuilder::new(app.dialog().clone())
        .set_file_name("deck-export.zip")
        .save_file(move |file| {
            let _ = tx.send(file);
        });

    let dest = match rx.await {
        Ok(Some(FilePath::Path(path))) => path,
        _ => return Ok(()), // user cancelled or non-path
    };

    // Stream the archive straight to the chosen file off the async runtime.
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let svc = app.state::<Services>();
        let data_dir = app.path().app_data_dir().map_err(to_str)?;
        let file = std::fs::File::create(&dest)
            .map_err(|e| format!("Failed to create {:?}: {}", dest, e))?;
        let result = application::deck_transfer::export_deck(
            &svc.flashcard,
            &svc.tagging,
            deck_id,
            &data_dir,
            std::io::BufWriter::new(file),
        );
        // Clean up the partial file if anything failed midway.
        if result.is_err() {
            let _ = std::fs::remove_file(&dest);
        }
        result.map_err(to_str)
    })
    .await
    .map_err(to_str)??;

    Ok(())
}

#[tauri::command]
pub async fn import_deck(
    app: tauri::AppHandle,
) -> Result<i64, String> {
    let Some(bytes) = app
        .fs_adapter()
        .pick_import_file()
        .await
        .map_err(to_str)?
    else {
        return Ok(0);
    };

    tauri::async_runtime::spawn_blocking(move || -> Result<i64, String> {
        let svc = app.state::<Services>();
        let data_dir = app.path().app_data_dir().map_err(to_str)?;
        application::deck_transfer::import_deck(
            &svc.flashcard,
            &svc.tagging,
            svc.tx.as_ref(),
            &data_dir,
            &bytes,
        )
        .map_err(to_str)
    })
    .await
    .map_err(to_str)?
}

// ---------------------------------------------------------------------------
// Markdown rendering with syntax highlighting
// ---------------------------------------------------------------------------

use std::sync::OnceLock;
use syntect::parsing::SyntaxSet;
use syntect::highlighting::ThemeSet;
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag as CmTag, TagEnd, html};

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();

fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme_set() -> &'static ThemeSet {
    THEME_SET.get_or_init(ThemeSet::load_defaults)
}

fn md_protect_math(input: &str) -> (String, Vec<String>) {
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
            i += 2;
            let mut math = String::new();
            while i < chars.len() {
                if chars[i] == '$' && i + 1 < chars.len() && chars[i + 1] == '$' { i += 2; break; }
                math.push(chars[i]);
                i += 1;
            }
            let idx = placeholders.len();
            placeholders.push(format!("$${}$$", math));
            output.push_str(&format!("MATHPLACEHOLDER{}END", idx));
        } else if chars[i] == '$' {
            let start = i + 1;
            if start < chars.len() && chars[start] != ' ' && chars[start] != '\n' {
                let mut j = start;
                let mut found = false;
                while j < chars.len() && chars[j] != '\n' {
                    if chars[j] == '$' {
                        if j > start && chars[j - 1] != ' ' { found = true; }
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
                } else { output.push('$'); i += 1; }
            } else { output.push('$'); i += 1; }
        } else {
            output.push(chars[i]);
            i += 1;
        }
    }
    (output, placeholders)
}

fn md_restore_math(html: &str, placeholders: &[String]) -> String {
    let mut out = html.to_string();
    for (idx, math) in placeholders.iter().enumerate() {
        out = out.replace(&format!("MATHPLACEHOLDER{}END", idx), math);
    }
    out
}

fn highlight_code(code: &str, lang: &str) -> String {
    let ps = syntax_set();
    let ts = theme_set();
    let theme = &ts.themes["base16-ocean.dark"];
    let syntax = ps.find_syntax_by_token(lang).unwrap_or_else(|| ps.find_syntax_plain_text());
    syntect::html::highlighted_html_for_string(code, ps, syntax, theme)
        .unwrap_or_else(|_| format!(
            "<pre><code>{}</code></pre>",
            code.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
        ))
}

fn render_markdown_internal(input: &str) -> String {
    let (protected, placeholders) = md_protect_math(input);

    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_GFM);

    let parser = Parser::new_ext(&protected, opts);
    let mut events: Vec<Event> = Vec::new();
    let mut in_code = false;
    let mut code_lang = String::new();
    let mut code_buf = String::new();

    for event in parser {
        match event {
            Event::Start(CmTag::CodeBlock(kind)) => {
                in_code = true;
                code_lang = match &kind {
                    CodeBlockKind::Fenced(lang) => {
                        lang.split_whitespace().next().unwrap_or("").to_string()
                    }
                    CodeBlockKind::Indented => String::new(),
                };
                code_buf.clear();
            }
            Event::Text(text) if in_code => {
                code_buf.push_str(&text);
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code = false;
                let highlighted = highlight_code(&code_buf, &code_lang);
                events.push(Event::Html(highlighted.into()));
            }
            other if !in_code => events.push(other),
            _ => {}
        }
    }

    let mut html_out = String::new();
    html::push_html(&mut html_out, events.into_iter());
    md_restore_math(&html_out, &placeholders)
}

#[tauri::command]
pub fn render_markdown(text: String) -> String {
    render_markdown_internal(&text)
}
