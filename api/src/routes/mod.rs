// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

use crate::app_state::AppState;
use crate::handlers::info::info;
use crate::handlers::jobs::{
    cancel_job, create_job, delete_job, get_job, get_job_results, list_jobs, list_task_jobs,
    retry_job,
};
use crate::handlers::photos::{delete_photo, get_photo, task_photos};
use crate::handlers::providers::{list_providers, provider_details};
use crate::handlers::tasks::{create_task, delete_task, get_task, list_tasks, update_task};
use crate::handlers::upload_photos::upload_photos;
use axum::{
    Router,
    extract::DefaultBodyLimit,
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
        .route("/api/providers", get(list_providers))
        .route("/api/providers/{provider_name}", get(provider_details))
        .route(
            "/api/catalogs/{catalog_id}/tasks",
            post(create_task).get(list_tasks),
        )
        .route(
            "/api/tasks/{task_id}",
            get(get_task).patch(update_task).delete(delete_task),
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
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
