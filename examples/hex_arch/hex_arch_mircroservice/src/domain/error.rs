use axum::{http::StatusCode, response::IntoResponse};
use prk_eventbus::core::EventBusError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    #[error("Event bus client error: {0}")]
    EventBusClientError(#[from] EventBusError),
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        match self {
            AppError::Database(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
            }
            AppError::EventBusClientError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
            }
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg).into_response(),
            AppError::Unauthorized(_) => {
                (StatusCode::UNAUTHORIZED, self.to_string()).into_response()
            }
        }
    }
}
