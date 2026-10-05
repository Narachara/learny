use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2,
};
use axum::{http::StatusCode, response::IntoResponse, Extension, Json};
use domain_auth::LoginCredentials;
use ports::user_repo::UserRepository;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::auth_backend::AuthSession;

// ---------------------------------------------------------------------------
// Shared response type
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct UserResponse {
    pub id:       String,
    pub username: String,
}

impl From<domain_auth::User> for UserResponse {
    fn from(u: domain_auth::User) -> Self {
        UserResponse { id: u.id, username: u.username }
    }
}

// ---------------------------------------------------------------------------
// POST /api/auth/login
// ---------------------------------------------------------------------------

pub async fn login(
    mut auth_session: AuthSession,
    Json(creds): Json<LoginCredentials>,
) -> impl IntoResponse {
    match auth_session.authenticate(creds).await {
        Ok(Some(user)) => {
            if let Err(e) = auth_session.login(&user).await {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    e.to_string(),
                ).into_response();
            }
            Json(UserResponse::from(user.0)).into_response()
        }
        Ok(None) => StatusCode::UNAUTHORIZED.into_response(),
        Err(e)   => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// POST /api/auth/logout
// ---------------------------------------------------------------------------

pub async fn logout(mut auth_session: AuthSession) -> impl IntoResponse {
    if let Err(e) = auth_session.logout().await {
        return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }
    StatusCode::NO_CONTENT.into_response()
}

// ---------------------------------------------------------------------------
// GET /api/auth/me — returns current user or 401
// ---------------------------------------------------------------------------

pub async fn me(auth_session: AuthSession) -> impl IntoResponse {
    match auth_session.user {
        Some(user) => Json(UserResponse::from(user.0)).into_response(),
        None       => StatusCode::UNAUTHORIZED.into_response(),
    }
}

// ---------------------------------------------------------------------------
// POST /api/auth/register
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct RegisterBody {
    username: String,
    password: String,
    token:    Option<String>,
}

pub async fn register(
    mut auth_session: AuthSession,
    Extension(user_repo): Extension<Arc<dyn UserRepository>>,
    Json(body): Json<RegisterBody>,
) -> impl IntoResponse {
    if let Ok(secret) = std::env::var("REGISTER_SECRET") {
        let provided = body.token.as_deref().unwrap_or("");
        if provided != secret {
            return (StatusCode::FORBIDDEN, "invalid registration token").into_response();
        }
    }

    if body.username.trim().is_empty() || body.password.len() < 6 {
        return (StatusCode::BAD_REQUEST, "username required and password must be ≥ 6 chars")
            .into_response();
    }

    let password = body.password.clone();
    let hash = match tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|e| e.to_string())
    })
    .await
    {
        Ok(Ok(h))  => h,
        Ok(Err(e)) => return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
        Err(e)     => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let repo = user_repo.clone();
    let username = body.username.trim().to_string();
    match tokio::task::spawn_blocking(move || repo.create(&username, &hash)).await {
        Ok(Ok(user)) => {
            let session_user = crate::auth_backend::SessionUser(user.clone());
            if let Err(e) = auth_session.login(&session_user).await {
                return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
            }
            (StatusCode::CREATED, Json(UserResponse::from(user))).into_response()
        },
        Ok(Err(ports::RepositoryError::DuplicateName)) =>
            (StatusCode::CONFLICT, "username already taken").into_response(),
        Ok(Err(e)) =>
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        Err(e) =>
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
