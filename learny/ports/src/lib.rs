pub mod card_repo;
pub mod deck_repo;
pub mod tag_repo;
pub mod user_repo;
pub mod transaction;

/// Shared error type for all repository operations.
/// Adapters map their internal errors (rusqlite, IO, etc.) into this type.
#[derive(Debug)]
pub enum RepositoryError {
    NotFound,
    DuplicateName,
    Constraint(String), // FK violations, unique conflicts
    Internal(String),   // unexpected DB or IO errors
}

impl std::fmt::Display for RepositoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RepositoryError::NotFound        => write!(f, "record not found"),
            RepositoryError::DuplicateName   => write!(f, "name already exists"),
            RepositoryError::Constraint(msg) => write!(f, "constraint violation: {}", msg),
            RepositoryError::Internal(msg)   => write!(f, "internal error: {}", msg),
        }
    }
}

impl std::error::Error for RepositoryError {}
