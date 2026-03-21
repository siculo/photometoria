// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! Filesystem-backed implementation of PhotoStore
//!
//! This module provides a thread-safe implementation of the PhotoStore trait
//! with full persistence to the filesystem. Photo metadata is stored as JSON files,
//! and binary image data is stored as individual files.

use async_trait::async_trait;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::models::Photo;

use super::{FileSystemLayout, PhotoStore, PhotoStoreError, PhotoStoreResult, TaskStore};

// ============================================================================
// FileSystemPhotoStore Implementation
// ============================================================================

/// Filesystem-backed implementation of PhotoStore with full persistence.
///
/// This implementation stores photo metadata both in memory (using DashMap for
/// fast concurrent access) and on the filesystem (as JSON files for persistence).
/// Binary image data is stored as individual files.
///
/// ## Characteristics
///
/// - **Thread-safe**: Supports concurrent access from multiple Tokio tasks
/// - **Lock-free reads**: Get operations don't acquire locks
/// - **Fine-grained locking**: Writes lock only the specific shard
/// - **Full persistence**: Photo metadata survives server restarts
/// - **Flat structure**: All photos in a single map, filtered by task_id when needed
///
/// ## Persistence Strategy
///
/// - Metadata is written to photos.json after each create/delete operation
/// - On startup, all existing photos.json files are loaded into memory
/// - Deleting a task (via TaskStore) removes the entire directory including photos
///
/// For details on the filesystem layout, see [`FileSystemLayout`]
pub struct FileSystemPhotoStore {
    /// Photo metadata storage
    photos: Arc<DashMap<Uuid, Photo>>,
    /// Filesystem layout manager
    layout: FileSystemLayout,
    /// Reference to the task store for resolving catalog identity
    task_store: Arc<dyn TaskStore>,
}

impl FileSystemPhotoStore {
    /// Creates a new filesystem-backed photo store.
    ///
    /// This constructor loads all existing photos from the filesystem.
    /// Any errors during loading are logged but don't prevent startup.
    ///
    /// # Arguments
    /// * `storage_path` - Base path for storing photo files
    /// * `task_store` - Reference to the task store for resolving task-to-catalog relationships
    pub async fn new(storage_path: PathBuf, task_store: Arc<dyn TaskStore>) -> Self {
        let store = Self {
            photos: Arc::new(DashMap::new()),
            layout: FileSystemLayout::new(storage_path),
            task_store,
        };
        store.load_all().await;
        store
    }

    /// Loads all photos from all task directories.
    async fn load_all(&self) {
        let tasks_dir = self.layout.tasks_root();

        if !tasks_dir.exists() {
            debug!("Tasks directory does not exist, starting with empty photo store");
            return;
        }

        let mut entries = match tokio::fs::read_dir(&tasks_dir).await {
            Ok(entries) => entries,
            Err(e) => {
                warn!("Failed to read tasks directory: {}", e);
                return;
            }
        };

        let mut loaded_count = 0;
        let mut error_count = 0;

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let photos_json = path.join("photos.json");
            if !photos_json.exists() {
                // No photos.json is normal - task might have no photos yet
                continue;
            }

            match self.load_photos_from_file(&photos_json).await {
                Ok(photos) => {
                    for photo in photos {
                        self.photos.insert(photo.photo_id, photo);
                        loaded_count += 1;
                    }
                }
                Err(e) => {
                    warn!("Failed to load photos from {:?}: {}", photos_json, e);
                    error_count += 1;
                }
            }
        }

        info!(
            "Loaded {} photos from filesystem ({} errors)",
            loaded_count, error_count
        );
    }

    /// Loads photos from a JSON file.
    async fn load_photos_from_file(&self, path: &PathBuf) -> PhotoStoreResult<Vec<Photo>> {
        let content = tokio::fs::read_to_string(path).await.map_err(|e| {
            PhotoStoreError::StorageError(format!("Failed to read photos file: {}", e))
        })?;

        serde_json::from_str(&content).map_err(|e| {
            PhotoStoreError::StorageError(format!("Failed to parse photos JSON: {}", e))
        })
    }

    /// Saves all photos for a task to the filesystem.
    async fn save_photos_for_task(&self, task_id: Uuid) -> PhotoStoreResult<()> {
        let photos: Vec<Photo> = self
            .photos
            .iter()
            .filter(|entry| entry.value().task_id == task_id)
            .map(|entry| entry.value().clone())
            .collect();

        let path = self.layout.photos_json_path(task_id);

        // If no photos, remove the file if it exists
        if photos.is_empty() {
            if path.exists() {
                if let Err(e) = tokio::fs::remove_file(&path).await {
                    debug!("Failed to remove empty photos.json: {}", e);
                }
            }
            return Ok(());
        }

        let content = serde_json::to_string_pretty(&photos).map_err(|e| {
            PhotoStoreError::StorageError(format!("Failed to serialize photos: {}", e))
        })?;

        tokio::fs::write(&path, content).await.map_err(|e| {
            error!("Failed to write photos file {:?}: {}", path, e);
            PhotoStoreError::StorageError(format!("Failed to write photos file: {}", e))
        })?;

        debug!("Saved {} photos metadata to {:?}", photos.len(), path);
        Ok(())
    }

    async fn delete_file_and_save_updated_photos(
        &self,
        photo_id: Uuid,
        photo: &Photo,
    ) -> Result<(), PhotoStoreError> {
        // Also delete the file from disk
        let file_path = self.layout.photo_file_path(photo.task_id, photo.photo_id);
        if file_path.exists() {
            if let Err(e) = tokio::fs::remove_file(&file_path).await {
                error!("Failed to remove photo file {:?}: {}", file_path, e);
                // Log but don't fail - metadata is already removed
            } else {
                debug!("Removed photo file: {:?}", file_path);
            }
        }

        // Save updated photos.json
        if let Err(e) = self.save_photos_for_task(photo.task_id).await {
            warn!("Failed to update photos.json after delete: {}", e);
            // Don't fail - the photo is already deleted from memory
        }

        info!(
            "Photo deleted successfully: {} (task: {}, filename: '{}')",
            photo_id, photo.task_id, photo.filename
        );
        Ok(())
    }
}

// ============================================================================
// PhotoStore Trait Implementation
// ============================================================================

#[async_trait]
impl PhotoStore for FileSystemPhotoStore {
    async fn create(&self, photo: Photo) -> PhotoStoreResult<Photo> {
        let photo_id = photo.photo_id;
        let task_id = photo.task_id;

        debug!("Attempting to create photo: {}", photo_id);

        match self.photos.entry(photo_id) {
            Entry::Occupied(_) => Err(PhotoStoreError::AlreadyExists(photo_id)),
            Entry::Vacant(entry) => {
                entry.insert(photo.clone());

                // Save updated photos.json
                if let Err(e) = self.save_photos_for_task(task_id).await {
                    // Rollback on failure
                    self.photos.remove(&photo_id);
                    return Err(e);
                }

                info!(
                    "Photo created successfully: {} (task: {}, filename: '{}')",
                    photo_id, photo.task_id, photo.filename
                );
                Ok(photo)
            }
        }
    }

    async fn get(&self, photo_id: Uuid) -> PhotoStoreResult<Option<Photo>> {
        debug!("Retrieving photo: {}", photo_id);

        Ok(self
            .photos
            .get(&photo_id)
            .map(|entry| entry.value().clone()))
    }

    async fn list_by_task(&self, task_id: Uuid) -> PhotoStoreResult<Vec<Photo>> {
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

    async fn find_by_client_id(
        &self,
        task_id: Uuid,
        client_id: &str,
    ) -> PhotoStoreResult<Vec<Photo>> {
        debug!(
            "Finding photos by client_id '{}' in task {}",
            client_id, task_id
        );

        let mut photos: Vec<Photo> = self
            .photos
            .iter()
            .filter(|entry| {
                let photo = entry.value();
                photo.task_id == task_id && photo.client_id.as_deref() == Some(client_id)
            })
            .map(|entry| entry.value().clone())
            .collect();

        photos.sort_by_key(|photo| photo.uploaded_at);

        debug!(
            "Found {} photos with client_id '{}' in task {}",
            photos.len(),
            client_id,
            task_id
        );
        Ok(photos)
    }

    async fn delete(&self, photo_id: Uuid) -> PhotoStoreResult<()> {
        debug!("Deleting photo: {}", photo_id);

        match self.photos.remove(&photo_id) {
            Some((_, photo)) => {
                self.delete_file_and_save_updated_photos(photo_id, &photo)
                    .await
            }
            None => Err(PhotoStoreError::NotFound(photo_id)),
        }
    }

    async fn delete_by_task(&self, task_id: Uuid) -> PhotoStoreResult<usize> {
        debug!("Deleting all photos for task: {}", task_id);

        // Collect photo_ids to delete (can't delete while iterating)
        let photo_ids: Vec<Uuid> = self
            .photos
            .iter()
            .filter(|entry| entry.value().task_id == task_id)
            .map(|entry| *entry.key())
            .collect();

        let count = photo_ids.len();

        // Remove metadata - files are deleted by TaskStore::delete()
        // when it removes the entire task directory
        for photo_id in &photo_ids {
            self.photos.remove(photo_id);
        }

        // No need to save photos.json - the directory will be deleted

        info!("Deleted {} photos for task {}", count, task_id);
        Ok(count)
    }

    async fn count_by_task(&self, task_id: Uuid) -> PhotoStoreResult<usize> {
        debug!("Counting photos for task: {}", task_id);

        let count = self
            .photos
            .iter()
            .filter(|entry| entry.value().task_id == task_id)
            .count();

        Ok(count)
    }

    async fn total_size_by_task(&self, task_id: Uuid) -> PhotoStoreResult<u64> {
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

    async fn exists(&self, photo_id: Uuid) -> PhotoStoreResult<bool> {
        debug!("Checking if photo exists: {}", photo_id);

        Ok(self.photos.contains_key(&photo_id))
    }

    async fn save_data(&self, photo_id: Uuid, data: &[u8]) -> PhotoStoreResult<()> {
        debug!("Saving {} bytes for photo: {}", data.len(), photo_id);

        // Get photo metadata
        let photo = self
            .photos
            .get(&photo_id)
            .ok_or(PhotoStoreError::NotFound(photo_id))?;
        let photo_clone = photo.value().clone();
        drop(photo); // Release the lock

        // Ensure imgs directory exists
        let imgs_dir = self
            .layout
            .ensure_photos_dir(photo_clone.task_id)
            .await
            .map_err(|e| {
                error!("Failed to create imgs directory: {}", e);
                PhotoStoreError::StorageError(format!("Failed to create imgs directory: {}", e))
            })?;
        debug!("Ensured imgs directory exists: {:?}", imgs_dir);

        let file_path = self
            .layout
            .photo_file_path(photo_clone.task_id, photo_clone.photo_id);
        tokio::fs::write(&file_path, data).await.map_err(|e| {
            error!("Failed to write photo file {:?}: {}", file_path, e);
            PhotoStoreError::StorageError(format!("Failed to write file: {}", e))
        })?;

        info!(
            "Saved {} bytes for photo: {} at {:?}",
            data.len(),
            photo_id,
            file_path
        );
        Ok(())
    }

    async fn load_data(&self, photo_id: Uuid) -> PhotoStoreResult<Vec<u8>> {
        debug!("Loading data for photo: {}", photo_id);

        // Get photo metadata
        let photo = self
            .photos
            .get(&photo_id)
            .ok_or(PhotoStoreError::NotFound(photo_id))?;
        let photo_clone = photo.value().clone();
        drop(photo); // Release the lock

        let file_path = self
            .layout
            .photo_file_path(photo_clone.task_id, photo_clone.photo_id);
        let data = tokio::fs::read(&file_path).await.map_err(|e| {
            error!("Failed to read photo file {:?}: {}", file_path, e);
            PhotoStoreError::NotFound(photo_id)
        })?;

        debug!("Loaded {} bytes for photo: {}", data.len(), photo_id);
        Ok(data)
    }

    async fn delete_data(&self, photo_id: Uuid) -> PhotoStoreResult<()> {
        debug!("Deleting data for photo: {}", photo_id);

        // Get photo metadata (if it exists)
        if let Some(photo) = self.photos.get(&photo_id) {
            let photo_clone = photo.value().clone();
            drop(photo); // Release the lock

            let file_path = self
                .layout
                .photo_file_path(photo_clone.task_id, photo_clone.photo_id);
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
    use crate::storage::FileSystemTaskStore;
    use tempfile::TempDir;

    // Helper struct to keep temp dir alive during test
    struct TestStore {
        store: FileSystemPhotoStore,
        _temp_dir: TempDir,
    }

    // Helper function to create a fresh store with temp directory
    async fn create_store() -> TestStore {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();
        let task_store: Arc<dyn TaskStore> =
            Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
        let store = FileSystemPhotoStore::new(storage_path, task_store).await;
        TestStore {
            store,
            _temp_dir: temp_dir,
        }
    }

    // Helper to create task directory (normally done by TaskStore)
    async fn create_task_dir(ts: &TestStore, task_id: Uuid) {
        let task_dir = ts.store.layout.task_dir(task_id);
        tokio::fs::create_dir_all(&task_dir)
            .await
            .expect("Failed to create task dir");
    }

    // Helper function to create a test photo
    fn create_test_photo(task_id: Uuid, filename: &str, size_bytes: u64) -> Photo {
        Photo::new(task_id, None, filename.to_string(), size_bytes)
    }

    #[tokio::test]
    async fn test_create_photo() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        create_task_dir(&ts, task_id).await;
        let photo = create_test_photo(task_id, "test.jpg", 1_000_000);

        let result = ts.store.create(photo.clone()).await;

        assert!(result.is_ok());
        let created = result.unwrap();
        assert_eq!(created.photo_id, photo.photo_id);
        assert_eq!(created.task_id, photo.task_id);
        assert_eq!(created.filename, photo.filename);

        // Verify it exists in the store
        let exists = ts.store.exists(photo.photo_id).await.unwrap();
        assert!(exists);

        // Verify photos.json was created
        assert!(ts.store.layout.photos_json_path(task_id).exists());
    }

    #[tokio::test]
    async fn test_create_duplicate_fails() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        create_task_dir(&ts, task_id).await;
        let photo = create_test_photo(task_id, "test.jpg", 1_000_000);

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
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        create_task_dir(&ts, task_id).await;
        let photo = create_test_photo(task_id, "vacation.jpg", 5_000_000);

        ts.store.create(photo.clone()).await.unwrap();

        let result = ts.store.get(photo.photo_id).await.unwrap();

        assert!(result.is_some());
        let retrieved = result.unwrap();
        assert_eq!(retrieved.photo_id, photo.photo_id);
        assert_eq!(retrieved.task_id, photo.task_id);
        assert_eq!(retrieved.filename, photo.filename);
        assert_eq!(retrieved.size_bytes, photo.size_bytes);
    }

    #[tokio::test]
    async fn test_get_nonexistent_photo() {
        let ts = create_store().await;

        let result = ts.store.get(Uuid::new_v4()).await.unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_list_by_task() {
        let ts = create_store().await;
        let task_a = Uuid::new_v4();
        let task_b = Uuid::new_v4();
        create_task_dir(&ts, task_a).await;
        create_task_dir(&ts, task_b).await;

        // Create photos for two different tasks
        let photo1 = create_test_photo(task_a, "photo1.jpg", 1_000_000);
        let photo2 = create_test_photo(task_a, "photo2.jpg", 2_000_000);
        let photo3 = create_test_photo(task_b, "photo3.jpg", 3_000_000);

        ts.store.create(photo1.clone()).await.unwrap();
        ts.store.create(photo2.clone()).await.unwrap();
        ts.store.create(photo3.clone()).await.unwrap();

        // List photos for task_A
        let photos_a = ts.store.list_by_task(task_a).await.unwrap();
        assert_eq!(photos_a.len(), 2);
        assert!(photos_a.iter().all(|p| p.task_id == task_a));

        // List photos for task_B
        let photos_b = ts.store.list_by_task(task_b).await.unwrap();
        assert_eq!(photos_b.len(), 1);
        assert_eq!(photos_b[0].task_id, task_b);

        // List photos for nonexistent task
        let photos_c = ts.store.list_by_task(Uuid::new_v4()).await.unwrap();
        assert!(photos_c.is_empty());
    }

    #[tokio::test]
    async fn test_delete_photo() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        create_task_dir(&ts, task_id).await;
        let photo = create_test_photo(task_id, "test.jpg", 1_000_000);

        ts.store.create(photo.clone()).await.unwrap();
        // Save data so we can verify it's deleted too
        ts.store
            .save_data(photo.photo_id, &[1, 2, 3])
            .await
            .unwrap();
        let file_path = ts
            .store
            .layout
            .photo_file_path(photo.task_id, photo.photo_id);
        assert!(file_path.exists());

        // Delete the photo
        let result = ts.store.delete(photo.photo_id).await;

        assert!(result.is_ok());

        // Verify it no longer exists
        let retrieved = ts.store.get(photo.photo_id).await.unwrap();
        assert!(retrieved.is_none());

        // Verify file was removed
        assert!(!file_path.exists());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_fails() {
        let ts = create_store().await;

        let nonexistent_id = Uuid::new_v4();
        let result = ts.store.delete(nonexistent_id).await;

        assert!(result.is_err());
        match result {
            Err(PhotoStoreError::NotFound(id)) => {
                assert_eq!(id, nonexistent_id);
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_delete_by_task() {
        let ts = create_store().await;
        let task_a = Uuid::new_v4();
        let task_b = Uuid::new_v4();
        create_task_dir(&ts, task_a).await;
        create_task_dir(&ts, task_b).await;

        // Create photos for two different tasks
        let photo1 = create_test_photo(task_a, "photo1.jpg", 1_000_000);
        let photo2 = create_test_photo(task_a, "photo2.jpg", 2_000_000);
        let photo3 = create_test_photo(task_b, "photo3.jpg", 3_000_000);

        ts.store.create(photo1).await.unwrap();
        ts.store.create(photo2).await.unwrap();
        ts.store.create(photo3.clone()).await.unwrap();

        // Delete all photos for task_A
        let deleted = ts.store.delete_by_task(task_a).await.unwrap();
        assert_eq!(deleted, 2);

        // Verify task_A photos are gone
        let photos_a = ts.store.list_by_task(task_a).await.unwrap();
        assert!(photos_a.is_empty());

        // Verify task_B photo still exists
        let photos_b = ts.store.list_by_task(task_b).await.unwrap();
        assert_eq!(photos_b.len(), 1);
    }

    #[tokio::test]
    async fn test_count_by_task() {
        let ts = create_store().await;
        let task_a = Uuid::new_v4();
        let task_b = Uuid::new_v4();
        create_task_dir(&ts, task_a).await;
        create_task_dir(&ts, task_b).await;

        // Empty task should have count 0
        let count = ts.store.count_by_task(task_a).await.unwrap();
        assert_eq!(count, 0);

        // Add photos
        let photo1 = create_test_photo(task_a, "photo1.jpg", 1_000_000);
        let photo2 = create_test_photo(task_a, "photo2.jpg", 2_000_000);
        let photo3 = create_test_photo(task_b, "photo3.jpg", 3_000_000);

        ts.store.create(photo1).await.unwrap();
        ts.store.create(photo2).await.unwrap();
        ts.store.create(photo3).await.unwrap();

        // Count for task_A
        let count = ts.store.count_by_task(task_a).await.unwrap();
        assert_eq!(count, 2);

        // Count for task_B
        let count = ts.store.count_by_task(task_b).await.unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_total_size_by_task() {
        let ts = create_store().await;
        let task_a = Uuid::new_v4();
        let task_b = Uuid::new_v4();
        create_task_dir(&ts, task_a).await;
        create_task_dir(&ts, task_b).await;

        // Empty task should have size 0
        let size = ts.store.total_size_by_task(task_a).await.unwrap();
        assert_eq!(size, 0);

        // Add photos with known sizes
        let photo1 = create_test_photo(task_a, "photo1.jpg", 1_000_000);
        let photo2 = create_test_photo(task_a, "photo2.jpg", 2_500_000);
        let photo3 = create_test_photo(task_b, "photo3.jpg", 3_000_000);

        ts.store.create(photo1).await.unwrap();
        ts.store.create(photo2).await.unwrap();
        ts.store.create(photo3).await.unwrap();

        // Total size for task_A
        let size = ts.store.total_size_by_task(task_a).await.unwrap();
        assert_eq!(size, 3_500_000);

        // Total size for task_B
        let size = ts.store.total_size_by_task(task_b).await.unwrap();
        assert_eq!(size, 3_000_000);
    }

    #[tokio::test]
    async fn test_exists() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        create_task_dir(&ts, task_id).await;
        let photo = create_test_photo(task_id, "test.jpg", 1_000_000);

        // Should not exist initially
        let exists = ts.store.exists(photo.photo_id).await.unwrap();
        assert!(!exists);

        // Create the photo
        ts.store.create(photo.clone()).await.unwrap();

        // Should exist now
        let exists = ts.store.exists(photo.photo_id).await.unwrap();
        assert!(exists);

        // Random ID should not exist
        let exists = ts.store.exists(Uuid::new_v4()).await.unwrap();
        assert!(!exists);
    }

    // ========================================================================
    // Binary Data Tests
    // ========================================================================

    #[tokio::test]
    async fn test_save_and_load_data() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        create_task_dir(&ts, task_id).await;
        let photo = create_test_photo(task_id, "test.jpg", 1_000);

        // Create the photo first
        ts.store.create(photo.clone()).await.unwrap();

        // Save some data
        let test_data = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        ts.store
            .save_data(photo.photo_id, &test_data)
            .await
            .unwrap();

        // Verify file exists on disk
        let file_path = ts
            .store
            .layout
            .photo_file_path(photo.task_id, photo.photo_id);
        assert!(file_path.exists());

        // Load the data back
        let loaded = ts.store.load_data(photo.photo_id).await.unwrap();
        assert_eq!(loaded, test_data);
    }

    #[tokio::test]
    async fn test_save_data_requires_photo_metadata() {
        let ts = create_store().await;

        // Try to save data without creating photo metadata first
        let nonexistent_id = Uuid::new_v4();
        let test_data = vec![0xFF, 0xD8, 0xFF, 0xE0];
        let result = ts.store.save_data(nonexistent_id, &test_data).await;

        assert!(result.is_err());
        match result {
            Err(PhotoStoreError::NotFound(id)) => assert_eq!(id, nonexistent_id),
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_load_nonexistent_data() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        create_task_dir(&ts, task_id).await;
        let photo = create_test_photo(task_id, "test.jpg", 1_000);

        // Create photo metadata but don't save data
        ts.store.create(photo.clone()).await.unwrap();

        // Try to load data (file doesn't exist)
        let result = ts.store.load_data(photo.photo_id).await;

        assert!(result.is_err());
        match result {
            Err(PhotoStoreError::NotFound(_)) => {}
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_delete_also_removes_data() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        create_task_dir(&ts, task_id).await;
        let photo = create_test_photo(task_id, "test.jpg", 1_000);

        // Create photo and save data
        ts.store.create(photo.clone()).await.unwrap();
        let test_data = vec![0xFF, 0xD8, 0xFF, 0xE0];
        ts.store
            .save_data(photo.photo_id, &test_data)
            .await
            .unwrap();

        // Delete the photo
        ts.store.delete(photo.photo_id).await.unwrap();

        // Metadata should be gone, so load_data will fail with NotFound
        let result = ts.store.load_data(photo.photo_id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_delete_data_idempotent() {
        let ts = create_store().await;

        // Delete data that doesn't exist should not error
        let result = ts.store.delete_data(Uuid::new_v4()).await;
        assert!(result.is_ok());
    }

    // ========================================================================
    // Persistence Tests
    // ========================================================================

    #[tokio::test]
    async fn test_persistence_survives_reload() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();
        let task_id = Uuid::new_v4();

        // Create task directory (normally done by TaskStore)
        let task_dir = storage_path.join("tasks").join(task_id.to_string());
        tokio::fs::create_dir_all(&task_dir).await.unwrap();

        let photo1 = create_test_photo(task_id, "photo1.jpg", 1_000_000);
        let photo2 = create_test_photo(task_id, "photo2.jpg", 2_000_000);
        let photo1_id = photo1.photo_id;
        let photo2_id = photo2.photo_id;

        {
            let task_store: Arc<dyn TaskStore> =
                Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
            let store = FileSystemPhotoStore::new(storage_path.clone(), task_store).await;
            store.create(photo1).await.unwrap();
            store.create(photo2).await.unwrap();
            assert_eq!(store.count_by_task(task_id).await.unwrap(), 2);
        }

        // Create new store instance (simulates server restart)
        let task_store: Arc<dyn TaskStore> =
            Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
        let store = FileSystemPhotoStore::new(storage_path, task_store).await;

        // Photos should be loaded from filesystem
        assert_eq!(store.count_by_task(task_id).await.unwrap(), 2);
        assert!(store.exists(photo1_id).await.unwrap());
        assert!(store.exists(photo2_id).await.unwrap());

        let loaded_photo1 = store.get(photo1_id).await.unwrap().unwrap();
        assert_eq!(loaded_photo1.filename, "photo1.jpg");
    }

    #[tokio::test]
    async fn test_delete_removes_from_photos_json() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();
        let task_id = Uuid::new_v4();

        // Create task directory
        let task_dir = storage_path.join("tasks").join(task_id.to_string());
        tokio::fs::create_dir_all(&task_dir).await.unwrap();

        let photo = create_test_photo(task_id, "photo.jpg", 1_000_000);
        let photo_id = photo.photo_id;

        {
            let task_store: Arc<dyn TaskStore> =
                Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
            let store = FileSystemPhotoStore::new(storage_path.clone(), task_store).await;
            store.create(photo).await.unwrap();
            store.delete(photo_id).await.unwrap();
        }

        // Reload and verify photo is gone
        let task_store: Arc<dyn TaskStore> =
            Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
        let store = FileSystemPhotoStore::new(storage_path, task_store).await;
        assert_eq!(store.count_by_task(task_id).await.unwrap(), 0);
        assert!(!store.exists(photo_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_photos_stored_in_imgs_subdirectory() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();
        let task_id = Uuid::new_v4();

        // Create task directory
        let task_dir = storage_path.join("tasks").join(task_id.to_string());
        tokio::fs::create_dir_all(&task_dir).await.unwrap();

        let photo = create_test_photo(task_id, "test.jpg", 1_000);
        let photo_id = photo.photo_id;

        let task_store: Arc<dyn TaskStore> =
            Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
        let store = FileSystemPhotoStore::new(storage_path, task_store).await;
        store.create(photo).await.unwrap();

        // Save photo data
        let test_data = vec![0xFF, 0xD8, 0xFF, 0xE0];
        store.save_data(photo_id, &test_data).await.unwrap();

        // Verify imgs/ subdirectory exists
        let imgs_dir = task_dir.join("imgs");
        assert!(imgs_dir.exists(), "imgs/ directory should exist");
        assert!(imgs_dir.is_dir(), "imgs/ should be a directory");

        // Verify photo file is in imgs/ subdirectory
        let photo_file = imgs_dir.join(photo_id.to_string());
        assert!(photo_file.exists(), "Photo file should exist in imgs/");
        assert!(photo_file.is_file(), "Photo should be a file");

        // Verify photo is NOT in task root directory
        let wrong_path = task_dir.join(photo_id.to_string());
        assert!(
            !wrong_path.exists(),
            "Photo should NOT be in task root directory"
        );
    }

    // ========================================================================
    // find_by_client_id Tests
    // ========================================================================

    #[tokio::test]
    async fn test_find_by_client_id_returns_matching_photos() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        create_task_dir(&ts, task_id).await;

        let photo1 = Photo::new(
            task_id,
            Some("lr:42".to_string()),
            "a.jpg".to_string(),
            1000,
        );
        let photo2 = Photo::new(
            task_id,
            Some("lr:42".to_string()),
            "b.jpg".to_string(),
            2000,
        );
        let photo3 = Photo::new(
            task_id,
            Some("lr:99".to_string()),
            "c.jpg".to_string(),
            3000,
        );

        let photo1_id = photo1.photo_id;
        let photo2_id = photo2.photo_id;

        ts.store.create(photo1).await.unwrap();
        ts.store.create(photo2).await.unwrap();
        ts.store.create(photo3).await.unwrap();

        let results = ts.store.find_by_client_id(task_id, "lr:42").await.unwrap();

        assert_eq!(results.len(), 2);
        let ids: Vec<Uuid> = results.iter().map(|p| p.photo_id).collect();
        assert!(ids.contains(&photo1_id));
        assert!(ids.contains(&photo2_id));
    }

    #[tokio::test]
    async fn test_find_by_client_id_no_match() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        create_task_dir(&ts, task_id).await;

        let photo = Photo::new(
            task_id,
            Some("lr:42".to_string()),
            "a.jpg".to_string(),
            1000,
        );
        ts.store.create(photo).await.unwrap();

        let results = ts.store.find_by_client_id(task_id, "lr:999").await.unwrap();

        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_find_by_client_id_scoped_to_task() {
        let ts = create_store().await;
        let task_a = Uuid::new_v4();
        let task_b = Uuid::new_v4();
        create_task_dir(&ts, task_a).await;
        create_task_dir(&ts, task_b).await;

        let photo_a = Photo::new(task_a, Some("lr:42".to_string()), "a.jpg".to_string(), 1000);
        let photo_b = Photo::new(task_b, Some("lr:42".to_string()), "b.jpg".to_string(), 2000);

        let photo_a_id = photo_a.photo_id;

        ts.store.create(photo_a).await.unwrap();
        ts.store.create(photo_b).await.unwrap();

        let results = ts.store.find_by_client_id(task_a, "lr:42").await.unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].photo_id, photo_a_id);
    }

    #[tokio::test]
    async fn test_find_by_client_id_ignores_none_client_id() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        create_task_dir(&ts, task_id).await;

        let photo = Photo::new(task_id, None, "a.jpg".to_string(), 1000);
        ts.store.create(photo).await.unwrap();

        let results = ts.store.find_by_client_id(task_id, "lr:42").await.unwrap();

        assert!(results.is_empty());
    }
}
