/// Web fetch bridge — mirrors tauri_api.rs exactly.
/// All functions call the Axum REST server instead of Tauri invoke.
/// Components import from crate::api and never reference this file directly.
use gloo_net::http::Request;
use serde::Serialize;
use domain_flashcard::{Block, Card, Deck};
use domain_tagging::{CardTagInfo, Tag};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn log_err(msg: &str) {
    web_sys::console::error_1(&wasm_bindgen::JsValue::from_str(msg));
}

/// GET that returns a collection — falls back to empty on error.
async fn get_vec<T: for<'de> serde::Deserialize<'de>>(path: &str) -> Vec<T> {
    match Request::get(path).send().await {
        Ok(resp) => resp.json::<Vec<T>>().await.unwrap_or_else(|e| {
            log_err(&format!("GET {} parse error: {:?}", path, e));
            Vec::new()
        }),
        Err(e) => {
            log_err(&format!("GET {} failed: {:?}", path, e));
            Vec::new()
        }
    }
}

/// GET that returns a single item — panics with a clear message on error.
async fn get_one<T: for<'de> serde::Deserialize<'de>>(path: &str) -> T {
    Request::get(path).send().await
        .unwrap_or_else(|e| panic!("GET {} failed: {:?}", path, e))
        .json::<T>().await
        .unwrap_or_else(|e| panic!("GET {} parse error: {:?}", path, e))
}

/// POST that returns a single item.
async fn post<B: Serialize, T: for<'de> serde::Deserialize<'de> + Default>(path: &str, body: &B) -> T {
    let resp = match Request::post(path).json(body).unwrap().send().await {
        Ok(r)  => r,
        Err(e) => { log_err(&format!("POST {} failed: {:?}", path, e)); return T::default(); }
    };
    if !resp.ok() {
        log_err(&format!("POST {} returned {}", path, resp.status()));
        return T::default();
    }
    resp.json::<T>().await.unwrap_or_else(|e| {
        log_err(&format!("POST {} parse error: {:?}", path, e));
        T::default()
    })
}

async fn put<B: Serialize>(path: &str, body: &B) {
    if let Err(e) = Request::put(path).json(body).unwrap().send().await {
        log_err(&format!("PUT {} failed: {:?}", path, e));
    }
}

async fn delete_req(path: &str) {
    if let Err(e) = Request::delete(path).send().await {
        log_err(&format!("DELETE {} failed: {:?}", path, e));
    }
}

// ---------------------------------------------------------------------------
// Decks
// ---------------------------------------------------------------------------

pub async fn get_deck(deck_id: i64) -> Deck {
    get_one(&format!("/api/decks/{}", deck_id)).await
}

pub async fn get_decks() -> Vec<Deck> {
    get_vec("/api/decks").await
}

#[derive(Serialize)]
struct AddDeckBody { name: String }

pub async fn add_deck(name: String) -> i64 {
    let deck: Deck = post("/api/decks", &AddDeckBody { name }).await;
    deck.id
}

#[derive(Serialize)]
struct RenameDeckBody { name: String }

pub async fn rename_deck(name: String, deck_id: i64) {
    put(&format!("/api/decks/{}/rename", deck_id), &RenameDeckBody { name }).await;
}

pub async fn delete_deck(deck_id: i64) {
    delete_req(&format!("/api/decks/{}", deck_id)).await;
}

// ---------------------------------------------------------------------------
// Cards
// ---------------------------------------------------------------------------

pub async fn get_cards(deck_id: i64) -> Vec<Card> {
    get_vec(&format!("/api/decks/{}/cards", deck_id)).await
}

pub async fn search_cards(query: String) -> Vec<Card> {
    if query.trim().is_empty() { return Vec::new(); }
    let encoded = js_sys::encode_uri_component(&query);
    get_vec(&format!("/api/cards/search?q={}", encoded)).await
}

#[derive(Serialize)]
struct AddCardBody { name: String }

pub async fn add_card(deck_id: i64, name: String) -> i64 {
    post(&format!("/api/decks/{}/cards", deck_id), &AddCardBody { name }).await
}

pub async fn get_card(id: i64) -> Card {
    get_one(&format!("/api/cards/{}", id)).await
}

#[derive(Serialize)]
struct RenameCardBody { name: String }

pub async fn update_card_metadata(id: i64, name: String) {
    put(&format!("/api/cards/{}/name", id), &RenameCardBody { name }).await;
}

#[derive(Serialize)]
struct SaveBlocksBody<'a> { front: &'a Vec<Block>, back: &'a Vec<Block> }

pub async fn save_card_blocks(card_id: i64, front: &Vec<Block>, back: &Vec<Block>) {
    put(&format!("/api/cards/{}/blocks", card_id), &SaveBlocksBody { front, back }).await;
}

pub async fn delete_card(id: i64) {
    delete_req(&format!("/api/cards/{}", id)).await;
}

// ---------------------------------------------------------------------------
// Notes
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct SaveNotesBody { easy: Option<String>, difficult: Option<String> }

pub async fn save_card_notes(card_id: i64, easy: Option<String>, difficult: Option<String>) {
    put(&format!("/api/cards/{}/notes", card_id), &SaveNotesBody { easy, difficult }).await;
}

#[derive(Serialize)]
struct MoveCardsBody { card_ids: Vec<i64>, target_deck_id: i64 }

pub async fn move_cards(card_ids: Vec<i64>, target_deck_id: i64) {
    let _: () = post("/api/cards/move", &MoveCardsBody { card_ids, target_deck_id }).await;
}

pub async fn reset_deck_progress(deck_id: i64) {
    let _: () = post(&format!("/api/decks/{}/reset-progress", deck_id), &()).await;
}

// ---------------------------------------------------------------------------
// Tags
// ---------------------------------------------------------------------------

pub async fn get_all_tags_full() -> Vec<Tag> {
    get_vec("/api/tags-full").await
}

pub async fn get_all_tags() -> Vec<Tag> {
    get_vec("/api/tags").await
}

#[derive(Serialize)]
struct CreateTagBody { name: String }

pub async fn create_tag(name: String) -> Option<Tag> {
    match Request::post("/api/tags").json(&CreateTagBody { name }).unwrap().send().await {
        Ok(r) if r.ok() => r.json::<Tag>().await.ok(),
        Ok(r)  => { log_err(&format!("create_tag: {}", r.status())); None }
        Err(e) => { log_err(&format!("create_tag: {:?}", e)); None }
    }
}

#[derive(Serialize)]
struct RenameTagBody { name: String }

pub async fn rename_tag(tag_id: i64, name: String) -> Option<Tag> {
    let url = format!("/api/tags/{}", tag_id);
    match Request::put(&url).json(&RenameTagBody { name }).unwrap().send().await {
        Ok(r) if r.ok() => r.json::<Tag>().await.ok(),
        Ok(r)  => { log_err(&format!("rename_tag: {}", r.status())); None }
        Err(e) => { log_err(&format!("rename_tag: {:?}", e)); None }
    }
}

pub async fn delete_tag(tag_id: i64) {
    if let Err(e) = Request::delete(&format!("/api/tags/{}", tag_id)).send().await {
        log_err(&format!("delete_tag: {:?}", e));
    }
}

pub async fn get_cards_by_tags(tag_ids: Vec<i64>) -> Vec<Card> {
    if tag_ids.is_empty() { return Vec::new(); }
    let ids_str = tag_ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");
    get_vec(&format!("/api/cards/by-tags?tag_ids={}", ids_str)).await
}

pub async fn get_card_tags(card_id: i64) -> Vec<Tag> {
    get_vec(&format!("/api/cards/{}/tags", card_id)).await
}

/// A card's tags, each with its position in that tag's progression.
pub async fn get_card_tags_with_position(card_id: i64) -> Vec<CardTagInfo> {
    get_vec(&format!("/api/cards/{}/tags-with-position", card_id)).await
}

/// (tag_id, card_id, position) for the given tags — the progression numbers
/// shown beside cards in the knowledge map's card list.
pub async fn get_card_tag_positions(tag_ids: Vec<i64>) -> Vec<(i64, i64, i64)> {
    if tag_ids.is_empty() { return Vec::new(); }
    let ids_str = tag_ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");
    get_vec(&format!("/api/tags/card-positions?tag_ids={}", ids_str)).await
}

#[derive(Serialize)]
struct SetCardTagPositionBody { position: i64 }

/// Set a card's position within one tag's progression.
pub async fn set_card_tag_position(tag_id: i64, card_id: i64, position: i64) {
    put(&format!("/api/cards/{}/tags/{}/position", card_id, tag_id), &SetCardTagPositionBody { position }).await;
}

#[derive(Serialize)]
struct SyncTagsBody { tag_names: Vec<String> }

pub async fn sync_card_tags(card_id: i64, tag_names: Vec<String>) -> Vec<Tag> {
    let resp = match Request::put(&format!("/api/cards/{}/tags", card_id))
        .json(&SyncTagsBody { tag_names }).unwrap().send().await {
        Ok(r) if r.ok() => r,
        Ok(r) => { log_err(&format!("sync_card_tags: {}", r.status())); return Vec::new(); }
        Err(e) => { log_err(&format!("sync_card_tags: {:?}", e)); return Vec::new(); }
    };
    resp.json::<Vec<Tag>>().await.unwrap_or_default()
}

#[derive(Serialize)]
struct EdgeBody { parent_id: i64, child_id: i64 }

pub async fn get_tag_edges() -> Vec<(i64, i64)> {
    get_vec("/api/tags/edges").await
}

pub async fn add_tag_edge(parent_id: i64, child_id: i64) {
    match Request::post("/api/tags/edges")
        .json(&EdgeBody { parent_id, child_id }).unwrap().send().await {
        Ok(r) if r.ok() => {}
        Ok(r) => log_err(&format!("add_tag_edge: {}", r.status())),
        Err(e) => log_err(&format!("add_tag_edge: {:?}", e)),
    }
}

pub async fn remove_tag_edge(parent_id: i64, child_id: i64) {
    match Request::delete("/api/tags/edges")
        .json(&EdgeBody { parent_id, child_id }).unwrap().send().await {
        Ok(r) if r.ok() => {}
        Ok(r) => log_err(&format!("remove_tag_edge: {}", r.status())),
        Err(e) => log_err(&format!("remove_tag_edge: {:?}", e)),
    }
}

#[derive(Serialize)]
struct TagDeckBody { tag_id: i64, deck_id: i64 }

pub async fn get_tag_deck_pairs() -> Vec<(i64, i64)> {
    get_vec("/api/tags/deck-pairs").await
}

pub async fn assign_tag_to_deck(tag_id: i64, deck_id: i64) {
    match Request::post("/api/tags/deck")
        .json(&TagDeckBody { tag_id, deck_id }).unwrap().send().await {
        Ok(r) if r.ok() => {}
        Ok(r) => log_err(&format!("assign_tag_to_deck: {}", r.status())),
        Err(e) => log_err(&format!("assign_tag_to_deck: {:?}", e)),
    }
}

pub async fn unassign_tag_from_deck(tag_id: i64, deck_id: i64) {
    match Request::delete("/api/tags/deck")
        .json(&TagDeckBody { tag_id, deck_id }).unwrap().send().await {
        Ok(r) if r.ok() => {}
        Ok(r) => log_err(&format!("unassign_tag_from_deck: {}", r.status())),
        Err(e) => log_err(&format!("unassign_tag_from_deck: {:?}", e)),
    }
}

pub async fn detach_tag_from_cards(tag_id: i64) {
    if let Err(e) = Request::delete(&format!("/api/tags/{}/cards", tag_id)).send().await {
        log_err(&format!("detach_tag_from_cards: {:?}", e));
    }
}

#[derive(Serialize)]
struct SetPositionBody { x: f64, y: f64 }

pub async fn set_tag_position(tag_id: i64, x: f64, y: f64) {
    if let Err(e) = Request::put(&format!("/api/tags/{}/position", tag_id))
        .json(&SetPositionBody { x, y }).unwrap().send().await {
        log_err(&format!("set_tag_position: {:?}", e));
    }
}

pub async fn clear_tag_position(tag_id: i64) {
    if let Err(e) = Request::delete(&format!("/api/tags/{}/position", tag_id)).send().await {
        log_err(&format!("clear_tag_position: {:?}", e));
    }
}

pub async fn reset_tag_map() {
    let _: () = post("/api/tags/reset-map", &()).await;
}

pub async fn get_deck_tags(deck_id: i64) -> Vec<Tag> {
    get_vec(&format!("/api/decks/{}/deck-tags", deck_id)).await
}

pub async fn get_card_tag_pairs(deck_id: i64) -> Vec<(i64, i64)> {
    get_vec(&format!("/api/decks/{}/card-tag-pairs", deck_id)).await
}

// ---------------------------------------------------------------------------
// Study / scoring
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct AnswerBody { correct: bool }

pub async fn update_score(card_id: i64, correct: bool) -> Card {
    post::<AnswerBody, Card>(&format!("/api/cards/{}/answer", card_id), &AnswerBody { correct }).await
}

// ---------------------------------------------------------------------------
// Cover images
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct SetCoverBody { virtual_path: Option<String> }

pub async fn set_deck_cover(deck_id: i64, virtual_path: Option<String>) {
    put(&format!("/api/decks/{}/cover", deck_id), &SetCoverBody { virtual_path }).await;
}

pub async fn set_card_cover(card_id: i64, virtual_path: Option<String>) {
    put(&format!("/api/cards/{}/cover", card_id), &SetCoverBody { virtual_path }).await;
}

// ---------------------------------------------------------------------------
// File operations
// ---------------------------------------------------------------------------

/// Upload a file via the browser <input type="file"> picker.
/// Returns the virtual path (e.g. "files/uuid.png"), or empty string on cancel.
/// Note: the actual file-picker UI is handled in the component for web —
///       this function just performs the upload given a web_sys::File.
pub async fn upload_file(file: web_sys::File) -> String {
    upload_file_to(file, "/api/files/upload").await
}

async fn upload_file_to(file: web_sys::File, endpoint: &str) -> String {
    let form = web_sys::FormData::new().unwrap();
    form.append_with_blob("file", &file).unwrap();

    let resp = Request::post(endpoint)
        .body(form).unwrap()
        .send().await.unwrap();

    #[derive(serde::Deserialize)]
    struct UploadResp { path: String }
    resp.json::<UploadResp>().await.map(|r| r.path).unwrap_or_default()
}

/// Pick an image from the browser and upload it; returns virtual path or "".
pub async fn pick_image() -> String {
    pick_file("image/*", "/api/files/upload").await
}

/// Pick an image and upload it as a cover (server resizes to 640×320 JPEG).
pub async fn pick_cover_image() -> String {
    pick_file("image/*", "/api/files/upload-cover").await
}

/// Pick an audio file from the browser and upload it; returns virtual path or "".
pub async fn pick_audio() -> String {
    pick_file("audio/*,.mp3,.m4a,.wav,.ogg,.aac", "/api/files/upload").await
}

/// Pick a video file from the browser and upload it; returns virtual path or "".
pub async fn pick_video() -> String {
    pick_file("video/*,.mp4,.mov,.m4v,.webm", "/api/files/upload").await
}

/// Pick a zip archive from the browser and upload it; returns virtual path or "".
pub async fn pick_archive() -> String {
    pick_file(".zip,application/zip", "/api/files/upload").await
}

async fn pick_file(accept: &str, endpoint: &str) -> String {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{window, HtmlInputElement, Event};
    use js_sys::Promise;

    let document = window().unwrap().document().unwrap();
    let input: HtmlInputElement = document
        .create_element("input").unwrap()
        .dyn_into().unwrap();
    input.set_type("file");
    input.set_accept(accept);

    // Trigger the picker and wait for it to close via a one-shot Promise.
    // Dismissing the picker fires `cancel` and never `change`, so both have to
    // resolve it — otherwise the caller's upload overlay would wait forever.
    let promise = Promise::new(&mut |resolve, _reject| {
        for event in ["change", "cancel"] {
            let input_clone = input.clone();
            let resolve = resolve.clone();
            let cb = Closure::once(Box::new(move |_event: Event| {
                let _ = resolve.call1(&JsValue::NULL, &input_clone);
            }) as Box<dyn FnOnce(Event)>);
            input.add_event_listener_with_callback(event, cb.as_ref().unchecked_ref()).unwrap();
            cb.forget(); // leak intentionally — fires once then is done
        }
    });

    input.click();
    let _ = JsFuture::from(promise).await;

    let files = match input.files() {
        Some(f) if f.length() > 0 => f,
        _ => return String::new(),
    };

    let file = match files.get(0) {
        Some(f) => f,
        None    => return String::new(),
    };

    upload_file_to(file, endpoint).await
}

/// Download a file from the server — triggers browser save dialog.
pub async fn download_file(virtual_path: String) {
    use web_sys::window;
    let url = format!("/{}", virtual_path); // virtual_path already starts with "files/"
    if let Some(win) = window() {
        let _ = win.open_with_url_and_target(&url, "_blank");
    }
}

/// Delete a file asset from the server.
pub async fn delete_block_from_app_data(virtual_path: String) {
    delete_req(&format!("/api/files/{}", virtual_path)).await;
}

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize, Clone, Debug, PartialEq)]
pub struct UserInfo {
    pub id:       String,
    pub username: String,
}

#[derive(serde::Serialize)]
struct LoginBody { username: String, password: String }

#[derive(serde::Serialize)]
struct RegisterBody { username: String, password: String, token: String }

/// Returns the logged-in user on success, None on wrong credentials.
pub async fn login(username: String, password: String) -> Option<UserInfo> {
    let resp = Request::post("/api/auth/login")
        .json(&LoginBody { username, password }).ok()?
        .send().await.ok()?;
    if resp.status() == 200 { resp.json::<UserInfo>().await.ok() } else { None }
}

pub async fn logout() {
    Request::post("/api/auth/logout").send().await.ok();
}

/// Returns the current user if a valid session cookie exists, otherwise None.
pub async fn get_current_user() -> Option<UserInfo> {
    let resp = Request::get("/api/auth/me").send().await.ok()?;
    if resp.status() == 200 { resp.json::<UserInfo>().await.ok() } else { None }
}

/// Register a new account; returns the new user on success, None on conflict/error.
pub async fn register(username: String, password: String, token: String) -> Option<UserInfo> {
    let resp = Request::post("/api/auth/register")
        .json(&RegisterBody { username, password, token }).ok()?
        .send().await.ok()?;
    if resp.status() == 201 { resp.json::<UserInfo>().await.ok() } else { None }
}

// ---------------------------------------------------------------------------
// Import / Export
// ---------------------------------------------------------------------------

pub async fn export_deck(deck_id: i64) {
    use js_sys::{Array, Uint8Array};
    use wasm_bindgen::JsCast;
    use web_sys::{window, Blob, BlobPropertyBag, HtmlAnchorElement, Url};

    let resp = match Request::get(&format!("/api/decks/{}/export", deck_id)).send().await {
        Ok(r) if r.ok() => r,
        Ok(r) => { log_err(&format!("export_deck: server returned {}", r.status())); return; }
        Err(e) => { log_err(&format!("export_deck: {e:?}")); return; }
    };
    let bytes = match resp.binary().await {
        Ok(b)  => b,
        Err(e) => { log_err(&format!("export_deck: read body: {e:?}")); return; }
    };

    let array = Uint8Array::from(bytes.as_slice());
    let parts = Array::of1(&array);

    let opts = BlobPropertyBag::new();
    opts.set_type("application/zip");
    let blob = match Blob::new_with_u8_array_sequence_and_options(&parts, &opts) {
        Ok(b)  => b,
        Err(e) => { log_err(&format!("export_deck: blob: {e:?}")); return; }
    };
    let url = Url::create_object_url_with_blob(&blob).unwrap();

    let win = window().unwrap();
    let doc = win.document().unwrap();
    let a: HtmlAnchorElement = doc.create_element("a").unwrap().dyn_into().unwrap();
    a.set_href(&url);
    a.set_download("deck-export.zip");
    doc.body().unwrap().append_child(&a).unwrap();
    a.click();
    doc.body().unwrap().remove_child(&a).unwrap();
    Url::revoke_object_url(&url).unwrap();
}

pub async fn import_deck() -> i64 {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{window, Event, HtmlInputElement};
    use js_sys::Promise;

    let doc = window().unwrap().document().unwrap();
    let input: HtmlInputElement = doc.create_element("input").unwrap().dyn_into().unwrap();
    input.set_type("file");
    input.set_accept(".zip,application/zip");

    let promise = Promise::new(&mut |resolve, _| {
        let input_clone = input.clone();
        let cb = Closure::once(Box::new(move |_: Event| {
            resolve.call1(&JsValue::NULL, &input_clone).unwrap();
        }) as Box<dyn FnOnce(Event)>);
        input.add_event_listener_with_callback("change", cb.as_ref().unchecked_ref()).unwrap();
        cb.forget();
    });
    input.click();
    let _ = JsFuture::from(promise).await;

    let files = match input.files() {
        Some(f) if f.length() > 0 => f,
        _ => return 0,
    };
    let file = match files.get(0) {
        Some(f) => f,
        None    => return 0,
    };

    let form = web_sys::FormData::new().unwrap();
    form.append_with_blob("file", &file).unwrap();

    let resp = match Request::post("/api/decks/import").body(form).unwrap().send().await {
        Ok(r) if r.ok() => r,
        Ok(r) => { log_err(&format!("import_deck: server returned {}", r.status())); return 0; }
        Err(e) => { log_err(&format!("import_deck: {e:?}")); return 0; }
    };

    resp.json::<i64>().await.unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Markdown rendering (web build — no syntect, use local parser)
// ---------------------------------------------------------------------------

pub async fn render_markdown(text: String) -> String {
    crate::components::block_view::parse_markdown(&text)
}
