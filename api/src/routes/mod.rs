// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

use crate::api_version::{API_VERSION, api_version_middleware};
use crate::app_state::AppState;
use crate::handlers::catalogs::list_catalogs;
use crate::handlers::health::health;
use crate::handlers::info::info;
use crate::handlers::jobs::{
    cancel_job, create_job, delete_job, get_job, get_job_results, list_jobs, list_task_jobs,
    retry_job,
};
use crate::handlers::photos::{delete_photo, get_photo, task_photos};
use crate::handlers::project::{
    create_project, delete_project, get_project, list_projects, update_project,
};
use crate::handlers::providers::{list_providers, provider_details};
use crate::handlers::upload_photos::upload_photos;
use axum::{
    Router,
    extract::DefaultBodyLimit,
    middleware,
    routing::{get, post},
};
use tower_http::trace::TraceLayer;

pub fn create_router(state: AppState) -> Router {
    // Calculate max body size for uploads: max_photo_size * max_photos + overhead
    let max_upload_size = state.config.upload.max_photo_size.0 as usize
        * state.config.upload.max_photos_per_request
        + 1024 * 1024; // 1MB overhead for multipart boundaries

    Router::new()
        .route("/api/info", get(info))
        .route("/api/catalogs", get(list_catalogs))
        .route("/api/providers", get(list_providers))
        .route("/api/providers/{provider_name}", get(provider_details))
        .route(
            "/api/catalogs/{catalog_id}/tasks",
            post(create_project).get(list_projects),
        )
        .route(
            "/api/tasks/{task_id}",
            get(get_project)
                .patch(update_project)
                .delete(delete_project),
        )
        .route(
            "/api/tasks/{task_id}/photos",
            get(task_photos)
                .post(upload_photos)
                .layer(DefaultBodyLimit::max(max_upload_size)),
        )
        .route(
            "/api/photos/{photo_id}",
            get(get_photo).delete(delete_photo),
        )
        .route(
            "/api/tasks/{task_id}/jobs",
            get(list_task_jobs).post(create_job),
        )
        .route("/api/jobs", get(list_jobs))
        .route("/api/jobs/{job_id}", get(get_job).delete(delete_job))
        .route("/api/jobs/{job_id}/results", get(get_job_results))
        .route("/api/jobs/{job_id}/cancel", post(cancel_job))
        .route("/api/jobs/{job_id}/retry", post(retry_job))
        .layer(middleware::from_fn(api_version_middleware))
        // Registered after the version middleware: liveness probe is exempt from
        // API version negotiation.
        .route("/health", get(health))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::app_error::ErrorResponse;
    use crate::handlers::test_utils::fixtures::create_test_state;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_invalid_uuid_in_path_returns_400() {
        let ts = create_test_state().await;
        let app = create_router(ts.state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/tasks/not-a-uuid")
                    .header("X-Api-Version", API_VERSION)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let error: ErrorResponse = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(error.error, "invalid_uuid");
        assert_eq!(error.message, "Invalid UUID format in path parameter");
    }

    #[tokio::test]
    async fn test_health_exempt_from_version_middleware() {
        let ts = create_test_state().await;
        let app = create_router(ts.state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
