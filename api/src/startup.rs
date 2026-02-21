// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

use std::path::PathBuf;
use std::sync::Arc;

use tokio::signal;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use crate::app_state::AppState;
use crate::config::Config;
use crate::services::ai::ProviderRegistry;
use crate::services::worker::WorkerPool;
use crate::storage::{
    FileSystemJobStore, FileSystemPhotoStore, FileSystemTaskStore, JobStore, PhotoStore, TaskStore,
};
use tokio::sync::Mutex;

/// Initializes the application state with all required dependencies.
///
/// Creates the storage backends and AI provider registry, then wraps them
/// in the application state struct that will be shared across all request handlers.
/// Loads existing data from the filesystem.
///
/// # Errors
///
/// Returns an error if:
/// - The storage path cannot be created or is not writable
/// - AI provider configuration is invalid (e.g., invalid default provider)
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

    let task_store: Arc<dyn TaskStore> =
        Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
    tracing::info!("Initialized filesystem task store");

    let photo_store: Arc<dyn PhotoStore> =
        Arc::new(FileSystemPhotoStore::new(storage_path.clone()).await);
    tracing::info!("Initialized filesystem photo store");

    let job_store: Arc<dyn JobStore> = Arc::new(FileSystemJobStore::new(storage_path).await);
    tracing::info!("Initialized filesystem job store");

    // Initialize AI provider registry
    let ai_providers = Arc::new(
        ProviderRegistry::from_config(&config.ai)
            .map_err(|e| format!("Failed to initialize AI providers: {}", e))?,
    );
    if ai_providers.is_empty() {
        tracing::warn!("No AI providers configured");
    } else {
        tracing::info!(
            "Initialized {} AI provider{}",
            ai_providers.len(),
            if ai_providers.len() == 1 { "" } else { "s" }
        );
        match ai_providers.default_provider_name() {
            Some(name) => tracing::info!("Default provider is '{}'", name),
            None => tracing::info!("No default provider name"),
        };
    }

    // Initialize and start the worker pool
    let worker_pool = WorkerPool::new(
        &config,
        job_store.clone(),
        photo_store.clone(),
        task_store.clone(),
        ai_providers.clone(),
    );
    let worker_pool = Arc::new(Mutex::new(worker_pool));
    {
        let mut pool = worker_pool.lock().await;
        pool.start().await;
    }

    Ok(AppState::new(
        config,
        task_store,
        photo_store,
        job_store,
        ai_providers,
        worker_pool,
    ))
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
