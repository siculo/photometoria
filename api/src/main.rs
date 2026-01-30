mod app_state;
mod config;
mod handlers;
mod models;
mod routes;
mod services;
mod startup;
mod storage;

use startup::{init_app_state, init_tracing, shutdown_signal};

#[tokio::main]
async fn main() {
    init_tracing();

    tracing::info!("Starting Photometoria REST API...");

    let state = init_app_state();
    let app = routes::create_router(state);

    let addr = "0.0.0.0:3000";
    tracing::info!("Server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind to address");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("Server error");
}
