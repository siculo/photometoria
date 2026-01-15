use axum::{routing::get, Router};

pub fn create_router() -> Router {
    Router::new().route("/version", get(version))
}

async fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_version_returns_package_version() {
        let app = create_router();
        let request = Request::get("/version").body(Body::empty()).unwrap();
        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), 200);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body_str = String::from_utf8(body.to_vec()).unwrap();

        assert_eq!(body_str, env!("CARGO_PKG_VERSION"));
    }
}
