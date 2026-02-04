use axum::extract::rejection::{JsonRejection, PathRejection};
use axum::extract::{FromRequest, FromRequestParts, Path, Request};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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

    pub fn task_not_found(task_id: Uuid) -> Self {
        Self::not_found(format!("Task with id '{}' not found", task_id))
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
            _ => Self::bad_request("invalid_path", "Invalid path parameter"),
        }
    }
}

impl From<JsonRejection> for AppError {
    fn from(rejection: JsonRejection) -> Self {
        let message = rejection.body_text();
        Self::bad_request("invalid_json", message)
    }
}

// ============================================================================
// Custom Extractors
// ============================================================================

/// Custom Path extractor that returns AppError on rejection.
pub struct AppPath<T>(pub T);

impl<T, S> FromRequestParts<S> for AppPath<T>
where
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        Path::<T>::from_request_parts(parts, state)
            .await
            .map(|Path(v)| AppPath(v))
            .map_err(AppError::from)
    }
}

/// Custom Json extractor that returns AppError on rejection.
pub struct AppJson<T>(pub T);

impl<T, S> FromRequest<S> for AppJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        Json::<T>::from_request(req, state)
            .await
            .map(|Json(v)| AppJson(v))
            .map_err(AppError::from)
    }
}