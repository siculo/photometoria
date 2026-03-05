// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfoResult {
    pub general: GeneralInfo,
    pub server: ServerInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralInfo {
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub allocated_space_bytes: u64,
    pub used_space_bytes: u64,
    pub available_providers: Vec<String>,
    pub default_provider: Option<String>,
    pub active_tasks_count: usize,
    pub running_jobs_count: usize,
}
