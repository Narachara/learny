use wasm_bindgen::prelude::*;
use serde::{Serialize, de::DeserializeOwned};
use wasm_bindgen::JsValue;
use domain_flashcard::{Deck, Card, Block};
use domain_tagging::{CardTagInfo, Tag};

// ---------------------------------------------------------------------------
// Tauri bridge — all invoke calls live here.
// Components never call invoke directly; they use the typed wrappers below.
// ---------------------------------------------------------------------------

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(
        js_namespace = ["window", "__TAURI__", "core"],
        js_name = invoke
    )]
    async fn invoke_raw(cmd: &str, args: JsValue) -> JsValue;

    // Same invoke but returns Result so rejections don't panic.
    #[wasm_bindgen(
        js_namespace = ["window", "__TAURI__", "core"],
        js_name = invoke,
        catch
    )]
    async fn invoke_raw_catch(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

async fn tauri<T, A>(cmd: &str, args: A) -> T
where
    T: DeserializeOwned,
    A: Serialize,
{
    let js_args = serde_wasm_bindgen::to_value(&args).unwrap();
    let raw = invoke_raw(cmd, js_args).await;
    serde_wasm_bindgen::from_value(raw).unwrap()
}

async fn tauri_try<T, A>(cmd: &str, args: A) -> Result<T, String>
where
    T: DeserializeOwned,
    A: Serialize,
{
    let js_args = serde_wasm_bindgen::to_value(&args).unwrap();
    match invoke_raw_catch(cmd, js_args).await {
        Ok(raw)  => Ok(serde_wasm_bindgen::from_value(raw).unwrap()),
        Err(err) => Err(serde_wasm_bindgen::from_value::<String>(err)
                        .unwrap_or_else(|_| "Unknown error".into())),
    }
}

// ---------------------------------------------------------------------------
// Decks
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AddDeckArgs {
    name: String,
}

pub async fn add_deck(name: String) -> i64 {
    tauri("add_deck", AddDeckArgs { name }).await
}

pub async fn get_decks() -> Vec<Deck> {
    tauri("get_decks", ()).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GetDeckArgs { deck_id: i64 }

pub async fn get_deck(deck_id: i64) -> Deck {
    tauri("get_deck", GetDeckArgs { deck_id }).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RenameDeckArgs {
    name: String,
    deck_id: i64,
}

pub async fn rename_deck(name: String, deck_id: i64) {
    let _: () = tauri("rename_deck", RenameDeckArgs { name, deck_id }).await;
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeleteDeckArgs {
    deck_id: i64,
}

pub async fn delete_deck(deck_id: i64) {
    let _: () = tauri("delete_deck", DeleteDeckArgs { deck_id }).await;
}

// ---------------------------------------------------------------------------
// Cards
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddCardArgs {
    pub deck_id: i64,
    pub name: String,
}

/// Creates a card with the given name. Returns the new card's id.
pub async fn add_card(deck_id: i64, name: String) -> i64 {
    tauri("add_card", AddCardArgs { deck_id, name }).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GetCardArgs {
    id: i64,
}

pub async fn get_card(id: i64) -> Card {
    tauri("get_card", GetCardArgs { id }).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GetCardsArgs {
    deck_id: i64,
}

pub async fn get_cards(deck_id: i64) -> Vec<Card> {
    tauri("get_cards", GetCardsArgs { deck_id }).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchCardsArgs { query: String }

pub async fn search_cards(query: String) -> Vec<Card> {
    tauri("search_cards", SearchCardsArgs { query }).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateCardMetadataArgs {
    id: i64,
    name: String,
}

/// Renames an existing card. Only needed when editing — add_card already sets the name.
pub async fn update_card_metadata(id: i64, name: String) {
    let _: () = tauri("update_card_metadata", UpdateCardMetadataArgs { id, name }).await;
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SaveCardBlocksArgs<'a> {
    card_id: i64,
    front: &'a Vec<Block>,
    back: &'a Vec<Block>,
}

pub async fn save_card_blocks(card_id: i64, front: &Vec<Block>, back: &Vec<Block>) {
    let _: () = tauri("save_card_blocks", SaveCardBlocksArgs { card_id, front, back }).await;
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeleteCardArgs {
    id: i64,
}

pub async fn delete_card(id: i64) {
    let _: () = tauri("delete_card", DeleteCardArgs { id }).await;
}

// ---------------------------------------------------------------------------
// Notes
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SaveCardNotesArgs {
    card_id: i64,
    easy: Option<String>,
    difficult: Option<String>,
}

pub async fn save_card_notes(card_id: i64, easy: Option<String>, difficult: Option<String>) {
    let _: () = tauri("save_card_notes", SaveCardNotesArgs { card_id, easy, difficult }).await;
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MoveCardsArgs { card_ids: Vec<i64>, target_deck_id: i64 }

pub async fn move_cards(card_ids: Vec<i64>, target_deck_id: i64) {
    let _: () = tauri("move_cards", MoveCardsArgs { card_ids, target_deck_id }).await;
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResetProgressArgs { deck_id: i64 }

pub async fn reset_deck_progress(deck_id: i64) {
    let _: () = tauri("reset_deck_progress", ResetProgressArgs { deck_id }).await;
}

// ---------------------------------------------------------------------------
// Tags
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GetCardTagsArgs { card_id: i64 }

pub async fn get_all_tags_full() -> Vec<Tag> {
    tauri("get_all_tags_full", ()).await
}

/// Every tag, including ones not yet on any card (used by the knowledge map).
pub async fn get_all_tags() -> Vec<Tag> {
    tauri("get_all_tags", ()).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateTagArgs { name: String }

/// Create a standalone tag. Returns None on a blank/duplicate name.
pub async fn create_tag(name: String) -> Option<Tag> {
    tauri_try("create_tag", CreateTagArgs { name }).await.ok()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RenameTagArgs { tag_id: i64, name: String }

/// Rename a tag. Returns None on a blank/duplicate name.
pub async fn rename_tag(tag_id: i64, name: String) -> Option<Tag> {
    tauri_try("rename_tag", RenameTagArgs { tag_id, name }).await.ok()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeleteTagArgs { tag_id: i64 }

/// Delete a tag globally (also removes it from every card and the map).
pub async fn delete_tag(tag_id: i64) {
    let _: Result<(), String> = tauri_try("delete_tag", DeleteTagArgs { tag_id }).await;
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GetCardsByTagsArgs { tag_ids: Vec<i64> }

pub async fn get_cards_by_tags(tag_ids: Vec<i64>) -> Vec<Card> {
    tauri("get_cards_by_tags", GetCardsByTagsArgs { tag_ids }).await
}

pub async fn get_card_tags(card_id: i64) -> Vec<Tag> {
    tauri("get_card_tags", GetCardTagsArgs { card_id }).await
}

/// A card's tags, each with its position in that tag's progression.
/// Uses the catching invoke so a rejected call (e.g. an older backend binary
/// without the command) degrades to an empty list instead of panicking the
/// whole WASM app — a panic here would freeze every input in the UI.
pub async fn get_card_tags_with_position(card_id: i64) -> Vec<CardTagInfo> {
    tauri_try("get_card_tags_with_position", GetCardTagsArgs { card_id }).await.unwrap_or_default()
}

/// (tag_id, card_id, position) for the given tags — the progression numbers
/// shown beside cards in the knowledge map's card list.
pub async fn get_card_tag_positions(tag_ids: Vec<i64>) -> Vec<(i64, i64, i64)> {
    if tag_ids.is_empty() { return Vec::new(); }
    tauri_try("get_card_tag_positions", GetCardsByTagsArgs { tag_ids }).await.unwrap_or_default()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetCardTagPositionArgs { tag_id: i64, card_id: i64, position: i64 }

/// Set a card's position within one tag's progression.
pub async fn set_card_tag_position(tag_id: i64, card_id: i64, position: i64) {
    let _: Result<(), String> = tauri_try("set_card_tag_position", SetCardTagPositionArgs { tag_id, card_id, position }).await;
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncCardTagsArgs {
    card_id: i64,
    tag_names: Vec<String>,
}

pub async fn sync_card_tags(card_id: i64, tag_names: Vec<String>) -> Vec<Tag> {
    tauri("sync_card_tags", SyncCardTagsArgs { card_id, tag_names }).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TagEdgeArgs { parent_id: i64, child_id: i64 }

/// All knowledge-map edges as (parent_id, child_id) pairs.
pub async fn get_tag_edges() -> Vec<(i64, i64)> {
    tauri("get_tag_edges", ()).await
}

/// Link parent → child in the knowledge map (a tag may have many parents).
pub async fn add_tag_edge(parent_id: i64, child_id: i64) {
    let _: () = tauri("add_tag_edge", TagEdgeArgs { parent_id, child_id }).await;
}

/// Remove a parent → child link.
pub async fn remove_tag_edge(parent_id: i64, child_id: i64) {
    let _: () = tauri("remove_tag_edge", TagEdgeArgs { parent_id, child_id }).await;
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TagDeckArgs { tag_id: i64, deck_id: i64 }

/// All (tag_id, deck_id) memberships (card-derived + explicit) for the map filter.
pub async fn get_tag_deck_pairs() -> Vec<(i64, i64)> {
    tauri("get_tag_deck_pairs", ()).await
}

/// Explicitly assign a tag to a deck (no card needed).
pub async fn assign_tag_to_deck(tag_id: i64, deck_id: i64) {
    let _: () = tauri("assign_tag_to_deck", TagDeckArgs { tag_id, deck_id }).await;
}

/// Remove an explicit tag → deck assignment.
pub async fn unassign_tag_from_deck(tag_id: i64, deck_id: i64) {
    let _: () = tauri("unassign_tag_from_deck", TagDeckArgs { tag_id, deck_id }).await;
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DetachCardsArgs { tag_id: i64 }

/// Remove the tag from every card it's on (keeps the tag).
pub async fn detach_tag_from_cards(tag_id: i64) {
    let _: () = tauri("detach_tag_from_cards", DetachCardsArgs { tag_id }).await;
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetTagPositionArgs { tag_id: i64, x: f64, y: f64 }

/// Persist a tag's position on the knowledge-map canvas.
pub async fn set_tag_position(tag_id: i64, x: f64, y: f64) {
    let _: () = tauri("set_tag_position", SetTagPositionArgs { tag_id, x, y }).await;
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClearTagPositionArgs { tag_id: i64 }

/// Clear a tag's canvas position (return it to the unplaced sidebar).
pub async fn clear_tag_position(tag_id: i64) {
    let _: () = tauri("clear_tag_position", ClearTagPositionArgs { tag_id }).await;
}

/// Reset the whole knowledge map (clear all positions + parent links).
pub async fn reset_tag_map() {
    let _: () = tauri("reset_tag_map", ()).await;
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GetDeckTagsArgs { deck_id: i64 }

pub async fn get_deck_tags(deck_id: i64) -> Vec<Tag> {
    tauri("get_deck_tags", GetDeckTagsArgs { deck_id }).await
}

pub async fn get_card_tag_pairs(deck_id: i64) -> Vec<(i64, i64)> {
    tauri("get_card_tag_pairs", GetDeckTagsArgs { deck_id }).await
}

// ---------------------------------------------------------------------------
// Study / scoring
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateCardScoreArgs {
    card_id: i64,
    correct: bool,
}

/// Records a study answer and returns the updated card with new scores.
pub async fn update_score(card_id: i64, correct: bool) -> Card {
    tauri("update_score", UpdateCardScoreArgs { card_id, correct }).await
}

// ---------------------------------------------------------------------------
// File operations
// ---------------------------------------------------------------------------

/// DTO returned by the fs-adapter file-picker commands.
/// Defined locally because it only crosses the JSON bridge — no shared Rust type needed.
#[derive(serde::Deserialize)]
struct FileResponse {
    path: String,
}

/// Opens a file picker for images and copies the chosen file into app data.
/// Returns the virtual path (e.g. "files/uuid.png"), or empty string if cancelled.
pub async fn pick_image() -> String {
    // Catching invoke: a rejected command must not abort this task, or the
    // caller's upload overlay would spin with nothing left to clear it.
    let ret: Result<Option<FileResponse>, _> = tauri_try("plugin:fs-adapter|pick_image", ()).await;
    ret.ok().flatten().map(|r| r.path).unwrap_or_default()
}

/// Same as pick_image on Tauri — resize happens on the web server, not desktop.
pub async fn pick_cover_image() -> String {
    pick_image().await
}

/// Opens a file picker for audio files and copies the chosen file into app data.
/// Returns the virtual path (e.g. "files/uuid.mp3"), or empty string if cancelled.
pub async fn pick_audio() -> String {
    // Catching invoke: a rejected command must not abort this task, or the
    // caller's upload overlay would spin with nothing left to clear it.
    let ret: Result<Option<FileResponse>, _> = tauri_try("plugin:fs-adapter|pick_audio", ()).await;
    ret.ok().flatten().map(|r| r.path).unwrap_or_default()
}

/// Opens a file picker for video files and copies the chosen file into app data.
/// Returns the virtual path (e.g. "files/uuid.mp4"), or empty string if cancelled.
pub async fn pick_video() -> String {
    // Catching invoke: a rejected command must not abort this task, or the
    // caller's upload overlay would spin with nothing left to clear it.
    let ret: Result<Option<FileResponse>, _> = tauri_try("plugin:fs-adapter|pick_video", ()).await;
    ret.ok().flatten().map(|r| r.path).unwrap_or_default()
}

/// Opens a file picker for archives and copies the chosen file into app data.
/// Returns the virtual path, or empty string if cancelled.
pub async fn pick_archive() -> String {
    // Catching invoke: a rejected command must not abort this task, or the
    // caller's upload overlay would spin with nothing left to clear it.
    let ret: Result<Option<FileResponse>, _> = tauri_try("plugin:fs-adapter|pick_archive", ()).await;
    ret.ok().flatten().map(|r| r.path).unwrap_or_default()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadFileArgs {
    virtual_path: String,
}

/// Prompts the user to save a file from app data to a location of their choice.
pub async fn download_file(path: String) {
    let _: () = tauri("download_file", DownloadFileArgs { virtual_path: path }).await;
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeleteBlockArgs {
    virtual_path: String,
}

/// Deletes a file asset (image or archive) from app data.
pub async fn delete_block_from_app_data(virtual_path: String) {
    let _: () = tauri("delete_block_from_app_data", DeleteBlockArgs { virtual_path }).await;
}

// ---------------------------------------------------------------------------
// Cover images
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetDeckCoverArgs {
    deck_id: i64,
    virtual_path: Option<String>,
}

/// Set or clear the cover image for a deck.
/// Pass `Some(virtual_path)` to set, `None` to remove.
pub async fn set_deck_cover(deck_id: i64, virtual_path: Option<String>) {
    let _: () = tauri("set_deck_cover", SetDeckCoverArgs { deck_id, virtual_path }).await;
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetCardCoverArgs {
    card_id: i64,
    virtual_path: Option<String>,
}

/// Set or clear the cover image for a card.
/// Pass `Some(virtual_path)` to set, `None` to remove.
pub async fn set_card_cover(card_id: i64, virtual_path: Option<String>) {
    let _: () = tauri("set_card_cover", SetCardCoverArgs { card_id, virtual_path }).await;
}

// ---------------------------------------------------------------------------
// Import / Export
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportDeckArgs {
    deck_id: i64,
}

/// Exports a deck to a zip file at a user-chosen location.
pub async fn export_deck(deck_id: i64) {
    let _: () = tauri("export_deck", ExportDeckArgs { deck_id }).await;
}

/// Opens a file picker for a zip archive and imports the deck.
/// Returns the new deck id, or 0 if the user cancelled.
pub async fn import_deck() -> i64 {
    tauri("import_deck", ()).await
}

// ---------------------------------------------------------------------------
// Auth — Tauri is a single-user desktop app; auth is always bypassed.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct UserInfo {
    pub id:       String,
    pub username: String,
}

pub async fn login(_username: String, _password: String) -> Option<UserInfo> {
    Some(UserInfo { id: "local".into(), username: "local".into() })
}

pub async fn logout() {}

pub async fn get_current_user() -> Option<UserInfo> {
    Some(UserInfo { id: "local".into(), username: "local".into() })
}

pub async fn register(_username: String, _password: String, _token: String) -> Option<UserInfo> {
    Some(UserInfo { id: "local".into(), username: "local".into() })
}

// ---------------------------------------------------------------------------
// Markdown rendering
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RenderMarkdownArgs { text: String }

pub async fn render_markdown(text: String) -> String {
    tauri("render_markdown", RenderMarkdownArgs { text }).await
}
