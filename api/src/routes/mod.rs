use axum::{routing::get, Router};

pub fn create_router() -> Router {
    Router::new().route("/ping", get(ping))
}

async fn ping() -> &'static str {
    "PONG"
}
