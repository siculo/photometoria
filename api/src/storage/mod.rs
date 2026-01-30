//! Storage abstractions for data persistence
//!
//! This module provides trait-based abstractions for storing tasks, photos, and jobs.
//! The abstraction allows swapping implementations without changing business logic.

pub mod in_memory_job_store;
pub mod in_memory_photo_store;
pub mod in_memory_task_store;
pub mod job_store;
pub mod photo_store;
pub mod task_store;

// Re-export commonly used types
pub use in_memory_job_store::InMemoryJobStore;
pub use in_memory_photo_store::InMemoryPhotoStore;
pub use in_memory_task_store::InMemoryTaskStore;
pub use job_store::{JobStore, JobStoreError, JobStoreResult};
pub use photo_store::{PhotoStore, PhotoStoreError, PhotoStoreResult};
pub use task_store::{TaskStore, TaskStoreError, TaskStoreResult};
