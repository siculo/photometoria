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

    let config = match load_config(&args.config) {
        Ok(config) => config,
        Err(e) => {
            tracing::error!("{}", e);
            std::process::exit(1);
        }
    };

    let addr = config.server_addr();
    tracing::info!("Server listening on http://{}", addr);

    let state = init_app_state(config);
    let app = routes::create_router(state);

    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(listener) => listener,
        Err(e) => {
            tracing::error!("Failed to bind to {}: {}", addr, e);
            std::process::exit(1);
        }
    };

    if let Err(e) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        tracing::error!("Server error: {}", e);
        std::process::exit(1);
    }
}
