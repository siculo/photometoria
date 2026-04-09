// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! Shared utilities for filesystem storage implementations

use std::io;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use super::FileSystemLayout;

/// Extracts a UUID from a directory path's last component.
///
/// Returns `None` if the directory name is missing or cannot be parsed as a UUID.
pub(super) fn parse_uuid_from_dir(path: &Path) -> Option<Uuid> {
    path.file_name()
        .and_then(|n| n.to_str())
        .and_then(|s| s.parse().ok())
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
