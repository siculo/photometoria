// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfoResult {
    pub general: GeneralInfo,
    pub server: ServerInfo,
    pub limits: LimitsInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralInfo {
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub allocated_space_bytes: u64,
    pub used_space_bytes: u64,
    pub available_space_bytes: u64,
    pub available_providers: Vec<String>,
    pub default_provider: Option<String>,
    pub active_tasks_count: usize,
    pub running_jobs_count: usize,
}

/// Client-relevant configuration limits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitsInfo {
    pub max_photos_per_request: usize,
    pub max_photo_size_bytes: u64,
    pub max_concurrent_jobs: Option<usize>,
}
