use domain_auth::User;
use crate::RepositoryError;

/// Outbound port for user persistence.
pub trait UserRepository: Send + Sync {
    fn find_by_id(&self, id: &str) -> Result<Option<User>, RepositoryError>;
    /// Looks up by username (exact, case-insensitive).
    fn find_by_username(&self, username: &str) -> Result<Option<User>, RepositoryError>;
    /// Inserts a new user; returns `RepositoryError::DuplicateName` if username is taken.
    fn create(&self, username: &str, password_hash: &str) -> Result<User, RepositoryError>;
}
