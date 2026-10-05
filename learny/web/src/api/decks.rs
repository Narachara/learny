use crate::api::error::ApiError;
use axum::{
    extract::{Extension, Multipart, Path},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use std::io::Cursor;
use crate::auth_backend::AuthSession;
use crate::state::SharedServiceFactory;
use crate::api::files::DataDir;

fn user_id(auth_session: &AuthSession) -> Option<String> {
    auth_session.user.as_ref().map(|u| u.0.id.clone())
}

// ---------------------------------------------------------------------------
// GET /api/decks
// ---------------------------------------------------------------------------

pub async fn list(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.flashcard.get_decks() {
        Ok(decks) => Json(decks).into_response(),
        Err(e)    => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// POST /api/decks  { "name": "..." }
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateDeckBody {
    name: String,
}

pub async fn create(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Json(body): Json<CreateDeckBody>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.flashcard.create_deck(&body.name) {
        Ok(deck) => (StatusCode::CREATED, Json(deck)).into_response(),
        Err(e)   => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// PUT /api/decks/:id/rename  { "name": "..." }
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct RenameDeckBody {
    name: String,
}

pub async fn rename(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Path(id): Path<i64>,
    Json(body): Json<RenameDeckBody>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.flashcard.rename_deck(id, &body.name) {
        Ok(deck) => Json(deck).into_response(),
        Err(e)   => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// DELETE /api/decks/:id
// ---------------------------------------------------------------------------

pub async fn delete(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.flashcard.delete_deck(id) {
        Ok(())   => StatusCode::NO_CONTENT.into_response(),
        Err(e)   => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// PUT /api/decks/:id/cover  { "virtual_path": "files/uuid.png" | null }
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
    match svc.flashcard.set_deck_cover(id, body.virtual_path) {
        Ok(())   => StatusCode::NO_CONTENT.into_response(),
        Err(e)   => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// GET /api/decks/:id/export — transport only; the archive format lives in
// application::deck_transfer.
// ---------------------------------------------------------------------------

pub async fn export(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Extension(data_dir): Extension<DataDir>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };

    let dir = data_dir.as_ref().clone();
    let result = tokio::task::spawn_blocking(move || {
        let svc = factory.for_user(&uid);
        let mut buf = Vec::new();
        application::deck_transfer::export_deck(
            &svc.flashcard,
            &svc.tagging,
            id,
            &dir,
            Cursor::new(&mut buf),
        )?;
        Ok::<Vec<u8>, application::AppError>(buf)
    })
    .await;

    match result {
        Ok(Ok(bytes)) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/zip"),
                (header::CONTENT_DISPOSITION, "attachment; filename=\"deck-export.zip\""),
            ],
            bytes,
        ).into_response(),
        Ok(Err(e)) => ApiError(e).into_response(),
        Err(e)     => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// POST /api/decks/import
// ---------------------------------------------------------------------------

pub async fn import_deck(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Extension(data_dir): Extension<DataDir>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };

    let mut zip_bytes: Option<Vec<u8>> = None;
    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("file") {
            match field.bytes().await {
                Ok(b)  => { zip_bytes = Some(b.to_vec()); break; }
                Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
            }
        }
    }

    let bytes = match zip_bytes {
        Some(b) => b,
        None    => return (StatusCode::BAD_REQUEST, "missing file field").into_response(),
    };

    let dir = data_dir.as_ref().clone();
    let result = tokio::task::spawn_blocking(move || {
        let svc = factory.for_user(&uid);
        application::deck_transfer::import_deck(
            &svc.flashcard,
            &svc.tagging,
            svc.tx.as_ref(),
            &dir,
            &bytes,
        )
    })
    .await;

    match result {
        Ok(Ok(deck_id)) => Json(deck_id).into_response(),
        Ok(Err(e))      => ApiError(e).into_response(),
        Err(e)          => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// GET /api/decks/:id
// ---------------------------------------------------------------------------

pub async fn get(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.flashcard.get_deck(id) {
        Ok(deck) => Json(deck).into_response(),
        Err(e)   => (StatusCode::NOT_FOUND, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// POST /api/decks/:id/reset-progress
// ---------------------------------------------------------------------------

pub async fn reset_progress(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.flashcard.reset_deck_progress(id) {
        Ok(())  => StatusCode::NO_CONTENT.into_response(),
        Err(e)  => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// GET /api/decks/:id/deck-tags  →  all tags used by cards in this deck
// ---------------------------------------------------------------------------

pub async fn get_card_tag_pairs(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.tagging.card_tag_pairs_for_deck(id) {
        Ok(pairs) => Json(pairs).into_response(),
        Err(e)    => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn get_deck_tags(
    auth_session: AuthSession,
    Extension(factory): Extension<SharedServiceFactory>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let Some(uid) = user_id(&auth_session) else { return StatusCode::UNAUTHORIZED.into_response() };
    let svc = factory.for_user(&uid);
    match svc.tagging.tags_for_deck(id) {
        Ok(tags) => Json(tags).into_response(),
        Err(e)   => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
