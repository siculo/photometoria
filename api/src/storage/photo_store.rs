//! Photo storage abstraction layer
//!
//! This module provides trait-based abstraction for photo persistence,
//! allowing multiple implementations (in-memory, database, etc.) without
//! changing business logic.

use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

use crate::models::Photo;

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur during photo storage operations.
#[derive(Debug, Error)]
pub enum PhotoStoreError {
    /// The requested photo was not found.
    #[error("Photo not found: {0}")]
    NotFound(Uuid),

    /// A photo with the same ID already exists.
    #[error("Photo already exists: {0}")]
    AlreadyExists(Uuid),

    /// A generic storage error occurred.
    #[error("Storage error: {0}")]
    #[allow(dead_code)]
    StorageError(String),
}

/// Result type alias for photo storage operations.
pub type PhotoStoreResult<T> = Result<T, PhotoStoreError>;

// ============================================================================
// PhotoStore Trait
// ============================================================================

/// Trait defining the interface for photo persistence.
///
/// This trait provides an abstract interface for storing and retrieving photos.
/// Implementations can use different backends (in-memory, database, etc.).
///
/// ## Thread Safety
///
/// All implementations must be `Send + Sync` to support concurrent access
/// from multiple Tokio tasks without data races.
///
/// ## Relationship with Tasks
///
/// Photos belong to Tasks (1:N relationship). Each photo has a `task_id` field
/// that references its parent task. When a task is deleted, all associated
/// photos should be deleted as well.
#[async_trait]
pub trait PhotoStore: Send + Sync {
    /// Creates a new photo in the store.
    ///
    /// # Arguments
    /// * `photo` - The photo to create
    ///
    /// # Returns
    /// * `Ok(Photo)` - The created photo (same as input)
    /// * `Err(AlreadyExists)` - If a photo with the same ID already exists
    /// * `Err(StorageError)` - If a storage-level error occurs
    async fn create(&self, photo: Photo) -> PhotoStoreResult<Photo>;

    /// Retrieves a photo by its ID.
    ///
    /// # Arguments
    /// * `photo_id` - The unique identifier of the photo
    ///
    /// # Returns
    /// * `Ok(Some(Photo))` - The photo if found
    /// * `Ok(None)` - If no photo with the given ID exists
    /// * `Err(StorageError)` - If a storage-level error occurs
    async fn get(&self, photo_id: Uuid) -> PhotoStoreResult<Option<Photo>>;

    /// Lists all photos belonging to a specific task.
    ///
    /// # Arguments
    /// * `task_id` - The unique identifier of the parent task
    ///
    /// # Returns
    /// * `Ok(Vec<Photo>)` - Vector of photos (may be empty)
    /// * `Err(StorageError)` - If a storage-level error occurs
    ///
    /// # Note
    /// Photos are sorted by `uploaded_at` (oldest first) for consistent ordering.
    async fn list_by_task(&self, task_id: Uuid) -> PhotoStoreResult<Vec<Photo>>;

    /// Deletes a photo from the store.
    ///
    /// # Arguments
    /// * `photo_id` - The unique identifier of the photo to delete
    ///
    /// # Returns
    /// * `Ok(())` - Photo successfully deleted
    /// * `Err(NotFound)` - If no photo with the given ID exists
    /// * `Err(StorageError)` - If a storage-level error occurs
    async fn delete(&self, photo_id: Uuid) -> PhotoStoreResult<()>;

    /// Deletes all photos belonging to a specific task.
    ///
    /// # Arguments
    /// * `task_id` - The unique identifier of the parent task
    ///
    /// # Returns
    /// * `Ok(usize)` - Number of photos deleted
    /// * `Err(StorageError)` - If a storage-level error occurs
    ///
    /// # Note
    /// This is used when deleting a task to clean up all associated photos.
    async fn delete_by_task(&self, task_id: Uuid) -> PhotoStoreResult<usize>;

    /// Returns the count of photos belonging to a specific task.
    ///
    /// # Arguments
    /// * `task_id` - The unique identifier of the parent task
    ///
    /// # Returns
    /// * `Ok(usize)` - The count of photos
    /// * `Err(StorageError)` - If a storage-level error occurs
    async fn count_by_task(&self, task_id: Uuid) -> PhotoStoreResult<usize>;

    /// Returns the total size in bytes of all photos belonging to a specific task.
    ///
    /// # Arguments
    /// * `task_id` - The unique identifier of the parent task
    ///
    /// # Returns
    /// * `Ok(u64)` - Total size in bytes
    /// * `Err(StorageError)` - If a storage-level error occurs
    ///
    /// # Note
    /// This is used for TaskSummary.storage_used_bytes.
    async fn total_size_by_task(&self, task_id: Uuid) -> PhotoStoreResult<u64>;

    /// Returns the total size in bytes of all photos across all tasks.
    ///
    /// # Returns
    /// * `Ok(u64)` - Total size in bytes
    /// * `Err(StorageError)` - If a storage-level error occurs
    async fn total_size(&self) -> PhotoStoreResult<u64>;

    /// Checks if a photo exists.
    ///
    /// # Arguments
    /// * `photo_id` - The unique identifier to check
    ///
    /// # Returns
    /// * `Ok(true)` - Photo exists
    /// * `Ok(false)` - Photo does not exist
    /// * `Err(StorageError)` - If a storage-level error occurs
    #[allow(dead_code)]
    async fn exists(&self, photo_id: Uuid) -> PhotoStoreResult<bool>;

    // ========================================================================
    // Binary Data Operations
    // ========================================================================

    /// Saves the raw image data for a photo.
    ///
    /// # Arguments
    /// * `photo_id` - The unique identifier of the photo
    /// * `data` - The raw image bytes to store
    ///
    /// # Returns
    /// * `Ok(())` - Data saved successfully
    /// * `Err(NotFound)` - If the photo metadata doesn't exist
    /// * `Err(StorageError)` - If a storage-level error occurs
    ///
    /// # Note
    /// The photo metadata must be created first using `create()`.
    async fn save_data(&self, photo_id: Uuid, data: &[u8]) -> PhotoStoreResult<()>;

    /// Loads the raw image data for a photo.
    ///
    /// # Arguments
    /// * `photo_id` - The unique identifier of the photo
    ///
    /// # Returns
    /// * `Ok(Vec<u8>)` - The raw image bytes
    /// * `Err(NotFound)` - If the photo or its data doesn't exist
    /// * `Err(StorageError)` - If a storage-level error occurs
    #[allow(dead_code)]
    async fn load_data(&self, photo_id: Uuid) -> PhotoStoreResult<Vec<u8>>;

    /// Deletes the raw image data for a photo.
    ///
    /// # Arguments
    /// * `photo_id` - The unique identifier of the photo
    ///
    /// # Returns
    /// * `Ok(())` - Data deleted successfully (or didn't exist)
    /// * `Err(StorageError)` - If a storage-level error occurs
    ///
    /// # Note
    /// This is called internally by `delete()` and `delete_by_task()`.
    /// It does not return an error if the data doesn't exist.
    async fn delete_data(&self, photo_id: Uuid) -> PhotoStoreResult<()>;
}
