mod cli;

use photometoria_rest_api::config::load_config;
use photometoria_rest_api::routes;
use photometoria_rest_api::startup::{init_app_state, init_tracing, shutdown_signal};

use cli::parse_args;

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

    let state = match init_app_state(config).await {
        Ok(state) => state,
        Err(e) => {
            tracing::error!("{}", e);
            std::process::exit(1);
        }
    };
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
