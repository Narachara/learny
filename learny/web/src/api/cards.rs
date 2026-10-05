use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use domain_flashcard::Block;
use crate::auth_backend::AuthSession;
use crate::state::SharedServiceFactory;

fn user_id(auth_session: &AuthSession) -> Option<String> {
    auth_session.user.as_ref().map(|u| u.0.id.clone())
}

// ---------------------------------------------------------------------------
// GET /api/decks/:deck_id/cards
// ---------------------------------------------------------------------------

pub async fn list(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Path(deck_id): Path<i64>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.flashcard.get_cards(deck_id) {
        Ok(cards) => Json(cards).into_response(),
        Err(e)    => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// POST /api/decks/:deck_id/cards  { "name": "..." }
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateCardBody {
    name: String,
}

pub async fn create(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Path(deck_id): Path<i64>,
    Json(body): Json<CreateCardBody>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.flashcard.add_card(deck_id, &body.name) {
        Ok(card) => (StatusCode::CREATED, Json(card.id)).into_response(),
        Err(e)   => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// GET /api/cards/:id
// ---------------------------------------------------------------------------

pub async fn get(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.flashcard.get_card(id) {
        Ok(card) => Json(card).into_response(),
        Err(e)   => (StatusCode::NOT_FOUND, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// PUT /api/cards/:id/name  { "name": "..." }
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct RenameCardBody {
    name: String,
}

pub async fn rename(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Path(id): Path<i64>,
    Json(body): Json<RenameCardBody>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.flashcard.rename_card(id, &body.name) {
        Ok(_)  => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// PUT /api/cards/:id/blocks  { "front": [...], "back": [...] }
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SaveBlocksBody {
    front: Vec<Block>,
    back:  Vec<Block>,
}

pub async fn save_blocks(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Path(id): Path<i64>,
    Json(body): Json<SaveBlocksBody>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.flashcard.save_blocks(id, body.front, body.back) {
        Ok(())   => StatusCode::NO_CONTENT.into_response(),
        Err(e)   => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// DELETE /api/cards/:id
// ---------------------------------------------------------------------------

pub async fn delete(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.flashcard.delete_card(id) {
        Ok(())   => StatusCode::NO_CONTENT.into_response(),
        Err(e)   => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// POST /api/cards/:id/answer  { "correct": true }
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct AnswerBody {
    correct: bool,
}

pub async fn answer(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Path(id): Path<i64>,
    Json(body): Json<AnswerBody>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.study.rate_card(id, body.correct) {
        Ok(card) => Json(card).into_response(),
        Err(e)   => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// PUT /api/cards/:id/cover  { "virtual_path": "files/uuid.png" | null }
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SetCoverBody {
    virtual_path: Option<String>,
}

pub async fn set_cover(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Path(id): Path<i64>,
    Json(body): Json<SetCoverBody>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.flashcard.set_card_cover(id, body.virtual_path) {
        Ok(())   => StatusCode::NO_CONTENT.into_response(),
        Err(e)   => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// PUT /api/cards/:id/notes  { "easy": "...", "difficult": "..." }
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SaveNotesBody {
    easy:      Option<String>,
    difficult: Option<String>,
}

pub async fn save_notes(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Path(id): Path<i64>,
    Json(body): Json<SaveNotesBody>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.flashcard.save_card_notes(id, body.easy, body.difficult) {
        Ok(())   => StatusCode::NO_CONTENT.into_response(),
        Err(e)   => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// GET /api/cards/:id/tags
// ---------------------------------------------------------------------------

pub async fn get_tags(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    // Verify ownership
    if svc.flashcard.get_card(id).is_err() {
        return StatusCode::NOT_FOUND.into_response();
    }
    match svc.tagging.tags_for_card(id) {
        Ok(tags) => Json(tags).into_response(),
        Err(e)   => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// GET /api/cards/:id/tags-with-position
// ---------------------------------------------------------------------------

pub async fn get_tags_with_position(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    if svc.flashcard.get_card(id).is_err() {
        return StatusCode::NOT_FOUND.into_response();
    }
    match svc.tagging.tags_for_card_with_position(id) {
        Ok(infos) => Json(infos).into_response(),
        Err(e)    => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// PUT /api/cards/:id/tags/:tag_id/position  { "position": 2 }
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SetCardTagPositionBody {
    position: i64,
}

pub async fn set_card_tag_position(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Path((id, tag_id)): Path<(i64, i64)>,
    Json(body): Json<SetCardTagPositionBody>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    if svc.flashcard.get_card(id).is_err() {
        return StatusCode::NOT_FOUND.into_response();
    }
    match svc.tagging.set_card_tag_position(tag_id, id, body.position) {
        Ok(())  => StatusCode::NO_CONTENT.into_response(),
        Err(e)  => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// PUT /api/cards/:id/tags  { "tag_names": ["grammar", "verb"] }
// (Replaces all tags on the card)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SyncTagsBody {
    tag_names: Vec<String>,
}

pub async fn sync_tags(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Path(id): Path<i64>,
    Json(body): Json<SyncTagsBody>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    // Verify ownership
    if svc.flashcard.get_card(id).is_err() {
        return StatusCode::NOT_FOUND.into_response();
    }
    match svc.tagging.sync_tags_for_card(id, body.tag_names) {
        Ok(tags) => Json(tags).into_response(),
        Err(e)   => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// POST /api/cards/move  { "card_ids": [1,2,3], "target_deck_id": 5 }
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct MoveCardsBody {
    pub card_ids:       Vec<i64>,
    pub target_deck_id: i64,
}

// ---------------------------------------------------------------------------
// GET /api/tags-full  →  all tags with ids
// ---------------------------------------------------------------------------

pub async fn get_all_tags_full(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.tagging.get_tags_in_use() {
        Ok(tags) => Json(tags).into_response(),
        Err(e)   => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// GET /api/tags  →  every tag (including those on no cards)
// ---------------------------------------------------------------------------

pub async fn get_all_tags(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.tagging.get_all_tags() {
        Ok(tags) => Json(tags).into_response(),
        Err(e)   => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// POST /api/tags  { "name": "graphs" }  →  the created tag
// DELETE /api/tags/:id  →  delete a tag (and its card links / map edges)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateTagBody {
    name: String,
}

pub async fn create_tag(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Json(body): Json<CreateTagBody>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.tagging.create_tag(&body.name) {
        Ok(tag) => Json(tag).into_response(),
        Err(e)  => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
pub struct RenameTagBody {
    name: String,
}

pub async fn rename_tag(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Path(id): Path<i64>,
    Json(body): Json<RenameTagBody>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.tagging.rename_tag(id, &body.name) {
        Ok(tag) => Json(tag).into_response(),
        Err(e)  => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn delete_tag(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.tagging.delete_tag(id) {
        Ok(())  => StatusCode::OK.into_response(),
        Err(e)  => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// GET /api/tags/edges  →  [[parent_id, child_id], ...]
// ---------------------------------------------------------------------------

pub async fn get_tag_edges(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.tagging.get_tag_edges() {
        Ok(edges) => Json(edges).into_response(),
        Err(e)    => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// POST /api/tags/edges    { "parent_id": 1, "child_id": 2 }  → add a link
// DELETE /api/tags/edges  { "parent_id": 1, "child_id": 2 }  → remove a link
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct EdgeBody {
    parent_id: i64,
    child_id: i64,
}

pub async fn add_tag_edge(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Json(body): Json<EdgeBody>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.tagging.add_tag_edge(body.parent_id, body.child_id) {
        Ok(())  => StatusCode::OK.into_response(),
        Err(e)  => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn remove_tag_edge(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Json(body): Json<EdgeBody>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.tagging.remove_tag_edge(body.parent_id, body.child_id) {
        Ok(())  => StatusCode::OK.into_response(),
        Err(e)  => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// GET /api/tags/deck-pairs  →  [[tag_id, deck_id], ...] (derived + explicit)
// POST /api/tags/deck       { tag_id, deck_id }  → assign
// DELETE /api/tags/deck     { tag_id, deck_id }  → unassign
// ---------------------------------------------------------------------------

pub async fn get_tag_deck_pairs(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.tagging.get_tag_deck_pairs() {
        Ok(pairs) => Json(pairs).into_response(),
        Err(e)    => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
pub struct TagDeckBody {
    tag_id: i64,
    deck_id: i64,
}

pub async fn assign_tag_to_deck(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Json(body): Json<TagDeckBody>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.tagging.assign_tag_to_deck(body.tag_id, body.deck_id) {
        Ok(())  => StatusCode::OK.into_response(),
        Err(e)  => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn unassign_tag_from_deck(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Json(body): Json<TagDeckBody>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.tagging.unassign_tag_from_deck(body.tag_id, body.deck_id) {
        Ok(())  => StatusCode::OK.into_response(),
        Err(e)  => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// DELETE /api/tags/:id/cards  → remove the tag from every card (keeps the tag)
// ---------------------------------------------------------------------------

pub async fn detach_tag_from_cards(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.tagging.detach_tag_from_cards(id) {
        Ok(())  => StatusCode::OK.into_response(),
        Err(e)  => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// PUT /api/tags/:id/position  { "x": 12.0, "y": 34.0 }
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SetPositionBody {
    x: f64,
    y: f64,
}

pub async fn set_tag_position(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Path(id): Path<i64>,
    Json(body): Json<SetPositionBody>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.tagging.set_tag_position(id, body.x, body.y) {
        Ok(())  => StatusCode::OK.into_response(),
        Err(e)  => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// DELETE /api/tags/:id/position  (return tag to the unplaced sidebar)
// ---------------------------------------------------------------------------

pub async fn clear_tag_position(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.tagging.clear_tag_position(id) {
        Ok(())  => StatusCode::OK.into_response(),
        Err(e)  => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// POST /api/tags/reset-map  (clear all positions + parent links)
// ---------------------------------------------------------------------------

pub async fn reset_tag_map(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.tagging.reset_tag_map() {
        Ok(())  => StatusCode::OK.into_response(),
        Err(e)  => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// GET /api/cards/by-tags?tag_ids=1,2,3
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
pub struct ByTagsQuery {
    tag_ids: String,
}

pub async fn get_cards_by_tags(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    axum::extract::Query(q): axum::extract::Query<ByTagsQuery>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    let ids: Vec<i64> = q.tag_ids.split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    match svc.tagging.cards_by_tags(&ids) {
        Ok(cards) => Json(cards).into_response(),
        Err(e)    => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// GET /api/tags/card-positions?tag_ids=1,2,3
// ---------------------------------------------------------------------------

pub async fn get_card_tag_positions(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    axum::extract::Query(q): axum::extract::Query<ByTagsQuery>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    let ids: Vec<i64> = q.tag_ids.split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    match svc.tagging.card_tag_positions(&ids) {
        Ok(triples) => Json(triples).into_response(),
        Err(e)      => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// GET /api/cards/search?q=keyword
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
pub struct SearchQuery {
    q: String,
}

pub async fn search_cards(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    axum::extract::Query(params): axum::extract::Query<SearchQuery>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.flashcard.search_cards(&params.q) {
        Ok(cards) => Json(cards).into_response(),
        Err(e)    => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn move_cards(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Json(body): Json<MoveCardsBody>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.flashcard.move_cards(body.card_ids, body.target_deck_id) {
        Ok(())  => StatusCode::NO_CONTENT.into_response(),
        Err(e)  => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
