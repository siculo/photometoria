mod app_state;
mod config;
mod handlers;
mod models;
mod routes;
mod services;
mod storage;

use std::sync::Arc;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use app_state::AppState;
use storage::{InMemoryTaskStore, TaskStore};

#[tokio::main]
async fn main() {
    // Initialize tracing subscriber with environment-aware defaults
    #[cfg(debug_assertions)]
    let default_log_level = "photometoria_rest_api=debug,tower_http=info";

    #[cfg(not(debug_assertions))]
    let default_log_level = "photometoria_rest_api=info,tower_http=info";

    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| default_log_level.into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting Photometoria REST API...");

    // Initialize the in-memory task store
    let task_store: Arc<dyn TaskStore> = Arc::new(InMemoryTaskStore::new());
    tracing::info!("Initialized in-memory task store");

    // Create application state
    let state = AppState::new(task_store);

    // Create router with state
    let app = routes::create_router(state);

    tracing::info!("Server listening on http://0.0.0.0:3000");

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}
