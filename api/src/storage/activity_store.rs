// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! Activity storage abstraction layer
//!
//! This module provides trait-based abstraction for activity persistence,
//! allowing multiple implementations (in-memory, database, etc.) without
//! changing business logic.

use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

use crate::models::Activity;

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur during activity storage operations.
#[derive(Debug, Error)]
pub enum ActivityStoreError {
    /// The requested activity was not found.
    #[error("Activity not found: {0}")]
    NotFound(Uuid),

    /// An activity with the same ID already exists.
    #[error("Activity already exists: {0}")]
    AlreadyExists(Uuid),

    /// A generic storage error occurred.
    #[error("Storage error: {0}")]
    #[allow(dead_code)]
    StorageError(String),
}

/// Result type alias for activity storage operations.
pub type ActivityStoreResult<T> = Result<T, ActivityStoreError>;

// ============================================================================
// ActivityStore Trait
// ============================================================================

/// Trait defining the interface for activity persistence.
///
/// This trait provides an abstract interface for storing and retrieving activities.
/// Implementations can use different backends (in-memory, database, etc.).
///
/// ## Thread Safety
///
/// All implementations must be `Send + Sync` to support concurrent access
/// from multiple Tokio tasks without data races.
///
/// ## Relationship with Projects
///
/// Activities belong to Projects (1:N relationship). Each activity has a `project_id`
/// field that references its parent project. When a project is deleted, all associated
/// activities should be deleted as well.
///
/// ## Activity Lifecycle
///
/// Activities transition through states: Queued → Processing → Completed/Failed/Cancelled.
/// The `update` method is used to persist state changes.
#[async_trait]
pub trait ActivityStore: Send + Sync {
    /// Creates a new activity in the store.
    async fn create(&self, activity: Activity) -> ActivityStoreResult<Activity>;

    /// Retrieves an activity by its ID.
    async fn get(&self, activity_id: Uuid) -> ActivityStoreResult<Option<Activity>>;

    /// Lists all activities in the store, sorted by `created_at` (oldest first).
    async fn list(&self) -> ActivityStoreResult<Vec<Activity>>;

    /// Lists all activities belonging to a specific project, sorted by `created_at`.
    async fn list_by_task(&self, project_id: Uuid) -> ActivityStoreResult<Vec<Activity>>;

    /// Updates an existing activity.
    async fn update(&self, activity: Activity) -> ActivityStoreResult<Activity>;

    /// Deletes an activity from the store.
    async fn delete(&self, activity_id: Uuid) -> ActivityStoreResult<()>;

    /// Deletes all activities belonging to a specific project.
    ///
    /// Returns the number of activities deleted.
    async fn delete_by_task(&self, project_id: Uuid) -> ActivityStoreResult<usize>;

    /// Checks if an activity exists.
    #[allow(dead_code)]
    async fn exists(&self, activity_id: Uuid) -> ActivityStoreResult<bool>;

    /// Returns the count of activities belonging to a specific project.
    async fn count_by_task(&self, project_id: Uuid) -> ActivityStoreResult<usize>;
}
