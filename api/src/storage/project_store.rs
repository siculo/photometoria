// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! Project storage abstraction layer
//!
//! This module provides trait-based abstraction for project persistence,
//! allowing multiple implementations (in-memory, database, etc.) without
//! changing business logic.

use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

use crate::models::Project;

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur during project storage operations.
#[derive(Debug, Error)]
pub enum ProjectStoreError {
    /// The requested project was not found.
    #[error("Project not found: {0}")]
    NotFound(Uuid),

    /// A project with the same ID already exists.
    #[error("Project already exists: {0}")]
    AlreadyExists(Uuid),

    /// A generic storage error occurred.
    #[error("Storage error: {0}")]
    #[allow(dead_code)]
    StorageError(String),
}

/// Result type alias for project storage operations.
pub type ProjectStoreResult<T> = Result<T, ProjectStoreError>;

// ============================================================================
// ProjectStore Trait
// ============================================================================

/// Trait defining the interface for project persistence.
///
/// This trait provides an abstract interface for storing and retrieving projects.
/// Implementations can use different backends (in-memory, database, etc.).
///
/// ## Thread Safety
///
/// All implementations must be `Send + Sync` to support concurrent access
/// from multiple Tokio tasks without data races.
///
/// ## Example Implementation
///
/// ```ignore
/// use async_trait::async_trait;
/// use std::sync::Arc;
/// use dashmap::DashMap;
///
/// pub struct InMemoryProjectStore {
///     projects: Arc<DashMap<String, Project>>,
/// }
///
/// #[async_trait]
/// impl ProjectStore for InMemoryProjectStore {
///     async fn create(&self, project: Project) -> ProjectStoreResult<Project> {
///         // Implementation here
///     }
///     // ... other methods
/// }
/// ```
#[async_trait]
pub trait ProjectStore: Send + Sync {
    /// Creates a new project in the store.
    ///
    /// # Arguments
    /// * `project` - The project to create
    ///
    /// # Returns
    /// * `Ok(Project)` - The created project (same as input)
    /// * `Err(AlreadyExists)` - If a project with the same ID already exists
    /// * `Err(StorageError)` - If a storage-level error occurs
    async fn create(&self, project: Project) -> ProjectStoreResult<Project>;

    /// Retrieves a project by its ID.
    ///
    /// # Arguments
    /// * `project_id` - The unique identifier of the project
    ///
    /// # Returns
    /// * `Ok(Some(Project))` - The project if found
    /// * `Ok(None)` - If no project with the given ID exists
    /// * `Err(StorageError)` - If a storage-level error occurs
    async fn get(&self, project_id: Uuid) -> ProjectStoreResult<Option<Project>>;

    /// Lists all projects in the store.
    ///
    /// # Returns
    /// * `Ok(Vec<Project>)` - Vector of all projects (may be empty)
    /// * `Err(StorageError)` - If a storage-level error occurs
    ///
    /// # Note
    /// The order of projects in the returned vector is implementation-defined.
    /// For consistent ordering, implementations should sort by `created_at`.
    async fn list(&self) -> ProjectStoreResult<Vec<Project>>;

    /// Updates an existing project.
    ///
    /// # Arguments
    /// * `project` - The project with updated fields (project_id must match existing project)
    ///
    /// # Returns
    /// * `Ok(Project)` - The updated project
    /// * `Err(NotFound)` - If no project with the given ID exists
    /// * `Err(StorageError)` - If a storage-level error occurs
    ///
    /// # Note
    /// This performs a full replacement of the project. Only the `context` field
    /// is typically updated in practice, but the entire project is stored.
    async fn update(&self, project: Project) -> ProjectStoreResult<Project>;

    /// Deletes a project from the store.
    ///
    /// # Arguments
    /// * `project_id` - The unique identifier of the project to delete
    ///
    /// # Returns
    /// * `Ok(())` - Project successfully deleted
    /// * `Err(NotFound)` - If no project with the given ID exists
    /// * `Err(StorageError)` - If a storage-level error occurs
    async fn delete(&self, project_id: Uuid) -> ProjectStoreResult<()>;

    /// Checks if a project exists.
    ///
    /// # Arguments
    /// * `project_id` - The unique identifier to check
    ///
    /// # Returns
    /// * `Ok(true)` - Project exists
    /// * `Ok(false)` - Project does not exist
    /// * `Err(StorageError)` - If a storage-level error occurs
    ///
    /// # Note
    /// This is more efficient than calling `get()` when you only need to
    /// check existence without retrieving the project data.
    #[allow(dead_code)]
    async fn exists(&self, project_id: Uuid) -> ProjectStoreResult<bool>;

    /// Returns the total number of projects in the store.
    ///
    /// # Returns
    /// * `Ok(usize)` - The count of projects
    /// * `Err(StorageError)` - If a storage-level error occurs
    #[allow(dead_code)]
    async fn count(&self) -> ProjectStoreResult<usize>;

    /// Lists all projects belonging to a specific catalog.
    ///
    /// # Arguments
    /// * `catalog_id` - The catalog identifier to filter by
    ///
    /// # Returns
    /// * `Ok(Vec<Project>)` - Vector of projects in the catalog (may be empty)
    /// * `Err(StorageError)` - If a storage-level error occurs
    ///
    /// # Note
    /// The order of projects in the returned vector is implementation-defined.
    /// For consistent ordering, implementations should sort by `created_at`.
    async fn list_by_catalog(&self, catalog_id: Uuid) -> ProjectStoreResult<Vec<Project>>;

    /// Finds a project by its name within a specific catalog.
    ///
    /// # Arguments
    /// * `catalog_id` - The catalog to search within
    /// * `name` - The name to search for (case-sensitive)
    ///
    /// # Returns
    /// * `Ok(Some(Project))` - The project if found
    /// * `Ok(None)` - If no project with the given name exists in the catalog
    /// * `Err(StorageError)` - If a storage-level error occurs
    async fn find_by_name(
        &self,
        catalog_id: Uuid,
        name: &str,
    ) -> ProjectStoreResult<Option<Project>>;
}
