// Central error type for the application.
//
// Each variant maps to a distinct failure mode. The IntoResponse implementation
// converts errors into HTTP responses automatically when returned from axum handlers,
// so handlers can use `?` without manually setting status codes.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Missing environment variable: {0}")]
    MissingEnvVar(&'static str),

    // #[from] generates From<reqwest::Error> so the ? operator converts HTTP errors automatically.
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Failed to parse API response: {0}")]
    ParseError(String),

    #[error("AI interpretation error: {0}")]
    AiError(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::MissingEnvVar(var) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Server misconfiguration: missing {}", var),
            ),
            AppError::HttpError(_) => (
                StatusCode::BAD_GATEWAY,
                "Failed to reach upstream data source".to_string(),
            ),
            AppError::ParseError(msg) => (
                StatusCode::BAD_GATEWAY,
                format!("Failed to parse upstream data: {}", msg),
            ),
            AppError::AiError(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("AI service error: {}", msg),
            ),
            AppError::Internal(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Internal error: {}", msg),
            ),
        };

        (status, Json(json!({ "error": message }))).into_response()
    }
}
