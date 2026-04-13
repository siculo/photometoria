// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! Data models for the Photometoria REST API
//!
//! This module contains all data structures (entities and DTOs) used throughout
//! the application.

pub mod catalog;
pub mod info;
pub mod job;
pub mod photo;
pub mod task;

// Re-export public types for convenient access
pub use catalog::{Catalog, CatalogSummary};
pub use job::{
    CreateJobRequest, Job, JobCancelledResponse, JobDetailResponse, JobProgress, JobResponse,
    JobResultsResponse, JobStatus, JobSummary, PhotoResult, PhotoResultStatus, ResultSummary,
    RetryJobResponse,
};
pub use photo::{
    FailedUpload, Photo, PhotoListResponse, PhotoResponse, PhotoSummary, UploadPhotosResponse,
    UploadedPhoto,
};
pub use task::{CreateTaskRequest, Task, TaskDetail, TaskResponse, TaskSummary, UpdateTaskRequest};
