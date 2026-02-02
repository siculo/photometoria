//! In-memory implementation of PhotoStore using DashMap
//!
//! This module provides a thread-safe in-memory implementation of the PhotoStore trait
//! using DashMap for concurrent access without global locks.

use async_trait::async_trait;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use std::sync::Arc;
use tracing::{debug, info};

use crate::models::Photo;

use super::{PhotoStore, PhotoStoreError, PhotoStoreResult};

// ============================================================================
// InMemoryPhotoStore Implementation
// ============================================================================

/// In-memory implementation of PhotoStore using DashMap for thread-safe concurrent access.
///
/// This implementation stores photos in memory using DashMap, which provides excellent
/// concurrent read/write performance through internal sharding.
///
/// ## Characteristics
///
/// - **Thread-safe**: Supports concurrent access from multiple Tokio tasks
/// - **Lock-free reads**: Get operations don't acquire locks
/// - **Fine-grained locking**: Writes lock only the specific shard
/// - **No persistence**: Data is lost when the server restarts
/// - **Flat structure**: All photos in a single map, filtered by task_id when needed
///
/// ## Use Cases
///
/// - Development and testing
/// - Single-user scenarios
/// - Prototyping before adding database persistence
pub struct InMemoryPhotoStore {
    photos: Arc<DashMap<String, Photo>>,
}

impl InMemoryPhotoStore {
    /// Creates a new empty in-memory photo store.
    pub fn new() -> Self {
        Self {
            photos: Arc::new(DashMap::new()),
        }
    }
}

impl Default for InMemoryPhotoStore {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// PhotoStore Trait Implementation
// ============================================================================

#[async_trait]
impl PhotoStore for InMemoryPhotoStore {
    async fn create(&self, photo: Photo) -> PhotoStoreResult<Photo> {
        let photo_id = photo.photo_id.clone();

        debug!("Attempting to create photo: {}", photo_id);

        match self.photos.entry(photo_id.clone()) {
            Entry::Occupied(_) => Err(PhotoStoreError::AlreadyExists(photo_id)),
            Entry::Vacant(entry) => {
                entry.insert(photo.clone());
                info!(
                    "Photo created successfully: {} (task: {}, filename: '{}')",
                    photo_id, photo.task_id, photo.filename
                );
                Ok(photo)
            }
        }
    }

    async fn get(&self, photo_id: &str) -> PhotoStoreResult<Option<Photo>> {
        debug!("Retrieving photo: {}", photo_id);

        Ok(self.photos.get(photo_id).map(|entry| entry.value().clone()))
    }

    async fn list_by_task(&self, task_id: &str) -> PhotoStoreResult<Vec<Photo>> {
        debug!("Listing photos for task: {}", task_id);

        let mut photos: Vec<Photo> = self
            .photos
            .iter()
            .filter(|entry| entry.value().task_id == task_id)
            .map(|entry| entry.value().clone())
            .collect();

        // Sort by uploaded_at (oldest first) for deterministic output
        photos.sort_by_key(|photo| photo.uploaded_at);

        info!("Listed {} photos for task {}", photos.len(), task_id);
        Ok(photos)
    }

    async fn delete(&self, photo_id: &str) -> PhotoStoreResult<()> {
        debug!("Deleting photo: {}", photo_id);

        match self.photos.remove(photo_id) {
            Some((_, photo)) => {
                info!(
                    "Photo deleted successfully: {} (task: {}, filename: '{}')",
                    photo_id, photo.task_id, photo.filename
                );
                Ok(())
            }
            None => Err(PhotoStoreError::NotFound(photo_id.to_string())),
        }
    }

    async fn delete_by_task(&self, task_id: &str) -> PhotoStoreResult<usize> {
        debug!("Deleting all photos for task: {}", task_id);

        // Collect photo_ids to delete (can't delete while iterating)
        let photo_ids: Vec<String> = self
            .photos
            .iter()
            .filter(|entry| entry.value().task_id == task_id)
            .map(|entry| entry.key().clone())
            .collect();

        let count = photo_ids.len();

        for photo_id in photo_ids {
            self.photos.remove(&photo_id);
        }

        info!("Deleted {} photos for task {}", count, task_id);
        Ok(count)
    }

    async fn count_by_task(&self, task_id: &str) -> PhotoStoreResult<usize> {
        debug!("Counting photos for task: {}", task_id);

        let count = self
            .photos
            .iter()
            .filter(|entry| entry.value().task_id == task_id)
            .count();

        Ok(count)
    }

    async fn total_size_by_task(&self, task_id: &str) -> PhotoStoreResult<u64> {
        debug!("Calculating total size for task: {}", task_id);

        let total: u64 = self
            .photos
            .iter()
            .filter(|entry| entry.value().task_id == task_id)
            .map(|entry| entry.value().size_bytes)
            .sum();

        Ok(total)
    }

    async fn total_size(&self) -> PhotoStoreResult<u64> {
        debug!("Calculating total size");
        
        let total: u64 = self
            .photos
            .iter()
            .map(|entry| entry.value().size_bytes)
            .sum();

        Ok(total)
    }

    async fn exists(&self, photo_id: &str) -> PhotoStoreResult<bool> {
        debug!("Checking if photo exists: {}", photo_id);

        Ok(self.photos.contains_key(photo_id))
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to create a fresh store
    fn create_store() -> InMemoryPhotoStore {
        InMemoryPhotoStore::new()
    }

    // Helper function to create a test photo
    fn create_test_photo(task_id: &str, filename: &str, size_bytes: u64) -> Photo {
        Photo::new(task_id.to_string(), filename.to_string(), size_bytes)
    }

    #[tokio::test]
    async fn test_create_photo() {
        let store = create_store();
        let photo = create_test_photo("task_123", "test.jpg", 1_000_000);

        let result = store.create(photo.clone()).await;

        assert!(result.is_ok());
        let created = result.unwrap();
        assert_eq!(created.photo_id, photo.photo_id);
        assert_eq!(created.task_id, photo.task_id);
        assert_eq!(created.filename, photo.filename);

        // Verify it exists in the store
        let exists = store.exists(&photo.photo_id).await.unwrap();
        assert!(exists);
    }

    #[tokio::test]
    async fn test_create_duplicate_fails() {
        let store = create_store();
        let photo = create_test_photo("task_123", "test.jpg", 1_000_000);

        // First creation should succeed
        store.create(photo.clone()).await.unwrap();

        // Second creation with same ID should fail
        let result = store.create(photo.clone()).await;

        assert!(result.is_err());
        match result {
            Err(PhotoStoreError::AlreadyExists(id)) => {
                assert_eq!(id, photo.photo_id);
            }
            _ => panic!("Expected AlreadyExists error"),
        }
    }

    #[tokio::test]
    async fn test_get_existing_photo() {
        let store = create_store();
        let photo = create_test_photo("task_123", "vacation.jpg", 5_000_000);

        store.create(photo.clone()).await.unwrap();

        let result = store.get(&photo.photo_id).await.unwrap();

        assert!(result.is_some());
        let retrieved = result.unwrap();
        assert_eq!(retrieved.photo_id, photo.photo_id);
        assert_eq!(retrieved.task_id, photo.task_id);
        assert_eq!(retrieved.filename, photo.filename);
        assert_eq!(retrieved.size_bytes, photo.size_bytes);
    }

    #[tokio::test]
    async fn test_get_nonexistent_photo() {
        let store = create_store();

        let result = store.get("nonexistent_id").await.unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_list_by_task() {
        let store = create_store();

        // Create photos for two different tasks
        let photo1 = create_test_photo("task_A", "photo1.jpg", 1_000_000);
        let photo2 = create_test_photo("task_A", "photo2.jpg", 2_000_000);
        let photo3 = create_test_photo("task_B", "photo3.jpg", 3_000_000);

        store.create(photo1.clone()).await.unwrap();
        store.create(photo2.clone()).await.unwrap();
        store.create(photo3.clone()).await.unwrap();

        // List photos for task_A
        let photos_a = store.list_by_task("task_A").await.unwrap();
        assert_eq!(photos_a.len(), 2);
        assert!(photos_a.iter().all(|p| p.task_id == "task_A"));

        // List photos for task_B
        let photos_b = store.list_by_task("task_B").await.unwrap();
        assert_eq!(photos_b.len(), 1);
        assert_eq!(photos_b[0].task_id, "task_B");

        // List photos for nonexistent task
        let photos_c = store.list_by_task("task_C").await.unwrap();
        assert!(photos_c.is_empty());
    }

    #[tokio::test]
    async fn test_delete_photo() {
        let store = create_store();
        let photo = create_test_photo("task_123", "test.jpg", 1_000_000);

        store.create(photo.clone()).await.unwrap();

        // Delete the photo
        let result = store.delete(&photo.photo_id).await;

        assert!(result.is_ok());

        // Verify it no longer exists
        let retrieved = store.get(&photo.photo_id).await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_fails() {
        let store = create_store();

        let result = store.delete("nonexistent_id").await;

        assert!(result.is_err());
        match result {
            Err(PhotoStoreError::NotFound(id)) => {
                assert_eq!(id, "nonexistent_id");
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_delete_by_task() {
        let store = create_store();

        // Create photos for two different tasks
        let photo1 = create_test_photo("task_A", "photo1.jpg", 1_000_000);
        let photo2 = create_test_photo("task_A", "photo2.jpg", 2_000_000);
        let photo3 = create_test_photo("task_B", "photo3.jpg", 3_000_000);

        store.create(photo1).await.unwrap();
        store.create(photo2).await.unwrap();
        store.create(photo3.clone()).await.unwrap();

        // Delete all photos for task_A
        let deleted = store.delete_by_task("task_A").await.unwrap();
        assert_eq!(deleted, 2);

        // Verify task_A photos are gone
        let photos_a = store.list_by_task("task_A").await.unwrap();
        assert!(photos_a.is_empty());

        // Verify task_B photo still exists
        let photos_b = store.list_by_task("task_B").await.unwrap();
        assert_eq!(photos_b.len(), 1);
    }

    #[tokio::test]
    async fn test_count_by_task() {
        let store = create_store();

        // Empty task should have count 0
        let count = store.count_by_task("task_A").await.unwrap();
        assert_eq!(count, 0);

        // Add photos
        let photo1 = create_test_photo("task_A", "photo1.jpg", 1_000_000);
        let photo2 = create_test_photo("task_A", "photo2.jpg", 2_000_000);
        let photo3 = create_test_photo("task_B", "photo3.jpg", 3_000_000);

        store.create(photo1).await.unwrap();
        store.create(photo2).await.unwrap();
        store.create(photo3).await.unwrap();

        // Count for task_A
        let count = store.count_by_task("task_A").await.unwrap();
        assert_eq!(count, 2);

        // Count for task_B
        let count = store.count_by_task("task_B").await.unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_total_size_by_task() {
        let store = create_store();

        // Empty task should have size 0
        let size = store.total_size_by_task("task_A").await.unwrap();
        assert_eq!(size, 0);

        // Add photos with known sizes
        let photo1 = create_test_photo("task_A", "photo1.jpg", 1_000_000);
        let photo2 = create_test_photo("task_A", "photo2.jpg", 2_500_000);
        let photo3 = create_test_photo("task_B", "photo3.jpg", 3_000_000);

        store.create(photo1).await.unwrap();
        store.create(photo2).await.unwrap();
        store.create(photo3).await.unwrap();

        // Total size for task_A
        let size = store.total_size_by_task("task_A").await.unwrap();
        assert_eq!(size, 3_500_000);

        // Total size for task_B
        let size = store.total_size_by_task("task_B").await.unwrap();
        assert_eq!(size, 3_000_000);
    }

    #[tokio::test]
    async fn test_exists() {
        let store = create_store();
        let photo = create_test_photo("task_123", "test.jpg", 1_000_000);

        // Should not exist initially
        let exists = store.exists(&photo.photo_id).await.unwrap();
        assert!(!exists);

        // Create the photo
        store.create(photo.clone()).await.unwrap();

        // Should exist now
        let exists = store.exists(&photo.photo_id).await.unwrap();
        assert!(exists);

        // Random ID should not exist
        let exists = store.exists("random_id").await.unwrap();
        assert!(!exists);
    }
}
