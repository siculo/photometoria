// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

use crate::config::Config;
use crate::services::ai::ProviderRegistry;
use crate::services::worker::WorkerPool;
use crate::storage::{JobStore, PhotoStore, ProjectStore};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Application state shared across all request handlers.
///
/// This struct contains all the dependencies that handlers need to access,
/// such as storage backends and configuration. It is designed to be cloned
/// cheaply (using Arc internally) and passed to each request handler.
#[derive(Clone)]
pub struct AppState {
    /// Application configuration
    pub config: Config,
    /// Thread-safe reference to the project storage backend.
    pub project_store: Arc<dyn ProjectStore>,
    /// Thread-safe reference to the photo storage backend.
    pub photo_store: Arc<dyn PhotoStore>,
    /// Thread-safe reference to the job storage backend.
    pub job_store: Arc<dyn JobStore>,
    /// Registry of AI providers for image analysis.
    pub ai_providers: Arc<ProviderRegistry>,
    /// Worker pool for background job processing.
    pub worker_pool: Arc<Mutex<WorkerPool>>,
}

impl AppState {
    /// Creates a new AppState instance with all required dependencies.
    ///
    /// # Arguments
    ///
    /// * `config` - Current server configuration
    /// * `project_store` - An Arc-wrapped implementation of ProjectStore
    /// * `photo_store` - An Arc-wrapped implementation of PhotoStore
    /// * `ai_providers` - Registry of AI providers for image analysis
    pub fn new(
        config: Config,
        project_store: Arc<dyn ProjectStore>,
        photo_store: Arc<dyn PhotoStore>,
        job_store: Arc<dyn JobStore>,
        ai_providers: Arc<ProviderRegistry>,
        worker_pool: Arc<Mutex<WorkerPool>>,
    ) -> Self {
        Self {
            config,
            project_store,
            photo_store,
            ai_providers,
            job_store,
            worker_pool,
        }
    }
}
