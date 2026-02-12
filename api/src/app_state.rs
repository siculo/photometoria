use crate::config::Config;
use crate::services::ai::ProviderRegistry;
use crate::services::worker::WorkerPool;
use crate::storage::{JobStore, PhotoStore, TaskStore};
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
    /// Thread-safe reference to the task storage backend.
    pub task_store: Arc<dyn TaskStore>,
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
    /// * `task_store` - An Arc-wrapped implementation of TaskStore
    /// * `photo_store` - An Arc-wrapped implementation of PhotoStore
    /// * `ai_providers` - Registry of AI providers for image analysis
    ///
    /// # Example
    ///
    /// ```ignore
    /// let config = Config::default();
    /// let storage_path = PathBuf::from(&config.storage.path);
    /// let task_store = Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
    /// let photo_store = Arc::new(FileSystemPhotoStore::new(storage_path).await);
    /// let ai_providers = Arc::new(ProviderRegistry::from_config(&config.ai)?);
    /// let state = AppState::new(config, task_store, photo_store, ai_providers);
    /// ```
    pub fn new(
        config: Config,
        task_store: Arc<dyn TaskStore>,
        photo_store: Arc<dyn PhotoStore>,
        job_store: Arc<dyn JobStore>,
        ai_providers: Arc<ProviderRegistry>,
        worker_pool: Arc<Mutex<WorkerPool>>,
    ) -> Self {
        Self {
            config,
            task_store,
            photo_store,
            ai_providers,
            job_store,
            worker_pool,
        }
    }
}
