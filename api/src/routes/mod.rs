use crate::app_state::AppState;
use crate::handlers::jobs::{
    cancel_job, create_job, delete_job, get_job, get_job_results, list_jobs, retry_job,
};
use crate::handlers::models::list_models;
use crate::handlers::photos::{delete_photo, get_photo, task_photos};
use crate::handlers::tasks::{create_task, delete_task, get_task, list_tasks, update_task};
use crate::handlers::upload_photos::upload_photos;
use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};

pub fn create_router(state: AppState) -> Router {
    // Calculate max body size for uploads: max_photo_size * max_photos + overhead
    let max_upload_size = state.config.upload.max_photo_size.0 as usize
        * state.config.upload.max_photos_per_request
        + 1024 * 1024; // 1MB overhead for multipart boundaries

    Router::new()
        .route("/version", get(version))
        .route("/api/tasks", post(create_task).get(list_tasks))
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
        .route("/api/tasks/{task_id}/jobs", post(create_job))
        .route("/api/jobs", get(list_jobs))
        .route("/api/jobs/{job_id}", get(get_job).delete(delete_job))
        .route("/api/jobs/{job_id}/results", get(get_job_results))
        .route("/api/jobs/{job_id}/cancel", post(cancel_job))
        .route("/api/jobs/{job_id}/retry", post(retry_job))
        .route("/api/models", get(list_models))
        .with_state(state)
}

async fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tempfile::TempDir;
    use tower::ServiceExt;

    use crate::app_state::AppState;
    use crate::config::Config;
    use crate::services::ai::ProviderRegistry;
    use crate::services::worker::WorkerPool;
    use crate::storage::{
        FileSystemJobStore, FileSystemPhotoStore, FileSystemTaskStore, PhotoStore, TaskStore,
    };
    use tokio::sync::Mutex;

    struct TestApp {
        router: Router,
        state: AppState,
        _temp_dir: TempDir,
    }

    async fn create_test_app() -> TestApp {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();
        let config = Config::default();
        let task_store: Arc<dyn TaskStore> =
            Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
        let photo_store: Arc<dyn PhotoStore> =
            Arc::new(FileSystemPhotoStore::new(storage_path.clone()).await);
        let job_store = Arc::new(FileSystemJobStore::new(storage_path).await);
        let ai_providers = Arc::new(ProviderRegistry::new());
        let worker_pool = Arc::new(Mutex::new(WorkerPool::new_inactive(job_store.clone())));
        let state = AppState::new(
            config,
            task_store,
            photo_store,
            job_store,
            ai_providers,
            worker_pool,
        );
        let router = create_router(state.clone());
        TestApp {
            router,
            state,
            _temp_dir: temp_dir,
        }
    }

    #[tokio::test]
    async fn test_version_returns_package_version() {
        let ta = create_test_app().await;
        let request = Request::get("/version").body(Body::empty()).unwrap();
        let response = ta.router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), 200);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body_str = String::from_utf8(body.to_vec()).unwrap();

        assert_eq!(body_str, env!("CARGO_PKG_VERSION"));
    }
}
