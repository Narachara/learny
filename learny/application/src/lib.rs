pub mod deck_transfer;
pub mod flashcard_service;
pub mod study_service;
pub mod tagging_service;
pub mod analytics_service;

/// Errors that can occur in any application service.
/// Services map domain and repository errors into this type.
#[derive(Debug)]
pub enum AppError {
    /// The requested entity does not exist.
    NotFound,
    /// Input violates a domain rule (e.g. empty name).
    ValidationError(String),
    /// A repository operation failed.
    Repository(ports::RepositoryError),
    /// A file or archive operation failed.
    Io(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::NotFound                => write!(f, "not found"),
            AppError::ValidationError(msg)    => write!(f, "validation error: {}", msg),
            AppError::Repository(e)           => write!(f, "repository error: {}", e),
            AppError::Io(msg)                 => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for AppError {}

impl From<ports::RepositoryError> for AppError {
    fn from(e: ports::RepositoryError) -> Self {
        AppError::Repository(e)
    }
}
