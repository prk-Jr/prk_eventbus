use actix_web::{error::Error as ActixError, http::StatusCode, HttpResponse};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
}

impl actix_web::error::ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        match self {
            AppError::Database(_) => HttpResponse::InternalServerError().body(self.to_string()),
            AppError::NotFound(msg) => HttpResponse::NotFound().body(msg.clone()),
            AppError::Unauthorized(_) => HttpResponse::Unauthorized().body(self.to_string()),
        }
    }
}
