mod app_state;
mod cli;
mod config;
mod handlers;
mod models;
mod routes;
mod services;
mod startup;
mod storage;

use cli::parse_args;
use config::load_config;
use startup::{init_app_state, init_tracing, shutdown_signal};

#[tokio::main]
async fn main() {
    let args = parse_args();

    init_tracing();

    tracing::info!("Starting Photometoria REST API...");

    let config = load_config(&args.config).expect("Failed to load configuration");

    let state = init_app_state();
    let app = routes::create_router(state);

    let addr = config.server_addr();
    tracing::info!("Server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind to address");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("Server error");
}
