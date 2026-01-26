//! Data models for the Photometoria REST API
//!
//! This module contains all data structures (entities and DTOs) used throughout
//! the application.

pub mod task;

// Re-export public types for convenient access
pub use task::{CreateTaskRequest, Task, TaskDetail, TaskResponse, TaskSummary, UpdateTaskRequest};
