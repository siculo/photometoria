// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! Filesystem layout abstraction for storage paths
//!
//! This module provides a centralized definition of the filesystem directory structure
//! for tasks, photos, and jobs. All filesystem-based storage implementations
//! ([`FileSystemTaskStore`], [`FileSystemPhotoStore`], [`FileSystemJobStore`]) use this
//! type to ensure consistency in path generation and directory organization.
//!
//! [`FileSystemTaskStore`]: crate::storage::FileSystemTaskStore
//! [`FileSystemPhotoStore`]: crate::storage::FileSystemPhotoStore
//! [`FileSystemJobStore`]: crate::storage::FileSystemJobStore

use std::io;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::models::{Job, Photo, Task};

/// Encapsulates the filesystem layout logic for all storage entities.
///
/// This type defines the directory structure and file paths for tasks, photos, and jobs.
/// It provides a centralized way to:
/// - Generate paths for storing entities
/// - Create directories as needed
/// - Discover existing entities during load operations
///
/// ## Directory Structure
///
/// ```text
/// {storage_path}/
/// └── tasks/
///     └── {task_id}/
///         ├── task.json          # Task metadata
///         ├── photos.json        # Photos metadata
///         ├── imgs/              # Photo binary data
///         │   ├── {photo_id_1}
///         │   └── {photo_id_2}
///         └── jobs/              # Job metadata
///             ├── {job_id_1}.json
///             └── {job_id_2}.json
/// ```
pub struct FileSystemLayout {
    storage_path: PathBuf,
}

impl FileSystemLayout {
    /// Creates a new layout with the given storage root path.
    pub fn new(storage_path: PathBuf) -> Self {
        Self { storage_path }
    }

    // ========================================================================
    // Task Paths
    // ========================================================================

    /// Returns the directory path for a task.
    ///
    /// Example: `{storage_path}/tasks/{task_id}/`
    pub fn task_dir(&self, task: &Task) -> PathBuf {
        self.task_dir_by_id(task.task_id)
    }

    /// Returns the path to a task's metadata file.
    ///
    /// Example: `{storage_path}/tasks/{task_id}/task.json`
    pub fn task_json_path(&self, task: &Task) -> PathBuf {
        self.task_json_path_by_id(task.task_id)
    }

    // ========================================================================
    // Photo Paths
    // ========================================================================

    /// Returns the directory where photo binary files are stored for a task.
    ///
    /// Example: `{storage_path}/tasks/{task_id}/imgs/`
    pub fn photos_dir(&self, task: &Task) -> PathBuf {
        self.task_dir_by_id(task.task_id).join("imgs")
    }

    /// Returns the path to a photo's metadata file.
    ///
    /// Example: `{storage_path}/tasks/{task_id}/photos.json`
    pub fn photos_json_path(&self, task: &Task) -> PathBuf {
        self.photos_json_path_by_id(task.task_id)
    }

    /// Returns the path to a photo's binary data file.
    ///
    /// Example: `{storage_path}/tasks/{task_id}/imgs/{photo_id}`
    pub fn photo_file_path(&self, photo: &Photo) -> PathBuf {
        self.task_dir_by_id(photo.task_id)
            .join("imgs")
            .join(photo.photo_id.to_string())
    }

    // ========================================================================
    // Job Paths
    // ========================================================================

    /// Returns the directory where job files are stored for a task.
    ///
    /// Example: `{storage_path}/tasks/{task_id}/jobs/`
    pub fn jobs_dir(&self, task: &Task) -> PathBuf {
        self.jobs_dir_by_id(task.task_id)
    }

    /// Returns the path to a job's metadata file.
    ///
    /// Example: `{storage_path}/tasks/{task_id}/jobs/{job_id}.json`
    pub fn job_file_path(&self, job: &Job) -> PathBuf {
        self.jobs_dir_by_id(job.task_id)
            .join(format!("{}.json", job.job_id))
    }

    // ========================================================================
    // Directory Creation Helpers
    // ========================================================================

    /// Ensures the task directory exists, creating it if necessary.
    ///
    /// Returns the path to the created directory.
    pub async fn ensure_task_dir(&self, task: &Task) -> io::Result<PathBuf> {
        let path = self.task_dir(task);
        tokio::fs::create_dir_all(&path).await?;
        Ok(path)
    }

    /// Ensures the photos (imgs) directory exists for a task.
    ///
    /// Returns the path to the created directory.
    pub async fn ensure_photos_dir(&self, task: &Task) -> io::Result<PathBuf> {
        let path = self.photos_dir(task);
        tokio::fs::create_dir_all(&path).await?;
        Ok(path)
    }

    /// Ensures the jobs directory exists for a task.
    ///
    /// Returns the path to the created directory.
    pub async fn ensure_jobs_dir(&self, task: &Task) -> io::Result<PathBuf> {
        let path = self.jobs_dir(task);
        tokio::fs::create_dir_all(&path).await?;
        Ok(path)
    }

    // ========================================================================
    // Discovery Methods (for loading from filesystem)
    // ========================================================================

    /// Scans the filesystem and returns paths to all task.json files.
    ///
    /// Used by TaskStore during initialization to load existing tasks.
    /// Returns paths to all task.json files found in the tasks directory.
    pub async fn scan_task_json_files(&self) -> io::Result<Vec<PathBuf>> {
        let tasks_dir = self.tasks_root();

        if !tasks_dir.exists() {
            return Ok(Vec::new());
        }

        let mut result = Vec::new();
        let mut entries = tokio::fs::read_dir(&tasks_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let task_json = path.join("task.json");
            if task_json.exists() {
                result.push(task_json);
            }
        }

        Ok(result)
    }

    /// Returns the path to photos.json for a given task ID.
    ///
    /// Used during loading operations.
    pub fn photos_json_path_by_id(&self, task_id: Uuid) -> PathBuf {
        self.task_dir_by_id(task_id).join("photos.json")
    }

    /// Checks if a photos.json file exists for a given task ID.
    ///
    /// Returns Some(path) if the file exists, None otherwise.
    pub fn photos_json_exists_by_id(&self, task_id: Uuid) -> Option<PathBuf> {
        let path = self.photos_json_path_by_id(task_id);
        if path.exists() { Some(path) } else { None }
    }

    /// Scans the jobs directory for a task and returns paths to all job JSON files.
    ///
    /// Used by JobStore during initialization to load existing jobs for a specific task.
    /// Returns empty vec if the jobs directory doesn't exist.
    pub async fn scan_job_files_by_id(&self, task_id: Uuid) -> io::Result<Vec<PathBuf>> {
        let jobs_dir = self.jobs_dir_by_id(task_id);

        if !jobs_dir.exists() {
            return Ok(Vec::new());
        }

        let mut result = Vec::new();
        let mut entries = tokio::fs::read_dir(&jobs_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                result.push(path);
            }
        }

        Ok(result)
    }

    // ========================================================================
    // Path Helpers by ID (for loading operations)
    // ========================================================================

    /// Returns the directory path for a task by ID.
    ///
    /// Used during loading when we only have the task ID from the directory name.
    pub fn task_dir_by_id(&self, task_id: Uuid) -> PathBuf {
        self.tasks_root().join(task_id.to_string())
    }

    /// Returns the path to a task's metadata file by ID.
    ///
    /// Used during loading operations.
    pub fn task_json_path_by_id(&self, task_id: Uuid) -> PathBuf {
        self.task_dir_by_id(task_id).join("task.json")
    }

    /// Returns the jobs directory path for a task by ID.
    ///
    /// Used during loading operations.
    pub fn jobs_dir_by_id(&self, task_id: Uuid) -> PathBuf {
        self.task_dir_by_id(task_id).join("jobs")
    }

    // ========================================================================
    // Utility Methods
    // ========================================================================

    /// Returns the root storage path.
    pub fn root(&self) -> &Path {
        &self.storage_path
    }

    /// Returns the tasks root directory path.
    ///
    /// Example: `{storage_path}/tasks/`
    pub fn tasks_root(&self) -> PathBuf {
        self.storage_path.join("tasks")
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_task() -> Task {
        Task::new("Test task".to_string())
    }

    fn create_test_photo(task_id: Uuid) -> Photo {
        Photo::new(task_id, "test.jpg".to_string(), 1000)
    }

    fn create_test_job(task_id: Uuid) -> Job {
        Job::new(task_id, "qwen3-vl:8b".to_string(), vec![Uuid::new_v4()])
    }

    #[test]
    fn test_new() {
        let storage_path = PathBuf::from("/tmp/storage");
        let layout = FileSystemLayout::new(storage_path.clone());
        assert_eq!(layout.root(), storage_path.as_path());
    }

    #[test]
    fn test_tasks_root() {
        let storage_path = PathBuf::from("/tmp/storage");
        let layout = FileSystemLayout::new(storage_path.clone());
        assert_eq!(layout.tasks_root(), storage_path.join("tasks"));
    }

    #[test]
    fn test_task_paths() {
        let storage_path = PathBuf::from("/tmp/storage");
        let layout = FileSystemLayout::new(storage_path.clone());
        let task = create_test_task();

        let task_dir = layout.task_dir(&task);
        assert_eq!(
            task_dir,
            storage_path.join("tasks").join(task.task_id.to_string())
        );

        let task_json = layout.task_json_path(&task);
        assert_eq!(task_json, task_dir.join("task.json"));
    }

    #[test]
    fn test_task_paths_by_id() {
        let storage_path = PathBuf::from("/tmp/storage");
        let layout = FileSystemLayout::new(storage_path.clone());
        let task_id = Uuid::new_v4();

        let task_dir = layout.task_dir_by_id(task_id);
        assert_eq!(
            task_dir,
            storage_path.join("tasks").join(task_id.to_string())
        );

        let task_json = layout.task_json_path_by_id(task_id);
        assert_eq!(task_json, task_dir.join("task.json"));
    }

    #[test]
    fn test_photo_paths() {
        let storage_path = PathBuf::from("/tmp/storage");
        let layout = FileSystemLayout::new(storage_path.clone());
        let task = create_test_task();
        let photo = create_test_photo(task.task_id);

        let photos_dir = layout.photos_dir(&task);
        assert_eq!(
            photos_dir,
            storage_path
                .join("tasks")
                .join(task.task_id.to_string())
                .join("imgs")
        );

        let photos_json = layout.photos_json_path(&task);
        assert_eq!(
            photos_json,
            storage_path
                .join("tasks")
                .join(task.task_id.to_string())
                .join("photos.json")
        );

        let photo_file = layout.photo_file_path(&photo);
        assert_eq!(photo_file, photos_dir.join(photo.photo_id.to_string()));
    }

    #[test]
    fn test_job_paths() {
        let storage_path = PathBuf::from("/tmp/storage");
        let layout = FileSystemLayout::new(storage_path.clone());
        let task = create_test_task();
        let job = create_test_job(task.task_id);

        let jobs_dir = layout.jobs_dir(&task);
        assert_eq!(
            jobs_dir,
            storage_path
                .join("tasks")
                .join(task.task_id.to_string())
                .join("jobs")
        );

        let job_file = layout.job_file_path(&job);
        assert_eq!(job_file, jobs_dir.join(format!("{}.json", job.job_id)));
    }

    #[tokio::test]
    async fn test_ensure_task_dir() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let layout = FileSystemLayout::new(temp_dir.path().to_path_buf());
        let task = create_test_task();

        let path = layout.ensure_task_dir(&task).await.unwrap();
        assert!(path.exists());
        assert!(path.is_dir());
        assert_eq!(path, layout.task_dir(&task));
    }

    #[tokio::test]
    async fn test_ensure_photos_dir() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let layout = FileSystemLayout::new(temp_dir.path().to_path_buf());
        let task = create_test_task();

        let path = layout.ensure_photos_dir(&task).await.unwrap();
        assert!(path.exists());
        assert!(path.is_dir());
        assert_eq!(path, layout.photos_dir(&task));
    }

    #[tokio::test]
    async fn test_ensure_jobs_dir() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let layout = FileSystemLayout::new(temp_dir.path().to_path_buf());
        let task = create_test_task();

        let path = layout.ensure_jobs_dir(&task).await.unwrap();
        assert!(path.exists());
        assert!(path.is_dir());
        assert_eq!(path, layout.jobs_dir(&task));
    }

    #[tokio::test]
    async fn test_scan_task_json_files_empty() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let layout = FileSystemLayout::new(temp_dir.path().to_path_buf());

        let files = layout.scan_task_json_files().await.unwrap();
        assert!(files.is_empty());
    }

    #[tokio::test]
    async fn test_scan_task_json_files() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let layout = FileSystemLayout::new(temp_dir.path().to_path_buf());

        // Create some task directories with task.json files
        let task1 = create_test_task();
        let task2 = create_test_task();
        let task3 = create_test_task();

        layout.ensure_task_dir(&task1).await.unwrap();
        layout.ensure_task_dir(&task2).await.unwrap();
        layout.ensure_task_dir(&task3).await.unwrap();

        // Write task.json files
        tokio::fs::write(layout.task_json_path(&task1), b"{}")
            .await
            .unwrap();
        tokio::fs::write(layout.task_json_path(&task2), b"{}")
            .await
            .unwrap();
        tokio::fs::write(layout.task_json_path(&task3), b"{}")
            .await
            .unwrap();

        let files = layout.scan_task_json_files().await.unwrap();
        assert_eq!(files.len(), 3);

        // Verify all paths exist
        for file in &files {
            assert!(file.exists());
            assert!(file.ends_with("task.json"));
        }
    }

    #[tokio::test]
    async fn test_photos_json_exists_by_id() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let layout = FileSystemLayout::new(temp_dir.path().to_path_buf());
        let task = create_test_task();

        // Initially doesn't exist
        assert!(layout.photos_json_exists_by_id(task.task_id).is_none());

        // Create the directory and file
        layout.ensure_task_dir(&task).await.unwrap();
        tokio::fs::write(layout.photos_json_path(&task), b"[]")
            .await
            .unwrap();

        // Now it exists
        let path = layout.photos_json_exists_by_id(task.task_id);
        assert!(path.is_some());
        assert_eq!(path.unwrap(), layout.photos_json_path(&task));
    }

    #[tokio::test]
    async fn test_scan_job_files_empty() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let layout = FileSystemLayout::new(temp_dir.path().to_path_buf());
        let task = create_test_task();

        // Jobs directory doesn't exist yet
        let files = layout.scan_job_files_by_id(task.task_id).await.unwrap();
        assert!(files.is_empty());
    }

    #[tokio::test]
    async fn test_scan_job_files() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let layout = FileSystemLayout::new(temp_dir.path().to_path_buf());
        let task = create_test_task();

        // Create jobs directory
        layout.ensure_jobs_dir(&task).await.unwrap();

        // Create some job files
        let job1 = create_test_job(task.task_id);
        let job2 = create_test_job(task.task_id);
        let job3 = create_test_job(task.task_id);

        tokio::fs::write(layout.job_file_path(&job1), b"{}")
            .await
            .unwrap();
        tokio::fs::write(layout.job_file_path(&job2), b"{}")
            .await
            .unwrap();
        tokio::fs::write(layout.job_file_path(&job3), b"{}")
            .await
            .unwrap();

        let files = layout.scan_job_files_by_id(task.task_id).await.unwrap();
        assert_eq!(files.len(), 3);

        // Verify all paths exist and are JSON files
        for file in &files {
            assert!(file.exists());
            assert_eq!(file.extension().and_then(|s| s.to_str()), Some("json"));
        }
    }

    #[tokio::test]
    async fn test_scan_job_files_ignores_non_json() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let layout = FileSystemLayout::new(temp_dir.path().to_path_buf());
        let task = create_test_task();

        // Create jobs directory
        layout.ensure_jobs_dir(&task).await.unwrap();

        // Create a job file and a non-JSON file
        let job = create_test_job(task.task_id);
        tokio::fs::write(layout.job_file_path(&job), b"{}")
            .await
            .unwrap();
        tokio::fs::write(layout.jobs_dir(&task).join("README.txt"), b"test")
            .await
            .unwrap();

        let files = layout.scan_job_files_by_id(task.task_id).await.unwrap();
        assert_eq!(files.len(), 1); // Only the JSON file
    }
}
