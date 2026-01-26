use axum::{Router, routing::get};

use crate::app_state::AppState;

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/version", get(version))
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
    use tower::ServiceExt;

    use crate::app_state::AppState;
    use crate::storage::{InMemoryTaskStore, TaskStore};

    #[tokio::test]
    async fn test_version_returns_package_version() {
        // Create a test state with an in-memory task store
        let task_store: Arc<dyn TaskStore> = Arc::new(InMemoryTaskStore::new());
        let state = AppState::new(task_store);

        let app = create_router(state);
        let request = Request::get("/version").body(Body::empty()).unwrap();
        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), 200);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body_str = String::from_utf8(body.to_vec()).unwrap();

        assert_eq!(body_str, env!("CARGO_PKG_VERSION"));
    }
}
