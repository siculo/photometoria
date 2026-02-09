//! Storage abstractions for data persistence
//!
//! This module provides trait-based abstractions for storing tasks, photos, and jobs.
//! The abstraction allows swapping implementations without changing business logic.

pub mod filesystem_job_store;
pub mod filesystem_photo_store;
pub mod filesystem_task_store;
pub mod job_store;
pub mod photo_store;
pub mod task_store;

// Re-export commonly used types
pub use filesystem_job_store::FileSystemJobStore;
pub use filesystem_photo_store::FileSystemPhotoStore;
pub use filesystem_task_store::FileSystemTaskStore;
pub use job_store::{JobStore, JobStoreError, JobStoreResult};
pub use photo_store::{PhotoStore, PhotoStoreError, PhotoStoreResult};
pub use task_store::{TaskStore, TaskStoreError, TaskStoreResult};
