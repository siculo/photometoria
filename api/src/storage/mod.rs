// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! Storage abstractions for data persistence
//!
//! This module provides trait-based abstractions for storing tasks, photos, and jobs.
//! The abstraction allows swapping implementations without changing business logic.
//!
//! ## Architecture
//!
//! - **Trait definitions** ([`ProjectStore`], [`PhotoStore`], [`JobStore`]) define the storage interface
//! - **Filesystem implementations** ([`FileSystemProjectStore`], [`FileSystemPhotoStore`], [`FileSystemJobStore`])
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
pub mod filesystem_project_store;
pub mod job_store;
pub mod photo_store;
pub mod project_store;
mod utils;

// Re-export commonly used types
pub use filesystem_job_store::FileSystemJobStore;
pub use filesystem_layout::FileSystemLayout;
pub use filesystem_photo_store::FileSystemPhotoStore;
pub use filesystem_project_store::FileSystemProjectStore;
pub use job_store::{JobStore, JobStoreError, JobStoreResult};
pub use photo_store::{PhotoStore, PhotoStoreError, PhotoStoreResult};
pub use project_store::{ProjectStore, ProjectStoreError, ProjectStoreResult};
