// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

use super::ByteSize;
use serde::{Deserialize, Serialize};

/// Storage configuration section
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorageConfig {
    pub path: String,
    pub max_size: ByteSize,
}

impl StorageConfig {
    /// Appends this section's summary to the output buffer.
    pub fn format_summary(&self, out: &mut String) {
        out.push_str(&format!("  [storage]\n"));
        out.push_str(&format!("    path: {}\n", self.path));
        out.push_str(&format!("    max_size: {}\n", self.max_size));
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            path: "./storage".to_string(),
            max_size: "10GiB".parse().unwrap(),
        }
    }
}
