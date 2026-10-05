use application::AppError;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// Web-side wrapper mapping application errors to HTTP responses in one place.
/// Handlers can `?` any `AppError` by returning `Result<_, ApiError>`.
pub struct ApiError(pub AppError);

impl From<AppError> for ApiError {
    fn from(e: AppError) -> Self {
        ApiError(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self.0 {
            AppError::NotFound            => StatusCode::NOT_FOUND,
            AppError::ValidationError(_)  => StatusCode::BAD_REQUEST,
            AppError::Io(_)               => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Repository(_)       => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, self.0.to_string()).into_response()
    }
}
