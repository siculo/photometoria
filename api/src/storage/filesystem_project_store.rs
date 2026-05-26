// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! Filesystem-backed implementation of ProjectStore
//!
//! This module provides a thread-safe implementation of the ProjectStore trait
//! with full persistence to the filesystem. Project metadata is stored as JSON files.

use async_trait::async_trait;
use chrono::Local;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::models::Project;

use super::utils::{
    load_json_from_file, parse_uuid_from_dir, try_quarantine, try_scan_project_dirs,
};
use super::{FileSystemLayout, ProjectStore, ProjectStoreError, ProjectStoreResult};

fn sorted_projects(iter: impl Iterator<Item = Project>) -> Vec<Project> {
    let mut projects: Vec<Project> = iter.collect();
    projects.sort_by_key(|p| p.created_at);
    projects
}

/// Filesystem-backed implementation of ProjectStore with full persistence.
///
/// This implementation stores project metadata both in memory (using DashMap for
/// fast concurrent access) and on the filesystem (as JSON files for persistence).
///
/// ## Characteristics
///
/// - **Thread-safe**: Supports concurrent access from multiple Tokio tasks
/// - **Lock-free reads**: Get operations don't acquire locks
/// - **Fine-grained locking**: Writes lock only the specific shard
/// - **Full persistence**: Project metadata survives server restarts
/// - **Filesystem integration**: Creates/removes project directories and metadata files
///
/// ## Persistence Strategy
///
/// - Metadata is written to disk after each create/update operation
/// - On startup, all existing task.json files are loaded into memory
/// - Deleting a project removes both the in-memory entry and the entire directory
///
/// For details on the filesystem layout, see [`FileSystemLayout`]
pub struct FileSystemProjectStore {
    projects: Arc<DashMap<Uuid, Project>>,
    layout: FileSystemLayout,
}

impl FileSystemProjectStore {
    /// Creates a new filesystem-backed project store.
    ///
    /// This constructor loads all existing projects from the filesystem.
    /// Any errors during loading are logged but don't prevent startup.
    ///
    /// # Arguments
    /// * `storage_path` - Base path for storing project directories
    pub async fn new(storage_path: PathBuf) -> Self {
        Self::new_with_boot_ts(
            storage_path,
            Local::now().format("%Y%m%d_%H%M%S").to_string(),
        )
        .await
    }

    /// Creates a new filesystem-backed project store with an explicit boot timestamp.
    ///
    /// Use this when multiple stores must share the same quarantine directory for
    /// a single server boot.
    pub async fn new_with_boot_ts(storage_path: PathBuf, boot_ts: String) -> Self {
        let store = Self {
            projects: Arc::new(DashMap::new()),
            layout: FileSystemLayout::new_with_boot_ts(storage_path, boot_ts),
        };
        store.load_all().await;
        store
    }

    /// Loads all projects from the filesystem into memory.
    ///
    /// Scans the catalog hierarchy: `catalogs/{catalog_id}/tasks/{project_id}/task.json`
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
            let (n_loaded, n_errors) = self.load_catalog_projects(catalog_id).await;
            loaded += n_loaded;
            errors += n_errors;
        }

        info!(
            "Loaded {} projects from filesystem ({} errors)",
            loaded, errors
        );
    }

    /// Loads all projects for a single catalog from the filesystem.
    ///
    /// Returns the count of successfully loaded projects and errors encountered.
    /// Project directories with corrupt, inconsistent, or missing task.json are
    /// moved to the boot-scoped quarantine directory.
    async fn load_catalog_projects(&self, catalog_id: Uuid) -> (usize, usize) {
        let Some(project_dirs) = try_scan_project_dirs(&self.layout, catalog_id).await else {
            return (0, 1);
        };

        let mut loaded = 0;
        let mut errors = 0;

        for project_dir in project_dirs {
            let Some(dir_id) = parse_uuid_from_dir(&project_dir) else {
                warn!("Skipping non-UUID project directory: {:?}", project_dir);
                continue;
            };

            let project_json = project_dir.join("task.json");

            let project = match load_json_from_file::<Project>(&project_json).await {
                Ok(p) => p,
                Err(e) if e.kind() == ErrorKind::NotFound => {
                    warn!("Orphan project directory (no task.json): {:?}", project_dir);
                    try_quarantine(&self.layout, &project_dir).await;
                    errors += 1;
                    continue;
                }
                Err(e) => {
                    error!(
                        "Corrupt task.json in {:?}: {} — quarantining project directory",
                        project_dir, e
                    );
                    try_quarantine(&self.layout, &project_dir).await;
                    errors += 1;
                    continue;
                }
            };

            if dir_id != project.project_id {
                error!(
                    "Inconsistent task.json in {:?}: project_id {} does not match directory {} — quarantining",
                    project_dir, project.project_id, dir_id
                );
                try_quarantine(&self.layout, &project_dir).await;
                errors += 1;
                continue;
            }

            self.projects.insert(project.project_id, project);
            loaded += 1;
        }

        (loaded, errors)
    }

    /// Saves a project's metadata to the filesystem atomically.
    ///
    /// Writes to a temporary file first, then renames to the final path.
    /// This ensures the target file is never left in a partially-written state.
    async fn save_project_to_file(&self, project: &Project) -> ProjectStoreResult<()> {
        let path = self
            .layout
            .project_json_path(project.catalog_id, project.project_id);
        let tmp_path = path.with_extension("tmp");
        let content = serde_json::to_string_pretty(project).map_err(|e| {
            ProjectStoreError::StorageError(format!("Failed to serialize project: {}", e))
        })?;

        tokio::fs::write(&tmp_path, &content).await.map_err(|e| {
            error!(
                "Failed to write temporary project file {:?}: {}",
                tmp_path, e
            );
            ProjectStoreError::StorageError(format!("Failed to write project file: {}", e))
        })?;

        tokio::fs::rename(&tmp_path, &path).await.map_err(|e| {
            error!("Failed to rename {:?} -> {:?}: {}", tmp_path, path, e);
            ProjectStoreError::StorageError(format!(
                "Failed to atomically save project file: {}",
                e
            ))
        })?;

        debug!("Saved project metadata to {:?}", path);
        Ok(())
    }
}

#[async_trait]
impl ProjectStore for FileSystemProjectStore {
    async fn create(&self, project: Project) -> ProjectStoreResult<Project> {
        let project_id = project.project_id;

        debug!("Attempting to create project: {}", project_id);

        match self.projects.entry(project_id) {
            Entry::Occupied(_) => Err(ProjectStoreError::AlreadyExists(project_id)),
            Entry::Vacant(entry) => {
                let project_dir = self
                    .layout
                    .ensure_project_dir(project.catalog_id, project.project_id)
                    .await
                    .map_err(|e| {
                        error!("Failed to create project directory: {}", e);
                        ProjectStoreError::StorageError(format!(
                            "Failed to create project directory: {}",
                            e
                        ))
                    })?;
                debug!("Created project directory: {:?}", project_dir);

                self.save_project_to_file(&project).await?;

                entry.insert(project.clone());
                info!(
                    "Project created successfully: {} (context: '{}')",
                    project_id, project.context
                );
                Ok(project)
            }
        }
    }

    async fn get(&self, project_id: Uuid) -> ProjectStoreResult<Option<Project>> {
        debug!("Retrieving project: {}", project_id);

        Ok(self
            .projects
            .get(&project_id)
            .map(|entry| entry.value().clone()))
    }

    async fn list(&self) -> ProjectStoreResult<Vec<Project>> {
        debug!("Listing all projects");

        let projects = sorted_projects(self.projects.iter().map(|e| e.value().clone()));

        info!("Listed {} projects", projects.len());
        Ok(projects)
    }

    async fn update(&self, project: Project) -> ProjectStoreResult<Project> {
        let project_id = project.project_id;

        debug!("Updating project: {}", project_id);

        match self.projects.get_mut(&project_id) {
            Some(mut entry) => {
                self.save_project_to_file(&project).await?;

                let old_context = entry.context.clone();
                *entry = project.clone();
                info!(
                    "Project updated successfully: {} (context: '{}' -> '{}')",
                    project_id, old_context, project.context
                );
                Ok(project)
            }
            None => Err(ProjectStoreError::NotFound(project_id)),
        }
    }

    async fn delete(&self, project_id: Uuid) -> ProjectStoreResult<()> {
        debug!("Deleting project: {}", project_id);

        let (_, project) = match self.projects.remove(&project_id) {
            Some(entry) => entry,
            None => return Err(ProjectStoreError::NotFound(project_id)),
        };

        let project_dir = self
            .layout
            .project_dir(project.catalog_id, project.project_id);
        match tokio::fs::remove_dir_all(&project_dir).await {
            Ok(()) => debug!("Removed project directory: {:?}", project_dir),
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => {
                error!(
                    "Failed to remove project directory {:?}: {}",
                    project_dir, e
                );
                self.projects.insert(project_id, project);
                return Err(ProjectStoreError::StorageError(format!(
                    "Failed to remove project directory: {}",
                    e
                )));
            }
        }

        info!(
            "Project deleted successfully: {} (context: '{}')",
            project_id, project.context
        );
        Ok(())
    }

    async fn exists(&self, project_id: Uuid) -> ProjectStoreResult<bool> {
        debug!("Checking if project exists: {}", project_id);

        Ok(self.projects.contains_key(&project_id))
    }

    async fn count(&self) -> ProjectStoreResult<usize> {
        debug!("Counting projects");

        Ok(self.projects.len())
    }

    async fn list_by_catalog(&self, catalog_id: Uuid) -> ProjectStoreResult<Vec<Project>> {
        debug!("Listing projects for catalog: {}", catalog_id);

        let projects = sorted_projects(
            self.projects
                .iter()
                .filter(|e| e.value().catalog_id == catalog_id)
                .map(|e| e.value().clone()),
        );

        info!(
            "Listed {} projects for catalog {}",
            projects.len(),
            catalog_id
        );
        Ok(projects)
    }

    async fn find_by_name(
        &self,
        catalog_id: Uuid,
        name: &str,
    ) -> ProjectStoreResult<Option<Project>> {
        debug!(
            "Finding project by name '{}' in catalog {}",
            name, catalog_id
        );

        Ok(self
            .projects
            .iter()
            .find(|entry| entry.value().catalog_id == catalog_id && entry.value().name == name)
            .map(|entry| entry.value().clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Duration, Utc};
    use tempfile::TempDir;
    use uuid::Uuid;

    struct TestStore {
        store: FileSystemProjectStore,
        _temp_dir: TempDir,
    }

    async fn create_store() -> TestStore {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let store = FileSystemProjectStore::new(temp_dir.path().to_path_buf()).await;
        TestStore {
            store,
            _temp_dir: temp_dir,
        }
    }

    fn create_test_project_with_timestamp(name: &str, timestamp: DateTime<Utc>) -> Project {
        Project {
            project_id: Uuid::new_v4(),
            catalog_id: Uuid::new_v4(),
            name: name.to_string(),
            context: format!("context for {}", name),
            created_at: timestamp,
        }
    }

    fn create_test_project(name: &str) -> Project {
        Project::new(
            Uuid::new_v4(),
            name.to_string(),
            format!("context for {}", name),
        )
    }

    #[tokio::test]
    async fn test_create_project() {
        let ts = create_store().await;
        let project = create_test_project("vacation in SF");

        let result = ts.store.create(project.clone()).await;

        assert!(result.is_ok());
        let created = result.unwrap();
        assert_eq!(created.project_id, project.project_id);
        assert_eq!(created.context, project.context);

        let exists = ts.store.exists(project.project_id).await.unwrap();
        assert!(exists);

        assert!(
            ts.store
                .layout
                .project_dir(project.catalog_id, project.project_id)
                .exists()
        );

        assert!(
            ts.store
                .layout
                .project_json_path(project.catalog_id, project.project_id)
                .exists()
        );
    }

    #[tokio::test]
    async fn test_create_duplicate_fails() {
        let ts = create_store().await;
        let project = create_test_project("vacation in SF");

        ts.store.create(project.clone()).await.unwrap();

        let result = ts.store.create(project.clone()).await;

        assert!(result.is_err());
        match result {
            Err(ProjectStoreError::AlreadyExists(id)) => {
                assert_eq!(id, project.project_id);
            }
            _ => panic!("Expected AlreadyExists error"),
        }
    }

    #[tokio::test]
    async fn test_get_existing_project() {
        let ts = create_store().await;
        let project = create_test_project("vacation in SF");

        ts.store.create(project.clone()).await.unwrap();

        let result = ts.store.get(project.project_id).await.unwrap();

        assert!(result.is_some());
        let retrieved = result.unwrap();
        assert_eq!(retrieved.project_id, project.project_id);
        assert_eq!(retrieved.context, project.context);
        assert_eq!(retrieved.created_at, project.created_at);
    }

    #[tokio::test]
    async fn test_get_nonexistent_project() {
        let ts = create_store().await;

        let result = ts.store.get(Uuid::new_v4()).await.unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_list_projects_ordered_by_created_at() {
        let ts = create_store().await;

        let now = Utc::now();
        let project1 = create_test_project_with_timestamp("First", now);
        let project2 = create_test_project_with_timestamp("Second", now + Duration::seconds(1));
        let project3 = create_test_project_with_timestamp("Third", now + Duration::seconds(2));

        ts.store.create(project2.clone()).await.unwrap();
        ts.store.create(project1.clone()).await.unwrap();
        ts.store.create(project3.clone()).await.unwrap();

        let projects = ts.store.list().await.unwrap();

        assert_eq!(projects.len(), 3);
        assert_eq!(projects[0].name, "First");
        assert_eq!(projects[1].name, "Second");
        assert_eq!(projects[2].name, "Third");
        assert_eq!(projects[0].project_id, project1.project_id);
        assert_eq!(projects[1].project_id, project2.project_id);
        assert_eq!(projects[2].project_id, project3.project_id);
    }

    #[tokio::test]
    async fn test_update_project() {
        let ts = create_store().await;
        let project = create_test_project("vacation in SF");

        ts.store.create(project.clone()).await.unwrap();

        let mut updated_project = project.clone();
        updated_project.context = "Updated context".to_string();

        let result = ts.store.update(updated_project.clone()).await;

        assert!(result.is_ok());
        let updated = result.unwrap();
        assert_eq!(updated.context, "Updated context");

        let retrieved = ts.store.get(project.project_id).await.unwrap().unwrap();
        assert_eq!(retrieved.context, "Updated context");
    }

    #[tokio::test]
    async fn test_update_nonexistent_fails() {
        let ts = create_store().await;
        let project = create_test_project("vacation in SF");

        let result = ts.store.update(project.clone()).await;

        assert!(result.is_err());
        match result {
            Err(ProjectStoreError::NotFound(id)) => {
                assert_eq!(id, project.project_id);
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_delete_project() {
        let ts = create_store().await;
        let project = create_test_project("vacation in SF");

        ts.store.create(project.clone()).await.unwrap();
        let project_dir = ts
            .store
            .layout
            .project_dir(project.catalog_id, project.project_id);
        assert!(project_dir.exists());

        let result = ts.store.delete(project.project_id).await;

        assert!(result.is_ok());

        let retrieved = ts.store.get(project.project_id).await.unwrap();
        assert!(retrieved.is_none());

        assert!(!project_dir.exists());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_fails() {
        let ts = create_store().await;

        let nonexistent_id = Uuid::new_v4();
        let result = ts.store.delete(nonexistent_id).await;

        assert!(result.is_err());
        match result {
            Err(ProjectStoreError::NotFound(id)) => {
                assert_eq!(id, nonexistent_id);
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_exists() {
        let ts = create_store().await;
        let project = create_test_project("vacation in SF");

        let exists = ts.store.exists(project.project_id).await.unwrap();
        assert!(!exists);

        ts.store.create(project.clone()).await.unwrap();

        let exists = ts.store.exists(project.project_id).await.unwrap();
        assert!(exists);

        let exists = ts.store.exists(Uuid::new_v4()).await.unwrap();
        assert!(!exists);
    }

    #[tokio::test]
    async fn test_count() {
        let ts = create_store().await;

        let count = ts.store.count().await.unwrap();
        assert_eq!(count, 0);

        let project1 = create_test_project("First");
        let project2 = create_test_project("Second");
        let project3 = create_test_project("Third");

        ts.store.create(project1.clone()).await.unwrap();
        ts.store.create(project2.clone()).await.unwrap();
        ts.store.create(project3.clone()).await.unwrap();

        let count = ts.store.count().await.unwrap();
        assert_eq!(count, 3);

        ts.store.delete(project1.project_id).await.unwrap();

        let count = ts.store.count().await.unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn test_persistence_survives_reload() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();

        let project1 = create_test_project("First project");
        let project2 = create_test_project("Second project");
        let project1_id = project1.project_id;
        let project2_id = project2.project_id;

        {
            let store = FileSystemProjectStore::new(storage_path.clone()).await;
            store.create(project1).await.unwrap();
            store.create(project2).await.unwrap();
            assert_eq!(store.count().await.unwrap(), 2);
        }

        let store = FileSystemProjectStore::new(storage_path).await;

        assert_eq!(store.count().await.unwrap(), 2);
        assert!(store.exists(project1_id).await.unwrap());
        assert!(store.exists(project2_id).await.unwrap());

        let loaded_project1 = store.get(project1_id).await.unwrap().unwrap();
        assert_eq!(loaded_project1.name, "First project");
    }

    #[tokio::test]
    async fn test_update_persists() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();

        let project = create_test_project("Original context");
        let project_id = project.project_id;

        {
            let store = FileSystemProjectStore::new(storage_path.clone()).await;
            store.create(project.clone()).await.unwrap();

            let mut updated = project.clone();
            updated.context = "Updated context".to_string();
            store.update(updated).await.unwrap();
        }

        let store = FileSystemProjectStore::new(storage_path).await;
        let loaded = store.get(project_id).await.unwrap().unwrap();
        assert_eq!(loaded.context, "Updated context");
    }

    /// Simulates a filesystem failure during delete by making the project directory
    /// non-removable, then verifies the project is rolled back into memory.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_delete_memory_rolled_back_when_filesystem_fails() {
        use std::os::unix::fs::PermissionsExt;

        let ts = create_store().await;
        let project = create_test_project("to delete");
        ts.store.create(project.clone()).await.unwrap();

        let project_dir = ts
            .store
            .layout
            .project_dir(project.catalog_id, project.project_id);
        let parent_dir = project_dir.parent().unwrap();
        std::fs::set_permissions(parent_dir, std::fs::Permissions::from_mode(0o555)).unwrap();

        let result = ts.store.delete(project.project_id).await;

        std::fs::set_permissions(parent_dir, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(result.is_err());
        let in_memory = ts.store.get(project.project_id).await.unwrap();
        assert!(in_memory.is_some());
    }

    /// Simulates a failure writing the `.tmp` file by making the project directory
    /// non-writable, then verifies the in-memory project is unchanged.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_update_memory_unchanged_when_tmp_write_fails() {
        use std::os::unix::fs::PermissionsExt;

        let ts = create_store().await;
        let project = create_test_project("original");
        ts.store.create(project.clone()).await.unwrap();

        let project_dir = ts
            .store
            .layout
            .project_dir(project.catalog_id, project.project_id);
        std::fs::set_permissions(&project_dir, std::fs::Permissions::from_mode(0o555)).unwrap();

        let mut updated = project.clone();
        updated.context = "updated context".to_string();
        let result = ts.store.update(updated).await;

        std::fs::set_permissions(&project_dir, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(result.is_err());
        let in_memory = ts.store.get(project.project_id).await.unwrap().unwrap();
        assert_eq!(in_memory.context, project.context);
    }

    /// Simulates a rename failure by replacing `task.json` with a directory,
    /// then verifies the in-memory project is unchanged.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_update_memory_unchanged_when_rename_fails() {
        let ts = create_store().await;
        let project = create_test_project("original");
        ts.store.create(project.clone()).await.unwrap();

        let project_json = ts
            .store
            .layout
            .project_json_path(project.catalog_id, project.project_id);
        tokio::fs::remove_file(&project_json).await.unwrap();
        tokio::fs::create_dir(&project_json).await.unwrap();

        let mut updated = project.clone();
        updated.context = "updated context".to_string();
        let result = ts.store.update(updated).await;

        tokio::fs::remove_dir(&project_json).await.unwrap();

        assert!(result.is_err());
        let in_memory = ts.store.get(project.project_id).await.unwrap().unwrap();
        assert_eq!(in_memory.context, project.context);
    }

    #[tokio::test]
    async fn test_list_by_catalog_returns_only_matching_projects() {
        let ts = create_store().await;
        let catalog_a = Uuid::new_v4();
        let catalog_b = Uuid::new_v4();

        let project1 = Project::new(catalog_a, "Project A1".to_string(), "ctx".to_string());
        let project2 = Project::new(catalog_a, "Project A2".to_string(), "ctx".to_string());
        let project3 = Project::new(catalog_b, "Project B1".to_string(), "ctx".to_string());

        ts.store.create(project1).await.unwrap();
        ts.store.create(project2).await.unwrap();
        ts.store.create(project3).await.unwrap();

        let catalog_a_projects = ts.store.list_by_catalog(catalog_a).await.unwrap();
        assert_eq!(catalog_a_projects.len(), 2);
        assert!(catalog_a_projects.iter().all(|p| p.catalog_id == catalog_a));

        let catalog_b_projects = ts.store.list_by_catalog(catalog_b).await.unwrap();
        assert_eq!(catalog_b_projects.len(), 1);
        assert_eq!(catalog_b_projects[0].name, "Project B1");
    }

    #[tokio::test]
    async fn test_list_by_catalog_empty_for_unknown_catalog() {
        let ts = create_store().await;

        let projects = ts.store.list_by_catalog(Uuid::new_v4()).await.unwrap();
        assert!(projects.is_empty());
    }

    #[tokio::test]
    async fn test_find_by_name_scoped_to_catalog() {
        let ts = create_store().await;
        let catalog_a = Uuid::new_v4();
        let catalog_b = Uuid::new_v4();

        let project_a = Project::new(catalog_a, "Shared Name".to_string(), "ctx a".to_string());
        let project_b = Project::new(catalog_b, "Shared Name".to_string(), "ctx b".to_string());

        ts.store.create(project_a.clone()).await.unwrap();
        ts.store.create(project_b.clone()).await.unwrap();

        let found_a = ts
            .store
            .find_by_name(catalog_a, "Shared Name")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found_a.catalog_id, catalog_a);
        assert_eq!(found_a.context, "ctx a");

        let found_b = ts
            .store
            .find_by_name(catalog_b, "Shared Name")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found_b.catalog_id, catalog_b);
        assert_eq!(found_b.context, "ctx b");
    }

    #[tokio::test]
    async fn test_find_by_name_not_found_in_different_catalog() {
        let ts = create_store().await;
        let catalog_a = Uuid::new_v4();
        let catalog_b = Uuid::new_v4();

        let project = Project::new(catalog_a, "Only in A".to_string(), "ctx".to_string());
        ts.store.create(project).await.unwrap();

        let result = ts.store.find_by_name(catalog_b, "Only in A").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_delete_removes_from_filesystem() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();

        let project = create_test_project("Project to delete");
        let project_id = project.project_id;

        {
            let store = FileSystemProjectStore::new(storage_path.clone()).await;
            store.create(project).await.unwrap();
            store.delete(project_id).await.unwrap();
        }

        let store = FileSystemProjectStore::new(storage_path).await;
        assert_eq!(store.count().await.unwrap(), 0);
        assert!(!store.exists(project_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_load_quarantines_corrupt_task_json() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();

        let catalog_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let project_dir = storage_path
            .join("catalogs")
            .join(catalog_id.to_string())
            .join("tasks")
            .join(project_id.to_string());
        tokio::fs::create_dir_all(&project_dir).await.unwrap();
        tokio::fs::write(project_dir.join("task.json"), b"not valid json")
            .await
            .unwrap();

        let store = FileSystemProjectStore::new(storage_path).await;

        assert_eq!(store.count().await.unwrap(), 0);
        assert!(!project_dir.exists());
        assert!(store.layout.quarantine_path_for(&project_dir).exists());
    }

    #[tokio::test]
    async fn test_load_quarantines_inconsistent_project_id() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();

        let catalog_id = Uuid::new_v4();
        let dir_project_id = Uuid::new_v4();
        let json_project_id = Uuid::new_v4();

        let project_dir = storage_path
            .join("catalogs")
            .join(catalog_id.to_string())
            .join("tasks")
            .join(dir_project_id.to_string());
        tokio::fs::create_dir_all(&project_dir).await.unwrap();

        let project = Project {
            project_id: json_project_id,
            catalog_id,
            name: "test".to_string(),
            context: "ctx".to_string(),
            created_at: Utc::now(),
        };
        tokio::fs::write(
            project_dir.join("task.json"),
            serde_json::to_string(&project).unwrap(),
        )
        .await
        .unwrap();

        let store = FileSystemProjectStore::new(storage_path).await;

        assert_eq!(store.count().await.unwrap(), 0);
        assert!(!project_dir.exists());
        assert!(store.layout.quarantine_path_for(&project_dir).exists());
    }

    #[tokio::test]
    async fn test_load_quarantines_orphan_project_dir() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();

        let catalog_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let project_dir = storage_path
            .join("catalogs")
            .join(catalog_id.to_string())
            .join("tasks")
            .join(project_id.to_string());
        tokio::fs::create_dir_all(&project_dir).await.unwrap();

        let store = FileSystemProjectStore::new(storage_path).await;

        assert_eq!(store.count().await.unwrap(), 0);
        assert!(!project_dir.exists());
        assert!(store.layout.quarantine_path_for(&project_dir).exists());
    }

    #[tokio::test]
    async fn test_load_valid_project_not_quarantined() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();

        let project = create_test_project("valid project");

        {
            let store = FileSystemProjectStore::new(storage_path.clone()).await;
            store.create(project.clone()).await.unwrap();
        }

        let store = FileSystemProjectStore::new(storage_path).await;

        assert_eq!(store.count().await.unwrap(), 1);
        assert!(store.exists(project.project_id).await.unwrap());
        assert!(!store.layout.quarantine_dir().exists());
    }
}
