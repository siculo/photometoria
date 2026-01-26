//! Storage abstractions for data persistence
//!
//! This module provides trait-based abstractions for storing tasks, photos, and jobs.
//! The abstraction allows swapping implementations without changing business logic.

pub mod in_memory_task_store;
pub mod task_store;

// Re-export commonly used types
pub use task_store::{TaskStore, TaskStoreError, TaskStoreResult};
