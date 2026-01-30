//! Data models for the Photometoria REST API
//!
//! This module contains all data structures (entities and DTOs) used throughout
//! the application.

pub mod job;
pub mod photo;
pub mod task;

// Re-export public types for convenient access
pub use job::{CreateJobRequest, Job, JobCancelledResponse, JobResponse, JobStatus, JobSummary};
pub use photo::{Photo, PhotoListResponse, PhotoResponse, PhotoSummary, UploadPhotosResponse};
pub use task::{CreateTaskRequest, Task, TaskDetail, TaskResponse, TaskSummary, UpdateTaskRequest};
