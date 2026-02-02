//! In-memory implementation of PhotoStore using DashMap
//!
//! This module provides a thread-safe in-memory implementation of the PhotoStore trait
//! using DashMap for concurrent access without global locks.
//!
//! Photo binary data is stored on the filesystem, while metadata is kept in memory.

use async_trait::async_trait;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, error, info};

use crate::models::Photo;

use super::{PhotoStore, PhotoStoreError, PhotoStoreResult};

// ============================================================================
// InMemoryPhotoStore Implementation
// ============================================================================

/// In-memory implementation of PhotoStore using DashMap for thread-safe concurrent access.
///
/// This implementation stores photo metadata in memory using DashMap, while binary
/// image data is stored on the filesystem.
///
/// ## Characteristics
///
/// - **Thread-safe**: Supports concurrent access from multiple Tokio tasks
/// - **Lock-free reads**: Get operations don't acquire locks
/// - **Fine-grained locking**: Writes lock only the specific shard
/// - **Partial persistence**: Metadata lost on restart, but files remain on disk
/// - **Flat structure**: All photos in a single map, filtered by task_id when needed
///
/// ## File Storage
///
/// Binary data is stored at: `{storage_path}/tasks/{task_id}/{photo_id}`
///
/// ## Use Cases
///
/// - Development and testing
/// - Single-user scenarios
/// - Prototyping before adding database persistence
pub struct InMemoryPhotoStore {
    /// Photo metadata storage
    photos: Arc<DashMap<String, Photo>>,
    /// Base path for file storage
    storage_path: PathBuf,
}

impl InMemoryPhotoStore {
    /// Creates a new empty in-memory photo store.
    ///
    /// # Arguments
    /// * `storage_path` - Base path for storing photo files
    pub fn new(storage_path: PathBuf) -> Self {
        Self {
            photos: Arc::new(DashMap::new()),
            storage_path,
        }
    }

    /// Returns the file path for a photo's binary data.
    fn photo_path(&self, task_id: &str, photo_id: &str) -> PathBuf {
        self.storage_path.join("tasks").join(task_id).join(photo_id)
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
                // Also delete the file from disk
                let file_path = self.photo_path(&photo.task_id, photo_id);
                if file_path.exists() {
                    if let Err(e) = tokio::fs::remove_file(&file_path).await {
                        error!("Failed to remove photo file {:?}: {}", file_path, e);
                        // Log but don't fail - metadata is already removed
                    } else {
                        debug!("Removed photo file: {:?}", file_path);
                    }
                }
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

        // Remove metadata only - files are deleted by TaskStore::delete()
        // when it removes the entire task directory
        for photo_id in &photo_ids {
            self.photos.remove(photo_id);
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

    async fn save_data(&self, photo_id: &str, data: &[u8]) -> PhotoStoreResult<()> {
        debug!("Saving {} bytes for photo: {}", data.len(), photo_id);

        // Get photo metadata to find task_id
        let photo = self.photos.get(photo_id).ok_or_else(|| {
            PhotoStoreError::NotFound(photo_id.to_string())
        })?;
        let task_id = photo.task_id.clone();
        drop(photo); // Release the lock

        let file_path = self.photo_path(&task_id, photo_id);
        tokio::fs::write(&file_path, data).await.map_err(|e| {
            error!("Failed to write photo file {:?}: {}", file_path, e);
            PhotoStoreError::StorageError(format!("Failed to write file: {}", e))
        })?;

        info!("Saved {} bytes for photo: {} at {:?}", data.len(), photo_id, file_path);
        Ok(())
    }

    async fn load_data(&self, photo_id: &str) -> PhotoStoreResult<Vec<u8>> {
        debug!("Loading data for photo: {}", photo_id);

        // Get photo metadata to find task_id
        let photo = self.photos.get(photo_id).ok_or_else(|| {
            PhotoStoreError::NotFound(photo_id.to_string())
        })?;
        let task_id = photo.task_id.clone();
        drop(photo); // Release the lock

        let file_path = self.photo_path(&task_id, photo_id);
        let data = tokio::fs::read(&file_path).await.map_err(|e| {
            error!("Failed to read photo file {:?}: {}", file_path, e);
            PhotoStoreError::NotFound(photo_id.to_string())
        })?;

        debug!("Loaded {} bytes for photo: {}", data.len(), photo_id);
        Ok(data)
    }

    async fn delete_data(&self, photo_id: &str) -> PhotoStoreResult<()> {
        debug!("Deleting data for photo: {}", photo_id);

        // Get photo metadata to find task_id (if it exists)
        if let Some(photo) = self.photos.get(photo_id) {
            let task_id = photo.task_id.clone();
            drop(photo); // Release the lock

            let file_path = self.photo_path(&task_id, photo_id);
            if file_path.exists() {
                if let Err(e) = tokio::fs::remove_file(&file_path).await {
                    error!("Failed to remove photo file {:?}: {}", file_path, e);
                    // Don't fail - best effort deletion
                }
            }
        }
        Ok(())
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // Helper struct to keep temp dir alive during test
    struct TestStore {
        store: InMemoryPhotoStore,
        _temp_dir: TempDir,
    }

    // Helper function to create a fresh store with temp directory
    fn create_store() -> TestStore {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let store = InMemoryPhotoStore::new(temp_dir.path().to_path_buf());
        TestStore {
            store,
            _temp_dir: temp_dir,
        }
    }

    // Helper to create task directory (normally done by TaskStore)
    async fn create_task_dir(ts: &TestStore, task_id: &str) {
        let task_dir = ts.store.storage_path.join("tasks").join(task_id);
        tokio::fs::create_dir_all(&task_dir).await.expect("Failed to create task dir");
    }

    // Helper function to create a test photo
    fn create_test_photo(task_id: &str, filename: &str, size_bytes: u64) -> Photo {
        Photo::new(task_id.to_string(), filename.to_string(), size_bytes)
    }

    #[tokio::test]
    async fn test_create_photo() {
        let ts = create_store();
        let photo = create_test_photo("task_123", "test.jpg", 1_000_000);

        let result = ts.store.create(photo.clone()).await;

        assert!(result.is_ok());
        let created = result.unwrap();
        assert_eq!(created.photo_id, photo.photo_id);
        assert_eq!(created.task_id, photo.task_id);
        assert_eq!(created.filename, photo.filename);

        // Verify it exists in the store
        let exists = ts.store.exists(&photo.photo_id).await.unwrap();
        assert!(exists);
    }

    #[tokio::test]
    async fn test_create_duplicate_fails() {
        let ts = create_store();
        let photo = create_test_photo("task_123", "test.jpg", 1_000_000);

        // First creation should succeed
        ts.store.create(photo.clone()).await.unwrap();

        // Second creation with same ID should fail
        let result = ts.store.create(photo.clone()).await;

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
        let ts = create_store();
        let photo = create_test_photo("task_123", "vacation.jpg", 5_000_000);

        ts.store.create(photo.clone()).await.unwrap();

        let result = ts.store.get(&photo.photo_id).await.unwrap();

        assert!(result.is_some());
        let retrieved = result.unwrap();
        assert_eq!(retrieved.photo_id, photo.photo_id);
        assert_eq!(retrieved.task_id, photo.task_id);
        assert_eq!(retrieved.filename, photo.filename);
        assert_eq!(retrieved.size_bytes, photo.size_bytes);
    }

    #[tokio::test]
    async fn test_get_nonexistent_photo() {
        let ts = create_store();

        let result = ts.store.get("nonexistent_id").await.unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_list_by_task() {
        let ts = create_store();

        // Create photos for two different tasks
        let photo1 = create_test_photo("task_A", "photo1.jpg", 1_000_000);
        let photo2 = create_test_photo("task_A", "photo2.jpg", 2_000_000);
        let photo3 = create_test_photo("task_B", "photo3.jpg", 3_000_000);

        ts.store.create(photo1.clone()).await.unwrap();
        ts.store.create(photo2.clone()).await.unwrap();
        ts.store.create(photo3.clone()).await.unwrap();

        // List photos for task_A
        let photos_a = ts.store.list_by_task("task_A").await.unwrap();
        assert_eq!(photos_a.len(), 2);
        assert!(photos_a.iter().all(|p| p.task_id == "task_A"));

        // List photos for task_B
        let photos_b = ts.store.list_by_task("task_B").await.unwrap();
        assert_eq!(photos_b.len(), 1);
        assert_eq!(photos_b[0].task_id, "task_B");

        // List photos for nonexistent task
        let photos_c = ts.store.list_by_task("task_C").await.unwrap();
        assert!(photos_c.is_empty());
    }

    #[tokio::test]
    async fn test_delete_photo() {
        let ts = create_store();
        create_task_dir(&ts, "task_123").await;
        let photo = create_test_photo("task_123", "test.jpg", 1_000_000);

        ts.store.create(photo.clone()).await.unwrap();
        // Save data so we can verify it's deleted too
        ts.store.save_data(&photo.photo_id, &[1, 2, 3]).await.unwrap();
        let file_path = ts.store.photo_path("task_123", &photo.photo_id);
        assert!(file_path.exists());

        // Delete the photo
        let result = ts.store.delete(&photo.photo_id).await;

        assert!(result.is_ok());

        // Verify it no longer exists
        let retrieved = ts.store.get(&photo.photo_id).await.unwrap();
        assert!(retrieved.is_none());

        // Verify file was removed
        assert!(!file_path.exists());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_fails() {
        let ts = create_store();

        let result = ts.store.delete("nonexistent_id").await;

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
        let ts = create_store();

        // Create photos for two different tasks
        let photo1 = create_test_photo("task_A", "photo1.jpg", 1_000_000);
        let photo2 = create_test_photo("task_A", "photo2.jpg", 2_000_000);
        let photo3 = create_test_photo("task_B", "photo3.jpg", 3_000_000);

        ts.store.create(photo1).await.unwrap();
        ts.store.create(photo2).await.unwrap();
        ts.store.create(photo3.clone()).await.unwrap();

        // Delete all photos for task_A
        let deleted = ts.store.delete_by_task("task_A").await.unwrap();
        assert_eq!(deleted, 2);

        // Verify task_A photos are gone
        let photos_a = ts.store.list_by_task("task_A").await.unwrap();
        assert!(photos_a.is_empty());

        // Verify task_B photo still exists
        let photos_b = ts.store.list_by_task("task_B").await.unwrap();
        assert_eq!(photos_b.len(), 1);
    }

    #[tokio::test]
    async fn test_count_by_task() {
        let ts = create_store();

        // Empty task should have count 0
        let count = ts.store.count_by_task("task_A").await.unwrap();
        assert_eq!(count, 0);

        // Add photos
        let photo1 = create_test_photo("task_A", "photo1.jpg", 1_000_000);
        let photo2 = create_test_photo("task_A", "photo2.jpg", 2_000_000);
        let photo3 = create_test_photo("task_B", "photo3.jpg", 3_000_000);

        ts.store.create(photo1).await.unwrap();
        ts.store.create(photo2).await.unwrap();
        ts.store.create(photo3).await.unwrap();

        // Count for task_A
        let count = ts.store.count_by_task("task_A").await.unwrap();
        assert_eq!(count, 2);

        // Count for task_B
        let count = ts.store.count_by_task("task_B").await.unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_total_size_by_task() {
        let ts = create_store();

        // Empty task should have size 0
        let size = ts.store.total_size_by_task("task_A").await.unwrap();
        assert_eq!(size, 0);

        // Add photos with known sizes
        let photo1 = create_test_photo("task_A", "photo1.jpg", 1_000_000);
        let photo2 = create_test_photo("task_A", "photo2.jpg", 2_500_000);
        let photo3 = create_test_photo("task_B", "photo3.jpg", 3_000_000);

        ts.store.create(photo1).await.unwrap();
        ts.store.create(photo2).await.unwrap();
        ts.store.create(photo3).await.unwrap();

        // Total size for task_A
        let size = ts.store.total_size_by_task("task_A").await.unwrap();
        assert_eq!(size, 3_500_000);

        // Total size for task_B
        let size = ts.store.total_size_by_task("task_B").await.unwrap();
        assert_eq!(size, 3_000_000);
    }

    #[tokio::test]
    async fn test_exists() {
        let ts = create_store();
        let photo = create_test_photo("task_123", "test.jpg", 1_000_000);

        // Should not exist initially
        let exists = ts.store.exists(&photo.photo_id).await.unwrap();
        assert!(!exists);

        // Create the photo
        ts.store.create(photo.clone()).await.unwrap();

        // Should exist now
        let exists = ts.store.exists(&photo.photo_id).await.unwrap();
        assert!(exists);

        // Random ID should not exist
        let exists = ts.store.exists("random_id").await.unwrap();
        assert!(!exists);
    }

    // ========================================================================
    // Binary Data Tests
    // ========================================================================

    #[tokio::test]
    async fn test_save_and_load_data() {
        let ts = create_store();
        create_task_dir(&ts, "task_123").await;
        let photo = create_test_photo("task_123", "test.jpg", 1_000);

        // Create the photo first
        ts.store.create(photo.clone()).await.unwrap();

        // Save some data
        let test_data = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        ts.store.save_data(&photo.photo_id, &test_data).await.unwrap();

        // Verify file exists on disk
        let file_path = ts.store.photo_path("task_123", &photo.photo_id);
        assert!(file_path.exists());

        // Load the data back
        let loaded = ts.store.load_data(&photo.photo_id).await.unwrap();
        assert_eq!(loaded, test_data);
    }

    #[tokio::test]
    async fn test_save_data_requires_photo_metadata() {
        let ts = create_store();

        // Try to save data without creating photo metadata first
        let test_data = vec![0xFF, 0xD8, 0xFF, 0xE0];
        let result = ts.store.save_data("nonexistent", &test_data).await;

        assert!(result.is_err());
        match result {
            Err(PhotoStoreError::NotFound(id)) => assert_eq!(id, "nonexistent"),
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_load_nonexistent_data() {
        let ts = create_store();
        create_task_dir(&ts, "task_123").await;
        let photo = create_test_photo("task_123", "test.jpg", 1_000);

        // Create photo metadata but don't save data
        ts.store.create(photo.clone()).await.unwrap();

        // Try to load data (file doesn't exist)
        let result = ts.store.load_data(&photo.photo_id).await;

        assert!(result.is_err());
        match result {
            Err(PhotoStoreError::NotFound(_)) => {}
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_delete_also_removes_data() {
        let ts = create_store();
        create_task_dir(&ts, "task_123").await;
        let photo = create_test_photo("task_123", "test.jpg", 1_000);

        // Create photo and save data
        ts.store.create(photo.clone()).await.unwrap();
        let test_data = vec![0xFF, 0xD8, 0xFF, 0xE0];
        ts.store.save_data(&photo.photo_id, &test_data).await.unwrap();

        // Delete the photo
        ts.store.delete(&photo.photo_id).await.unwrap();

        // Metadata should be gone, so load_data will fail with NotFound
        let result = ts.store.load_data(&photo.photo_id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_delete_data_idempotent() {
        let ts = create_store();

        // Delete data that doesn't exist should not error
        let result = ts.store.delete_data("nonexistent").await;
        assert!(result.is_ok());
    }
}
