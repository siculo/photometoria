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
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::models::Photo;

use super::utils::parse_uuid_from_dir;
use super::{FileSystemLayout, PhotoStore, PhotoStoreError, PhotoStoreResult, TaskStore};

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
    photos: Arc<DashMap<Uuid, Photo>>,
    layout: FileSystemLayout,
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

    /// Resolves the catalog identity for a given task.
    async fn resolve_catalog_id(&self, task_id: Uuid) -> PhotoStoreResult<Uuid> {
        let task = self
            .task_store
            .get(task_id)
            .await
            .map_err(|e| {
                PhotoStoreError::StorageError(format!("Failed to query task store: {}", e))
            })?
            .ok_or_else(|| {
                PhotoStoreError::StorageError(format!(
                    "Cannot resolve catalog: task {} not found",
                    task_id
                ))
            })?;
        Ok(task.catalog_id)
    }

    /// Loads all photos from all task directories.
    ///
    /// Scans the catalog hierarchy: `catalogs/{catalog_id}/tasks/{task_id}/photos.json`
    async fn load_all(&self) {
        let catalog_dirs = match self.layout.scan_catalog_dirs().await {
            Ok(dirs) => dirs,
            Err(e) => {
                warn!("Failed to scan catalog directories: {}", e);
                return;
            }
        };

        let mut loaded = 0;
        let mut errors = 0;

        for catalog_dir in catalog_dirs {
            let Some(catalog_id) = parse_uuid_from_dir(&catalog_dir) else {
                warn!("Skipping invalid catalog directory: {:?}", catalog_dir);
                continue;
            };
            let (n_loaded, n_errors) = self.load_catalog_photos(catalog_id).await;
            loaded += n_loaded;
            errors += n_errors;
        }

        info!("Loaded {} photos from filesystem ({} errors)", loaded, errors);
    }

    /// Loads all photos for a single catalog from the filesystem.
    ///
    /// Returns the count of successfully loaded photos and errors encountered.
    async fn load_catalog_photos(&self, catalog_id: Uuid) -> (usize, usize) {
        let photos_files = match self.layout.scan_photos_json_files(catalog_id).await {
            Ok(files) => files,
            Err(e) => {
                warn!(
                    "Failed to scan photos files for catalog {}: {}",
                    catalog_id, e
                );
                return (0, 1);
            }
        };

        let mut loaded = 0;
        let mut errors = 0;

        for photos_json in photos_files {
            match self.load_photos_from_file(&photos_json).await {
                Ok(photos) => {
                    loaded += photos.len();
                    for photo in photos {
                        self.photos.insert(photo.photo_id, photo);
                    }
                }
                Err(e) => {
                    warn!("Failed to load photos from {:?}: {}", photos_json, e);
                    errors += 1;
                }
            }
        }

        (loaded, errors)
    }

    /// Loads photos from a JSON file.
    async fn load_photos_from_file(&self, path: &Path) -> PhotoStoreResult<Vec<Photo>> {
        let content = tokio::fs::read_to_string(path).await.map_err(|e| {
            PhotoStoreError::StorageError(format!("Failed to read photos file: {}", e))
        })?;

        serde_json::from_str(&content).map_err(|e| {
            PhotoStoreError::StorageError(format!("Failed to parse photos JSON: {}", e))
        })
    }

    /// Saves all photos for a task to the filesystem.
    async fn save_photos_for_task(&self, catalog_id: Uuid, task_id: Uuid) -> PhotoStoreResult<()> {
        let photos: Vec<Photo> = self
            .photos
            .iter()
            .filter(|entry| entry.value().task_id == task_id)
            .map(|entry| entry.value().clone())
            .collect();

        let path = self.layout.photos_json_path(catalog_id, task_id);

        if photos.is_empty() {
            match tokio::fs::remove_file(&path).await {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => debug!("Failed to remove empty photos.json: {}", e),
            }
            return Ok(());
        }

        let tmp_path = path.with_extension("tmp");
        let content = serde_json::to_string_pretty(&photos).map_err(|e| {
            PhotoStoreError::StorageError(format!("Failed to serialize photos: {}", e))
        })?;

        tokio::fs::write(&tmp_path, &content).await.map_err(|e| {
            error!("Failed to write temporary photos file {:?}: {}", tmp_path, e);
            PhotoStoreError::StorageError(format!("Failed to write photos file: {}", e))
        })?;

        tokio::fs::rename(&tmp_path, &path).await.map_err(|e| {
            error!("Failed to rename {:?} -> {:?}: {}", tmp_path, path, e);
            PhotoStoreError::StorageError(format!("Failed to atomically save photos file: {}", e))
        })?;

        debug!("Saved {} photos metadata to {:?}", photos.len(), path);
        Ok(())
    }

    async fn delete_file_and_save_updated_photos(
        &self,
        photo_id: Uuid,
        photo: &Photo,
        catalog_id: Uuid,
    ) -> Result<(), PhotoStoreError> {
        self.save_photos_for_task(catalog_id, photo.task_id).await?;

        let file_path = self
            .layout
            .photo_file_path(catalog_id, photo.task_id, photo.photo_id);
        match tokio::fs::remove_file(&file_path).await {
            Ok(()) => debug!("Removed photo file: {:?}", file_path),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => error!("Failed to remove photo file {:?}: {}", file_path, e),
        }

        info!(
            "Photo deleted successfully: {} (task: {}, filename: '{}')",
            photo_id, photo.task_id, photo.filename
        );
        Ok(())
    }
}


#[async_trait]
impl PhotoStore for FileSystemPhotoStore {
    async fn create(&self, photo: Photo) -> PhotoStoreResult<Photo> {
        let photo_id = photo.photo_id;
        let task_id = photo.task_id;

        debug!("Attempting to create photo: {}", photo_id);

        let catalog_id = self.resolve_catalog_id(task_id).await?;

        match self.photos.entry(photo_id) {
            Entry::Occupied(_) => Err(PhotoStoreError::AlreadyExists(photo_id)),
            Entry::Vacant(entry) => {
                entry.insert(photo.clone());

                if let Err(e) = self.save_photos_for_task(catalog_id, task_id).await {
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

        let task_id = match self.photos.get(&photo_id) {
            Some(entry) => entry.value().task_id,
            None => return Err(PhotoStoreError::NotFound(photo_id)),
        };

        let catalog_id = self.resolve_catalog_id(task_id).await?;

        let (_, photo) = match self.photos.remove(&photo_id) {
            Some(entry) => entry,
            None => return Err(PhotoStoreError::NotFound(photo_id)),
        };

        if let Err(e) = self
            .delete_file_and_save_updated_photos(photo_id, &photo, catalog_id)
            .await
        {
            self.photos.insert(photo_id, photo);
            return Err(e);
        }

        Ok(())
    }

    async fn delete_by_task(&self, task_id: Uuid) -> PhotoStoreResult<usize> {
        debug!("Deleting all photos for task: {}", task_id);

        let photo_ids: Vec<Uuid> = self
            .photos
            .iter()
            .filter(|entry| entry.value().task_id == task_id)
            .map(|entry| *entry.key())
            .collect();

        let count = photo_ids.len();

        for photo_id in &photo_ids {
            self.photos.remove(photo_id);
        }

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

        let photo = self
            .photos
            .get(&photo_id)
            .ok_or(PhotoStoreError::NotFound(photo_id))?;
        let photo_clone = photo.value().clone();
        drop(photo);

        let catalog_id = self.resolve_catalog_id(photo_clone.task_id).await?;

        let imgs_dir = self
            .layout
            .ensure_photos_dir(catalog_id, photo_clone.task_id)
            .await
            .map_err(|e| {
                error!("Failed to create imgs directory: {}", e);
                PhotoStoreError::StorageError(format!("Failed to create imgs directory: {}", e))
            })?;
        debug!("Ensured imgs directory exists: {:?}", imgs_dir);

        let file_path =
            self.layout
                .photo_file_path(catalog_id, photo_clone.task_id, photo_clone.photo_id);
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

        let photo = self
            .photos
            .get(&photo_id)
            .ok_or(PhotoStoreError::NotFound(photo_id))?;
        let photo_clone = photo.value().clone();
        drop(photo);

        let catalog_id = self.resolve_catalog_id(photo_clone.task_id).await?;
        let file_path =
            self.layout
                .photo_file_path(catalog_id, photo_clone.task_id, photo_clone.photo_id);
        let data = tokio::fs::read(&file_path).await.map_err(|e| {
            error!("Failed to read photo file {:?}: {}", file_path, e);
            if e.kind() == std::io::ErrorKind::NotFound {
                PhotoStoreError::NotFound(photo_id)
            } else {
                PhotoStoreError::StorageError(format!("Failed to read photo file: {}", e))
            }
        })?;

        debug!("Loaded {} bytes for photo: {}", data.len(), photo_id);
        Ok(data)
    }

    async fn delete_data(&self, photo_id: Uuid) -> PhotoStoreResult<()> {
        debug!("Deleting data for photo: {}", photo_id);

        if let Some(photo) = self.photos.get(&photo_id) {
            let photo_clone = photo.value().clone();
            drop(photo);

            let catalog_id = self.resolve_catalog_id(photo_clone.task_id).await?;
            let file_path =
                self.layout
                    .photo_file_path(catalog_id, photo_clone.task_id, photo_clone.photo_id);
            match tokio::fs::remove_file(&file_path).await {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => error!("Failed to remove photo file {:?}: {}", file_path, e),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Task;
    use crate::storage::FileSystemTaskStore;
    use chrono::Utc;
    use tempfile::TempDir;

    struct TestStore {
        store: FileSystemPhotoStore,
        task_store: Arc<dyn TaskStore>,
        _temp_dir: TempDir,
    }

    async fn create_store() -> TestStore {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();
        let task_store: Arc<dyn TaskStore> =
            Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
        let store = FileSystemPhotoStore::new(storage_path, task_store.clone()).await;
        TestStore {
            store,
            task_store,
            _temp_dir: temp_dir,
        }
    }

    /// Creates a task in the task store so that resolve_catalog_id works.
    /// Returns the catalog_id assigned to the task.
    async fn setup_task(ts: &TestStore, task_id: Uuid) -> Uuid {
        let catalog_id = Uuid::new_v4();
        let task = Task {
            task_id,
            catalog_id,
            name: "test".to_string(),
            context: "test context".to_string(),
            created_at: Utc::now(),
        };
        ts.task_store.create(task).await.unwrap();
        catalog_id
    }

    fn create_test_photo(task_id: Uuid, filename: &str, size_bytes: u64) -> Photo {
        Photo::new(task_id, None, filename.to_string(), size_bytes)
    }

    #[tokio::test]
    async fn test_create_photo() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        let catalog_id = setup_task(&ts, task_id).await;
        let photo = create_test_photo(task_id, "test.jpg", 1_000_000);

        let result = ts.store.create(photo.clone()).await;

        assert!(result.is_ok());
        let created = result.unwrap();
        assert_eq!(created.photo_id, photo.photo_id);
        assert_eq!(created.task_id, photo.task_id);
        assert_eq!(created.filename, photo.filename);

        let exists = ts.store.exists(photo.photo_id).await.unwrap();
        assert!(exists);

        assert!(
            ts.store
                .layout
                .photos_json_path(catalog_id, task_id)
                .exists()
        );
    }

    #[tokio::test]
    async fn test_create_duplicate_fails() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        setup_task(&ts, task_id).await;
        let photo = create_test_photo(task_id, "test.jpg", 1_000_000);

        ts.store.create(photo.clone()).await.unwrap();

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
        setup_task(&ts, task_id).await;
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
        setup_task(&ts, task_a).await;
        setup_task(&ts, task_b).await;

        let photo1 = create_test_photo(task_a, "photo1.jpg", 1_000_000);
        let photo2 = create_test_photo(task_a, "photo2.jpg", 2_000_000);
        let photo3 = create_test_photo(task_b, "photo3.jpg", 3_000_000);

        ts.store.create(photo1.clone()).await.unwrap();
        ts.store.create(photo2.clone()).await.unwrap();
        ts.store.create(photo3.clone()).await.unwrap();

        let photos_a = ts.store.list_by_task(task_a).await.unwrap();
        assert_eq!(photos_a.len(), 2);
        assert!(photos_a.iter().all(|p| p.task_id == task_a));

        let photos_b = ts.store.list_by_task(task_b).await.unwrap();
        assert_eq!(photos_b.len(), 1);
        assert_eq!(photos_b[0].task_id, task_b);

        let photos_c = ts.store.list_by_task(Uuid::new_v4()).await.unwrap();
        assert!(photos_c.is_empty());
    }

    #[tokio::test]
    async fn test_delete_photo() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        let catalog_id = setup_task(&ts, task_id).await;
        let photo = create_test_photo(task_id, "test.jpg", 1_000_000);

        ts.store.create(photo.clone()).await.unwrap();
        ts.store
            .save_data(photo.photo_id, &[1, 2, 3])
            .await
            .unwrap();
        let file_path = ts
            .store
            .layout
            .photo_file_path(catalog_id, photo.task_id, photo.photo_id);
        assert!(file_path.exists());

        let result = ts.store.delete(photo.photo_id).await;

        assert!(result.is_ok());

        let retrieved = ts.store.get(photo.photo_id).await.unwrap();
        assert!(retrieved.is_none());

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
        setup_task(&ts, task_a).await;
        setup_task(&ts, task_b).await;

        // Create photos for two different tasks
        let photo1 = create_test_photo(task_a, "photo1.jpg", 1_000_000);
        let photo2 = create_test_photo(task_a, "photo2.jpg", 2_000_000);
        let photo3 = create_test_photo(task_b, "photo3.jpg", 3_000_000);

        ts.store.create(photo1).await.unwrap();
        ts.store.create(photo2).await.unwrap();
        ts.store.create(photo3.clone()).await.unwrap();

        let deleted = ts.store.delete_by_task(task_a).await.unwrap();
        assert_eq!(deleted, 2);

        let photos_a = ts.store.list_by_task(task_a).await.unwrap();
        assert!(photos_a.is_empty());

        let photos_b = ts.store.list_by_task(task_b).await.unwrap();
        assert_eq!(photos_b.len(), 1);
    }

    #[tokio::test]
    async fn test_count_by_task() {
        let ts = create_store().await;
        let task_a = Uuid::new_v4();
        let task_b = Uuid::new_v4();
        setup_task(&ts, task_a).await;
        setup_task(&ts, task_b).await;

        let count = ts.store.count_by_task(task_a).await.unwrap();
        assert_eq!(count, 0);

        let photo1 = create_test_photo(task_a, "photo1.jpg", 1_000_000);
        let photo2 = create_test_photo(task_a, "photo2.jpg", 2_000_000);
        let photo3 = create_test_photo(task_b, "photo3.jpg", 3_000_000);

        ts.store.create(photo1).await.unwrap();
        ts.store.create(photo2).await.unwrap();
        ts.store.create(photo3).await.unwrap();

        let count = ts.store.count_by_task(task_a).await.unwrap();
        assert_eq!(count, 2);

        let count = ts.store.count_by_task(task_b).await.unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_total_size_by_task() {
        let ts = create_store().await;
        let task_a = Uuid::new_v4();
        let task_b = Uuid::new_v4();
        setup_task(&ts, task_a).await;
        setup_task(&ts, task_b).await;

        let size = ts.store.total_size_by_task(task_a).await.unwrap();
        assert_eq!(size, 0);

        let photo1 = create_test_photo(task_a, "photo1.jpg", 1_000_000);
        let photo2 = create_test_photo(task_a, "photo2.jpg", 2_500_000);
        let photo3 = create_test_photo(task_b, "photo3.jpg", 3_000_000);

        ts.store.create(photo1).await.unwrap();
        ts.store.create(photo2).await.unwrap();
        ts.store.create(photo3).await.unwrap();

        let size = ts.store.total_size_by_task(task_a).await.unwrap();
        assert_eq!(size, 3_500_000);

        let size = ts.store.total_size_by_task(task_b).await.unwrap();
        assert_eq!(size, 3_000_000);
    }

    #[tokio::test]
    async fn test_exists() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        setup_task(&ts, task_id).await;
        let photo = create_test_photo(task_id, "test.jpg", 1_000_000);

        let exists = ts.store.exists(photo.photo_id).await.unwrap();
        assert!(!exists);

        ts.store.create(photo.clone()).await.unwrap();

        let exists = ts.store.exists(photo.photo_id).await.unwrap();
        assert!(exists);

        let exists = ts.store.exists(Uuid::new_v4()).await.unwrap();
        assert!(!exists);
    }

    #[tokio::test]
    async fn test_save_and_load_data() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        let catalog_id = setup_task(&ts, task_id).await;
        let photo = create_test_photo(task_id, "test.jpg", 1_000);

        ts.store.create(photo.clone()).await.unwrap();

        let test_data = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        ts.store
            .save_data(photo.photo_id, &test_data)
            .await
            .unwrap();

        let file_path = ts
            .store
            .layout
            .photo_file_path(catalog_id, photo.task_id, photo.photo_id);
        assert!(file_path.exists());

        let loaded = ts.store.load_data(photo.photo_id).await.unwrap();
        assert_eq!(loaded, test_data);
    }

    #[tokio::test]
    async fn test_save_data_requires_photo_metadata() {
        let ts = create_store().await;

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
        setup_task(&ts, task_id).await;
        let photo = create_test_photo(task_id, "test.jpg", 1_000);

        ts.store.create(photo.clone()).await.unwrap();

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
        setup_task(&ts, task_id).await;
        let photo = create_test_photo(task_id, "test.jpg", 1_000);

        ts.store.create(photo.clone()).await.unwrap();
        let test_data = vec![0xFF, 0xD8, 0xFF, 0xE0];
        ts.store
            .save_data(photo.photo_id, &test_data)
            .await
            .unwrap();

        ts.store.delete(photo.photo_id).await.unwrap();

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

    #[tokio::test]
    async fn test_persistence_survives_reload() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();
        let task_id = Uuid::new_v4();
        let catalog_id = Uuid::new_v4();

        let photo1 = create_test_photo(task_id, "photo1.jpg", 1_000_000);
        let photo2 = create_test_photo(task_id, "photo2.jpg", 2_000_000);
        let photo1_id = photo1.photo_id;
        let photo2_id = photo2.photo_id;

        {
            let task_store: Arc<dyn TaskStore> =
                Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
            let task = Task {
                task_id,
                catalog_id,
                name: "test".to_string(),
                context: "test".to_string(),
                created_at: Utc::now(),
            };
            task_store.create(task).await.unwrap();
            let store = FileSystemPhotoStore::new(storage_path.clone(), task_store).await;
            store.create(photo1).await.unwrap();
            store.create(photo2).await.unwrap();
            assert_eq!(store.count_by_task(task_id).await.unwrap(), 2);
        }

        let task_store: Arc<dyn TaskStore> =
            Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
        let store = FileSystemPhotoStore::new(storage_path, task_store).await;

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
        let catalog_id = Uuid::new_v4();

        let photo = create_test_photo(task_id, "photo.jpg", 1_000_000);
        let photo_id = photo.photo_id;

        {
            let task_store: Arc<dyn TaskStore> =
                Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
            let task = Task {
                task_id,
                catalog_id,
                name: "test".to_string(),
                context: "test".to_string(),
                created_at: Utc::now(),
            };
            task_store.create(task).await.unwrap();
            let store = FileSystemPhotoStore::new(storage_path.clone(), task_store).await;
            store.create(photo).await.unwrap();
            store.delete(photo_id).await.unwrap();
        }

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
        let catalog_id = Uuid::new_v4();

        let photo = create_test_photo(task_id, "test.jpg", 1_000);
        let photo_id = photo.photo_id;

        let task_store: Arc<dyn TaskStore> =
            Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
        let task = Task {
            task_id,
            catalog_id,
            name: "test".to_string(),
            context: "test".to_string(),
            created_at: Utc::now(),
        };
        task_store.create(task).await.unwrap();
        let store = FileSystemPhotoStore::new(storage_path.clone(), task_store).await;
        store.create(photo).await.unwrap();

        let test_data = vec![0xFF, 0xD8, 0xFF, 0xE0];
        store.save_data(photo_id, &test_data).await.unwrap();

        let layout = FileSystemLayout::new(storage_path);
        let task_dir = layout.task_dir(catalog_id, task_id);

        let imgs_dir = task_dir.join("imgs");
        assert!(imgs_dir.exists(), "imgs/ directory should exist");
        assert!(imgs_dir.is_dir(), "imgs/ should be a directory");

        let photo_file = imgs_dir.join(photo_id.to_string());
        assert!(photo_file.exists(), "Photo file should exist in imgs/");
        assert!(photo_file.is_file(), "Photo should be a file");

        let wrong_path = task_dir.join(photo_id.to_string());
        assert!(
            !wrong_path.exists(),
            "Photo should NOT be in task root directory"
        );
    }

    #[tokio::test]
    async fn test_find_by_client_id_returns_matching_photos() {
        let ts = create_store().await;
        let task_id = Uuid::new_v4();
        setup_task(&ts, task_id).await;

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
        setup_task(&ts, task_id).await;

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
        setup_task(&ts, task_a).await;
        setup_task(&ts, task_b).await;

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
        setup_task(&ts, task_id).await;

        let photo = Photo::new(task_id, None, "a.jpg".to_string(), 1000);
        ts.store.create(photo).await.unwrap();

        let results = ts.store.find_by_client_id(task_id, "lr:42").await.unwrap();

        assert!(results.is_empty());
    }
}
