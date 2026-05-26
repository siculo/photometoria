// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

use serde::{Deserialize, Serialize};

/// Project configuration section.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProjectConfig {
    /// Maximum allowed length of a project context in characters.
    pub max_context_length: usize,
}

impl ProjectConfig {
    /// Appends this section's summary to the output buffer.
    pub fn format_summary(&self, out: &mut String) {
        out.push_str("  [project]\n");
        out.push_str(&format!(
            "    max_context_length: {}\n",
            self.max_context_length
        ));
    }
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            max_context_length: 2000,
        }
    }
}
