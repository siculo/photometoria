// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! Data models for the Photometoria REST API
//!
//! This module contains all data structures (entities and DTOs) used throughout
//! the application.

pub mod activity;
pub mod catalog;
pub mod info;
pub mod photo;
pub mod project;

// Re-export public types for convenient access
pub use activity::{
    Activity, ActivityDetailResponse, ActivityProgress, ActivityResponse, ActivityResultsResponse,
    ActivityStatus, ActivitySummary, CreateActivityRequest, PhotoResult, PhotoResultStatus,
    ResultSummary, RetryActivityResponse,
};
pub use catalog::{Catalog, CatalogSummary};
pub use photo::{
    FailedUpload, Photo, PhotoListResponse, PhotoResponse, PhotoSummary, UploadPhotosResponse,
    UploadedPhoto,
};
pub use project::{
    CreateProjectRequest, Project, ProjectDetail, ProjectResponse, ProjectSummary,
    UpdateProjectRequest,
};
