use std::sync::Arc;
use async_trait::async_trait;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum_login::{AuthUser, AuthnBackend, UserId};
use domain_auth::LoginCredentials;
use ports::user_repo::UserRepository;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// SessionUser — newtype over domain_auth::User so we own the type and can
// implement the foreign AuthUser trait without violating the orphan rule.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionUser(pub domain_auth::User);

impl AuthUser for SessionUser {
    type Id = String;

    fn id(&self) -> String {
        self.0.id.clone()
    }

    /// Hash is part of the session token; changes on password update.
    fn session_auth_hash(&self) -> &[u8] {
        self.0.password_hash.as_bytes()
    }
}

// ---------------------------------------------------------------------------
// AuthBackend
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct AuthBackend {
    repo: Arc<dyn UserRepository>,
}

impl AuthBackend {
    pub fn new(repo: Arc<dyn UserRepository>) -> Self {
        Self { repo }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("repository error: {0}")]
    Repo(String),
    #[error("password hash error")]
    Hash,
}

#[async_trait]
impl AuthnBackend for AuthBackend {
    type User        = SessionUser;
    type Credentials = LoginCredentials;
    type Error       = BackendError;

    async fn authenticate(
        &self,
        creds: LoginCredentials,
    ) -> Result<Option<SessionUser>, BackendError> {
        let repo = self.repo.clone();
        let username = creds.username.clone();

        let user = tokio::task::spawn_blocking(move || {
            repo.find_by_username(&username)
        })
        .await
        .map_err(|e| BackendError::Repo(e.to_string()))?
        .map_err(|e| BackendError::Repo(e.to_string()))?;

        let Some(user) = user else { return Ok(None) };

        let hash     = user.password_hash.clone();
        let password = creds.password.clone();

        let ok = tokio::task::spawn_blocking(move || {
            let parsed = PasswordHash::new(&hash).map_err(|_| BackendError::Hash)?;
            Ok::<bool, BackendError>(
                Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok()
            )
        })
        .await
        .map_err(|e| BackendError::Repo(e.to_string()))??;

        Ok(if ok { Some(SessionUser(user)) } else { None })
    }

    async fn get_user(
        &self,
        user_id: &UserId<Self>,
    ) -> Result<Option<SessionUser>, BackendError> {
        let repo = self.repo.clone();
        let id   = user_id.clone();
        let user = tokio::task::spawn_blocking(move || repo.find_by_id(&id))
            .await
            .map_err(|e| BackendError::Repo(e.to_string()))?
            .map_err(|e| BackendError::Repo(e.to_string()))?;
        Ok(user.map(SessionUser))
    }
}

/// Convenience alias used in handler signatures.
pub type AuthSession = axum_login::AuthSession<AuthBackend>;
