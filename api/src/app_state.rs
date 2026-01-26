use std::sync::Arc;

use crate::storage::TaskStore;

/// Application state shared across all request handlers.
///
/// This struct contains all the dependencies that handlers need to access,
/// such as storage backends and configuration. It is designed to be cloned
/// cheaply (using Arc internally) and passed to each request handler.
#[derive(Clone)]
pub struct AppState {
    /// Thread-safe reference to the task storage backend.
    pub task_store: Arc<dyn TaskStore>,
}

impl AppState {
    /// Creates a new AppState instance with the given task store.
    ///
    /// # Arguments
    ///
    /// * `task_store` - An Arc-wrapped implementation of TaskStore
    ///
    /// # Example
    ///
    /// ```ignore
    /// let task_store = Arc::new(InMemoryTaskStore::new());
    /// let state = AppState::new(task_store);
    /// ```
    pub fn new(task_store: Arc<dyn TaskStore>) -> Self {
        Self { task_store }
    }
}
