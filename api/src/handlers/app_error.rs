use axum::extract::rejection::PathRejection;
use axum::http::{StatusCode};
use axum::Json;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq)]
pub struct AppError {
    pub status: StatusCode,
    pub body: ErrorResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorResponse {
    pub error: String,

    pub message: String,
}

impl AppError {
    pub fn new(status: StatusCode, error: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status,
            body: ErrorResponse {
                error: error.into(),
                message: message.into(),
            },
        }
    }

    pub fn bad_request(error: impl Into<String>, message: impl Into<String>) -> Self {
        AppError::new(StatusCode::BAD_REQUEST, error, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", message)
    }

    pub fn internal_error() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "An internal server error occurred",
        )
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}

impl From<PathRejection> for AppError {
    fn from(rejection: PathRejection) -> Self {
        match rejection {
            PathRejection::FailedToDeserializePathParams(inner) => {
                let message = inner.body_text();
                Self::bad_request("invalid_path_parameter", &message)
            }
            PathRejection::MissingPathParams(inner) => {
                let message = inner.body_text();
                Self::bad_request("missing_path_parameter", &message)
            }
            _ => {
                Self::bad_request("invalid_path", "Invalid path parameter")
            }
        }
    }
}