// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! API version negotiation: constant, compatibility check, and request middleware.

use axum::Json;
use axum::extract::Request;
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::handlers::app_error::ErrorResponse;

pub const API_VERSION: &str = "0.1";
pub const HEADER_NAME: &str = "x-api-version";

fn parse_version(s: &str) -> Option<(u32, u32)> {
    let (major_str, minor_str) = s.split_once('.')?;
    let major = major_str.parse::<u32>().ok()?;
    let minor = minor_str.parse::<u32>().ok()?;
    Some((major, minor))
}

fn check_compatibility(client: &str, server: &str) -> bool {
    match (parse_version(client), parse_version(server)) {
        (Some((cm, cn)), Some((sm, sn))) => cm == sm && sn >= cn,
        _ => false,
    }
}

pub async fn api_version_middleware(req: Request, next: Next) -> Response {
    let client_version = req
        .headers()
        .get(HEADER_NAME)
        .and_then(|v| v.to_str().ok().map(str::to_owned));

    match client_version {
        None => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "API_VERSION_MISSING".to_string(),
                message: "The X-Api-Version header is required".to_string(),
            }),
        )
            .into_response(),

        Some(ref version) if !check_compatibility(version, API_VERSION) => (
            StatusCode::NOT_ACCEPTABLE,
            Json(ErrorResponse {
                error: "API_VERSION_NOT_SUPPORTED".to_string(),
                message: format!(
                    "Server API version {} is not compatible with client version {}",
                    API_VERSION, version
                ),
            }),
        )
            .into_response(),

        Some(_) => {
            let mut response = next.run(req).await;
            response.headers_mut().insert(
                header::HeaderName::from_static(HEADER_NAME),
                header::HeaderValue::from_static(API_VERSION),
            );
            response
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::middleware;
    use axum::routing::get;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn test_app() -> Router {
        Router::new()
            .route("/test", get(|| async { "ok" }))
            .layer(middleware::from_fn(api_version_middleware))
    }

    // ── Compatibility logic ──────────────────────────────────────────────────

    #[test]
    fn test_check_compatible_same_version() {
        assert!(check_compatibility("1.0", "1.0"));
    }

    #[test]
    fn test_check_compatible_server_minor_higher() {
        assert!(check_compatibility("1.0", "1.3"));
    }

    #[test]
    fn test_check_incompatible_client_minor_higher() {
        assert!(!check_compatibility("1.1", "1.0"));
        assert!(!check_compatibility("1.99", "1.0"));
    }

    #[test]
    fn test_check_incompatible_different_major() {
        assert!(!check_compatibility("2.0", "1.0"));
        assert!(!check_compatibility("0.9", "1.0"));
    }

    #[test]
    fn test_check_incompatible_malformed_version() {
        assert!(!check_compatibility("", "1.0"));
        assert!(!check_compatibility("1", "1.0"));
        assert!(!check_compatibility("1.0.0", "1.0"));
        assert!(!check_compatibility("abc", "1.0"));
        assert!(!check_compatibility("1.abc", "1.0"));
    }

    // ── Middleware ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_middleware_missing_header_returns_400() {
        let response = test_app()
            .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let error: ErrorResponse = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(error.error, "API_VERSION_MISSING");
    }

    #[tokio::test]
    async fn test_middleware_incompatible_major_returns_406() {
        let response = test_app()
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header("X-Api-Version", "2.0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let error: ErrorResponse = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(error.error, "API_VERSION_NOT_SUPPORTED");
    }

    #[tokio::test]
    async fn test_middleware_client_minor_higher_returns_406() {
        let response = test_app()
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header("X-Api-Version", "1.1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE);
    }

    #[tokio::test]
    async fn test_middleware_compatible_version_passes_through() {
        let response = test_app()
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header("X-Api-Version", API_VERSION)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_middleware_compatible_version_adds_response_header() {
        let response = test_app()
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header("X-Api-Version", API_VERSION)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let version_header = response.headers().get("x-api-version");
        assert!(version_header.is_some());
        assert_eq!(version_header.unwrap().to_str().unwrap(), API_VERSION);
    }
}
