//! Storage abstractions for data persistence
//!
//! This module provides trait-based abstractions for storing tasks, photos, and jobs.
//! The abstraction allows swapping implementations without changing business logic.
//!
//! ## Architecture
//!
//! - **Trait definitions** ([`TaskStore`], [`PhotoStore`], [`JobStore`]) define the storage interface
//! - **Filesystem implementations** ([`FileSystemTaskStore`], [`FileSystemPhotoStore`], [`FileSystemJobStore`])
//!   provide persistent storage backed by JSON files and binary data
//! - **Filesystem layout** ([`FileSystemLayout`]) centralizes path generation and directory structure
//!
//! ## Directory Structure
//!
//! All filesystem implementations share a common directory layout defined by [`FileSystemLayout`].
//! See its documentation for details on the complete directory structure.

pub mod filesystem_job_store;
pub mod filesystem_layout;
pub mod filesystem_photo_store;
pub mod filesystem_task_store;
pub mod job_store;
pub mod photo_store;
pub mod task_store;

// Re-export commonly used types
pub use filesystem_job_store::FileSystemJobStore;
pub use filesystem_layout::FileSystemLayout;
pub use filesystem_photo_store::FileSystemPhotoStore;
pub use filesystem_task_store::FileSystemTaskStore;
pub use job_store::{JobStore, JobStoreError, JobStoreResult};
pub use photo_store::{PhotoStore, PhotoStoreError, PhotoStoreResult};
pub use task_store::{TaskStore, TaskStoreError, TaskStoreResult};
