// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! Storage abstractions for data persistence
//!
//! This module provides trait-based abstractions for storing projects, photos, and activities.
//! The abstraction allows swapping implementations without changing business logic.
//!
//! ## Architecture
//!
//! - **Trait definitions** ([`ProjectStore`], [`PhotoStore`], [`ActivityStore`]) define the storage interface
//! - **Filesystem implementations** ([`FileSystemProjectStore`], [`FileSystemPhotoStore`], [`FileSystemActivityStore`])
//!   provide persistent storage backed by JSON files and binary data
//! - **Filesystem layout** ([`FileSystemLayout`]) centralizes path generation and directory structure
//!
//! ## Directory Structure
//!
//! All filesystem implementations share a common directory layout defined by [`FileSystemLayout`].
//! See its documentation for details on the complete directory structure.

pub mod activity_store;
pub mod filesystem_activity_store;
pub mod filesystem_layout;
pub mod filesystem_photo_store;
pub mod filesystem_project_store;
pub mod photo_store;
pub mod project_store;
mod utils;

// Re-export commonly used types
pub use activity_store::{ActivityStore, ActivityStoreError, ActivityStoreResult};
pub use filesystem_activity_store::FileSystemActivityStore;
pub use filesystem_layout::FileSystemLayout;
pub use filesystem_photo_store::FileSystemPhotoStore;
pub use filesystem_project_store::FileSystemProjectStore;
pub use photo_store::{PhotoStore, PhotoStoreError, PhotoStoreResult};
pub use project_store::{ProjectStore, ProjectStoreError, ProjectStoreResult};
