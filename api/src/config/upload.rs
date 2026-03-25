// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

use super::ByteSize;
use serde::Deserialize;

/// Upload configuration section
#[derive(Debug, Clone, Deserialize)]
pub struct UploadConfig {
    pub max_photos_per_request: usize,
    pub max_photo_size: ByteSize,
}

impl UploadConfig {
    /// Appends this section's summary to the output buffer.
    pub fn format_summary(&self, out: &mut String) {
        out.push_str(&format!("  [upload]\n"));
        out.push_str(&format!(
            "    max_photos_per_request: {}\n",
            self.max_photos_per_request
        ));
        out.push_str(&format!("    max_photo_size: {}\n", self.max_photo_size));
    }
}

impl Default for UploadConfig {
    fn default() -> Self {
        Self {
            max_photos_per_request: 100,
            max_photo_size: "20MB".parse().unwrap(),
        }
    }
}
