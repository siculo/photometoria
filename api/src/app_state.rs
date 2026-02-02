use std::sync::Arc;
use crate::config::Config;
use crate::storage::{PhotoStore, TaskStore};

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
}

impl AppState {
    /// Creates a new AppState instance with the given task store.
    ///
    /// # Arguments
    ///
    /// * `config` - Current Server configuration
    /// * `task_store` - An Arc-wrapped implementation of TaskStore
    ///
    /// # Example
    ///
    /// ```ignore
    /// let config = Config::default();
    /// let storage_path = PathBuf::from(&config.storage.path);
    /// let task_store = Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
    /// let photo_store = Arc::new(FileSystemPhotoStore::new(storage_path).await);
    /// let state = AppState::new(config, task_store, photo_store);
    /// ```
    pub fn new(config: Config, task_store: Arc<dyn TaskStore>, photo_store: Arc<dyn PhotoStore>) -> Self {
        Self { config, task_store, photo_store }
    }
}
