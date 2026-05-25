// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! Shared utilities for filesystem storage implementations

use std::io;
use std::path::{Path, PathBuf};
use tracing::{error, warn};
use uuid::Uuid;

use super::FileSystemLayout;

/// Reads a JSON file from `path` and deserializes it into `T`.
///
/// - `io::ErrorKind::NotFound` is returned unchanged when the file does not exist.
/// - Parse failures are returned as `io::ErrorKind::InvalidData`.
pub(super) async fn load_json_from_file<T>(path: &Path) -> io::Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let content = tokio::fs::read_to_string(path).await?;
    serde_json::from_str(&content).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Extracts a UUID from a directory path's last component.
///
/// Returns `None` if the directory name is missing or cannot be parsed as a UUID.
pub(super) fn parse_uuid_from_dir(path: &Path) -> Option<Uuid> {
    path.file_name()
        .and_then(|n| n.to_str())
        .and_then(|s| s.parse().ok())
}

/// Scans project subdirectories for a catalog, logging a warning and returning `None` on failure.
pub(super) async fn try_scan_project_dirs(
    layout: &FileSystemLayout,
    catalog_id: Uuid,
) -> Option<Vec<PathBuf>> {
    match layout.scan_project_dirs(catalog_id).await {
        Ok(dirs) => Some(dirs),
        Err(e) => {
            warn!(
                "Failed to scan project directories for catalog {}: {}",
                catalog_id, e
            );
            None
        }
    }
}

/// Moves a file or directory into the boot-scoped quarantine directory, logging
/// any failure as an error.
///
/// Convenience wrapper around [`quarantine_move`] for use in load paths where
/// quarantine failures must be logged but should not abort the overall scan.
pub(super) async fn try_quarantine(layout: &FileSystemLayout, path: &Path) {
    if let Err(e) = quarantine_move(layout, path).await {
        error!("Failed to quarantine {:?}: {}", path, e);
    }
}

/// Moves a file or directory into the boot-scoped quarantine directory.
///
/// The destination mirrors the original path relative to the storage root, so
/// the storage hierarchy is preserved inside the quarantine directory.
/// Parent directories in the quarantine tree are created as needed.
///
/// Returns the quarantine path on success.
pub(super) async fn quarantine_move(layout: &FileSystemLayout, path: &Path) -> io::Result<PathBuf> {
    let dest = layout.quarantine_path_for(path);
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::rename(path, &dest).await?;
    Ok(dest)
}

/// Writes content to the boot-scoped quarantine path for a given file.
///
/// Unlike [`quarantine_move`], this does not remove the original file — it only
/// creates a quarantine copy. Used for partial recovery (e.g., writing only the
/// bad records extracted from a partially-corrupt file).
/// Parent directories in the quarantine tree are created as needed.
///
/// Returns the quarantine path on success.
pub(super) async fn quarantine_write(
    layout: &FileSystemLayout,
    original_path: &Path,
    content: &[u8],
) -> io::Result<PathBuf> {
    let dest = layout.quarantine_path_for(original_path);
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&dest, content).await?;
    Ok(dest)
}
