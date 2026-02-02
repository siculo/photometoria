use std::path::PathBuf;
use std::sync::Arc;

use tokio::signal;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use crate::app_state::AppState;
use crate::config::Config;
use crate::storage::{FileSystemPhotoStore, FileSystemTaskStore, PhotoStore, TaskStore};

/// Initializes the application state with all required dependencies.
///
/// Creates the task store and wraps it in the application state struct
/// that will be shared across all request handlers.
/// Loads existing data from the filesystem.
///
/// # Errors
///
/// Returns an error if the storage path cannot be created or is not writable.
pub async fn init_app_state(config: Config) -> Result<AppState, String> {
    let storage_path = PathBuf::from(&config.storage.path);
    tracing::info!("Using storage path: {:?}", storage_path);

    // Ensure storage directory exists and is writable
    if let Err(e) = tokio::fs::create_dir_all(&storage_path).await {
        return Err(format!(
            "Failed to create storage directory {:?}: {}",
            storage_path, e
        ));
    }

    // Verify the directory is writable by creating a test file
    let test_file = storage_path.join(".write_test");
    if let Err(e) = tokio::fs::write(&test_file, b"test").await {
        return Err(format!(
            "Storage directory {:?} is not writable: {}",
            storage_path, e
        ));
    }
    let _ = tokio::fs::remove_file(&test_file).await;

    let task_store: Arc<dyn TaskStore> = Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
    tracing::info!("Initialized filesystem task store");

    let photo_store: Arc<dyn PhotoStore> = Arc::new(FileSystemPhotoStore::new(storage_path).await);
    tracing::info!("Initialized filesystem photo store");

    Ok(AppState::new(config, task_store, photo_store))
}

/// Initializes the tracing subscriber with environment-aware defaults.
///
/// In debug builds, the default log level is set to `debug` for the application
/// and `info` for tower_http. In release builds, both default to `info`.
/// The log level can be overridden via the `RUST_LOG` environment variable.
pub fn init_tracing() {
    #[cfg(debug_assertions)]
    let default_log_level = "photometoria=debug,tower_http=info";

    #[cfg(not(debug_assertions))]
    let default_log_level = "photometoria=info,tower_http=info";

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| default_log_level.into()))
        .with(tracing_subscriber::fmt::layer())
        .init();
}

/// Handles graceful shutdown on CTRL+C (SIGINT) or SIGTERM.
///
/// This function waits for either a CTRL+C signal or a SIGTERM signal (Unix only),
/// and logs a shutdown message when received. This allows the server to complete
/// in-flight requests before terminating.
pub async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("Shutdown signal received, stopping server gracefully...");
}
