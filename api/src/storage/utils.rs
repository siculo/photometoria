// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! Shared utilities for filesystem storage implementations

use std::path::Path;
use uuid::Uuid;

/// Extracts a UUID from a directory path's last component.
///
/// Returns `None` if the directory name is missing or cannot be parsed as a UUID.
pub(super) fn parse_uuid_from_dir(path: &Path) -> Option<Uuid> {
    path.file_name()
        .and_then(|n| n.to_str())
        .and_then(|s| s.parse().ok())
}
