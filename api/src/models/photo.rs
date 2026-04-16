// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! Photo data model and related DTOs
//!
//! This module defines the Photo entity and all related Data Transfer Objects (DTOs)
//! for the REST API endpoints.
//!
//! A Photo represents an uploaded image file associated with a Task.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// Photo Entity (Internal Domain Model)
// ============================================================================

/// Represents an uploaded photo file.
///
/// A Photo belongs to a Task and contains metadata about the uploaded image.
/// The actual image data is stored on disk, referenced by photo_id.
///
/// ## Lifecycle
/// ```text
/// Uploaded → Available for Jobs → Explicitly Deleted (or Task Deleted)
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Photo {
    /// Unique identifier (UUID)
    pub photo_id: Uuid,

    /// Reference to the parent Task
    pub task_id: Uuid,

    /// Client-provided identifier for reconciliation (opaque string)
    #[serde(default)]
    pub client_id: Option<String>,

    /// Original filename from upload
    pub filename: String,

    /// File size in bytes
    pub size_bytes: u64,

    /// Timestamp when the photo was uploaded (ISO 8601)
    pub uploaded_at: DateTime<Utc>,
}

impl Photo {
    /// Creates a new Photo with generated UUID and current timestamp.
    ///
    /// # Arguments
    /// * `task_id` - The UUID of the parent Task
    /// * `client_id` - Optional client-provided identifier for reconciliation
    /// * `filename` - Original filename from upload
    /// * `size_bytes` - File size in bytes
    ///
    /// # Example
    /// ```
    /// use photometoria_rest_api::models::Photo;
    /// use uuid::Uuid;
    ///
    /// let photo = Photo::new(
    ///     Uuid::new_v4(),
    ///     Some("lr:123456".to_string()),
    ///     "IMG_1234.jpg".to_string(),
    ///     4_200_000,
    /// );
    /// ```
    pub fn new(
        task_id: Uuid,
        client_id: Option<String>,
        filename: String,
        size_bytes: u64,
    ) -> Self {
        Self {
            photo_id: Uuid::new_v4(),
            task_id,
            client_id,
            filename,
            size_bytes,
            uploaded_at: Utc::now(),
        }
    }
}

// ============================================================================
// DTOs for API Endpoints
// ============================================================================

/// Detailed information about a photo.
///
/// Used by: `GET /api/photos/{photo_id}`
///
/// # Example JSON
/// ```json
/// {
///   "photo_id": "550e8400-e29b-41d4-a716-446655440000",
///   "task_id": "550e8400-e29b-41d4-a716-446655440001",
///   "client_id": "lr:123456",
///   "filename": "IMG_1234.jpg",
///   "size_bytes": 4200000,
///   "uploaded_at": "2024-01-15T10:32:00Z"
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhotoResponse {
    /// Unique photo identifier
    pub photo_id: Uuid,

    /// Parent task identifier
    pub task_id: Uuid,

    /// Client-provided identifier for reconciliation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,

    /// Original filename
    pub filename: String,

    /// File size in bytes
    pub size_bytes: u64,

    /// Upload timestamp (ISO 8601)
    pub uploaded_at: DateTime<Utc>,
}

/// Summary information about a photo for listing endpoints.
///
/// Used in photo lists and summaries.
///
/// # Example JSON
/// ```json
/// {
///   "photo_id": "550e8400-e29b-41d4-a716-446655440000",
///   "filename": "IMG_1234.jpg",
///   "size_bytes": 4200000
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhotoSummary {
    /// Unique photo identifier
    pub photo_id: Uuid,

    /// Client-provided identifier for reconciliation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,

    /// Original filename
    pub filename: String,

    /// File size in bytes
    pub size_bytes: u64,
}

/// Response for photo upload endpoint.
///
/// Used by: `POST /api/tasks/{task_id}/photos`
///
/// The response always has HTTP 200/201 status. Use `uploaded` and `failed`
/// to determine the actual outcome:
/// - 201 Created: at least one photo was uploaded (`!uploaded.is_empty()`)
/// - 200 OK: no photos were uploaded (all failed or empty request)
///
/// # Example JSON (partial success)
/// ```json
/// {
///   "uploaded": [
///     {"photo_id": "p1", "client_id": "lr:001", "filename": "IMG_001.jpg", "size_bytes": 4200000, "replaced": false},
///     {"photo_id": "p2", "client_id": "lr:002", "filename": "IMG_002.jpg", "size_bytes": 3800000, "replaced": true}
///   ],
///   "failed": [
///     {"filename": "IMG_003.jpg", "reason": "file_too_large"}
///   ],
///   "created_count": 1,
///   "replaced_count": 1,
///   "failed_count": 1,
///   "uploaded_size_bytes": 8000000
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadPhotosResponse {
    /// Successfully uploaded photos (both new and replaced)
    pub uploaded: Vec<UploadedPhoto>,

    /// Photos that failed to upload
    pub failed: Vec<FailedUpload>,

    /// Number of newly created photos
    pub created_count: usize,

    /// Number of photos that replaced an existing entry with the same client_id
    pub replaced_count: usize,

    /// Number of photos that failed to upload
    pub failed_count: usize,

    /// Total size of successfully uploaded photos in bytes
    pub uploaded_size_bytes: u64,
}

/// Information about a successfully uploaded photo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadedPhoto {
    /// Client-provided identifier for tracking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,

    /// Unique photo identifier
    pub photo_id: Uuid,

    /// Original filename
    pub filename: String,

    /// File size in bytes
    pub size_bytes: u64,

    /// Whether this upload replaced an existing photo with the same client_id
    pub replaced: bool,
}

/// Information about a photo that failed to upload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedUpload {
    /// Client-provided identifier for tracking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,

    /// Original filename
    pub filename: String,

    /// Reason for failure (e.g., "file_too_large", "invalid_format", "storage_full")
    pub reason: String,
}

/// Response for listing photos in a task.
///
/// Used by: `GET /api/tasks/{task_id}/photos`
///
/// # Example JSON
/// ```json
/// {
///   "photos": [
///     {
///       "photo_id": "550e8400-e29b-41d4-a716-446655440000",
///       "client_id": "lr:123456",
///       "filename": "IMG_1234.jpg",
///       "size_bytes": 4200000
///     }
///   ],
///   "count": 1
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhotoListResponse {
    /// Photo summaries
    pub photos: Vec<PhotoSummary>,

    /// Total count of photos
    pub count: usize,
}

// ============================================================================
// Conversions from Entity to DTOs
// ============================================================================

impl From<Photo> for PhotoResponse {
    fn from(photo: Photo) -> Self {
        Self {
            photo_id: photo.photo_id,
            task_id: photo.task_id,
            client_id: photo.client_id,
            filename: photo.filename,
            size_bytes: photo.size_bytes,
            uploaded_at: photo.uploaded_at,
        }
    }
}

impl From<&Photo> for PhotoResponse {
    fn from(photo: &Photo) -> Self {
        Self {
            photo_id: photo.photo_id,
            task_id: photo.task_id,
            client_id: photo.client_id.clone(),
            filename: photo.filename.clone(),
            size_bytes: photo.size_bytes,
            uploaded_at: photo.uploaded_at,
        }
    }
}

impl From<Photo> for PhotoSummary {
    fn from(photo: Photo) -> Self {
        Self {
            photo_id: photo.photo_id,
            client_id: photo.client_id,
            filename: photo.filename,
            size_bytes: photo.size_bytes,
        }
    }
}

impl From<&Photo> for PhotoSummary {
    fn from(photo: &Photo) -> Self {
        Self {
            photo_id: photo.photo_id,
            client_id: photo.client_id.clone(),
            filename: photo.filename.clone(),
            size_bytes: photo.size_bytes,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_photo_new_generates_uuid() {
        let task_id = Uuid::new_v4();
        let photo = Photo::new(task_id, None, "test.jpg".to_string(), 1_000_000);
        assert!(!photo.photo_id.is_nil());
    }

    #[test]
    fn test_photo_new_sets_fields() {
        let task_id = Uuid::new_v4();
        let photo = Photo::new(task_id, None, "vacation.jpg".to_string(), 5_242_880);
        assert_eq!(photo.task_id, task_id);
        assert!(photo.client_id.is_none());
        assert_eq!(photo.filename, "vacation.jpg");
        assert_eq!(photo.size_bytes, 5_242_880);
    }

    #[test]
    fn test_photo_new_with_client_id() {
        let task_id = Uuid::new_v4();
        let photo = Photo::new(
            task_id,
            Some("lr:123456".to_string()),
            "photo.jpg".to_string(),
            2_000_000,
        );
        assert_eq!(photo.client_id, Some("lr:123456".to_string()));
    }

    #[test]
    fn test_photo_deserialization_without_client_id() {
        let json = r#"{"photo_id":"550e8400-e29b-41d4-a716-446655440000","task_id":"550e8400-e29b-41d4-a716-446655440001","filename":"test.jpg","size_bytes":1000,"uploaded_at":"2024-01-15T10:32:00Z"}"#;
        let photo: Photo = serde_json::from_str(json).unwrap();
        assert!(photo.client_id.is_none());
    }

    #[test]
    fn test_photo_to_response_conversion() {
        let task_id = Uuid::new_v4();
        let photo = Photo::new(
            task_id,
            Some("lr:42".to_string()),
            "test.jpg".to_string(),
            2_097_152,
        );
        let response: PhotoResponse = (&photo).into();

        assert_eq!(response.photo_id, photo.photo_id);
        assert_eq!(response.task_id, photo.task_id);
        assert_eq!(response.client_id, photo.client_id);
        assert_eq!(response.filename, photo.filename);
        assert_eq!(response.size_bytes, photo.size_bytes);
        assert_eq!(response.uploaded_at, photo.uploaded_at);
    }

    #[test]
    fn test_photo_to_summary_conversion() {
        let task_id = Uuid::new_v4();
        let photo = Photo::new(
            task_id,
            Some("lr:99".to_string()),
            "test.jpg".to_string(),
            1_048_576,
        );
        let summary: PhotoSummary = (&photo).into();

        assert_eq!(summary.photo_id, photo.photo_id);
        assert_eq!(summary.client_id, photo.client_id);
        assert_eq!(summary.filename, photo.filename);
        assert_eq!(summary.size_bytes, photo.size_bytes);
    }

    #[test]
    fn test_photo_serialization() {
        let task_id = Uuid::new_v4();
        let photo = Photo::new(
            task_id,
            Some("lr:1".to_string()),
            "test.jpg".to_string(),
            1_000_000,
        );
        let json = serde_json::to_string(&photo).unwrap();

        assert!(json.contains("photo_id"));
        assert!(json.contains("task_id"));
        assert!(json.contains("client_id"));
        assert!(json.contains("filename"));
        assert!(json.contains("size_bytes"));
        assert!(json.contains("uploaded_at"));
    }

    #[test]
    fn test_photo_response_serialization() {
        let response = PhotoResponse {
            photo_id: Uuid::new_v4(),
            task_id: Uuid::new_v4(),
            client_id: Some("lr:42".to_string()),
            filename: "test.jpg".to_string(),
            size_bytes: 4_200_000,
            uploaded_at: Utc::now(),
        };
        let json = serde_json::to_string(&response).unwrap();

        assert!(json.contains("size_bytes"));
    }

    #[test]
    fn test_upload_photos_response_serialization() {
        let response = UploadPhotosResponse {
            uploaded: vec![
                UploadedPhoto {
                    client_id: Some("lr:001".to_string()),
                    photo_id: Uuid::new_v4(),
                    filename: "IMG_001.jpg".to_string(),
                    size_bytes: 4_200_000,
                    replaced: false,
                },
                UploadedPhoto {
                    client_id: Some("lr:002".to_string()),
                    photo_id: Uuid::new_v4(),
                    filename: "IMG_002.jpg".to_string(),
                    size_bytes: 4_300_000,
                    replaced: true,
                },
            ],
            failed: vec![FailedUpload {
                client_id: Some("lr:003".to_string()),
                filename: "IMG_003.jpg".to_string(),
                reason: "file_too_large".to_string(),
            }],
            created_count: 1,
            replaced_count: 1,
            failed_count: 1,
            uploaded_size_bytes: 8_500_000,
        };
        let json = serde_json::to_string(&response).unwrap();

        assert!(json.contains("uploaded"));
        assert!(json.contains("failed"));
        assert!(json.contains("created_count"));
        assert!(json.contains("replaced_count"));
        assert!(json.contains("failed_count"));
        assert!(json.contains("uploaded_size_bytes"));
        assert!(json.contains("client_id"));
        assert!(json.contains("photo_id"));
        assert!(json.contains("filename"));
        assert!(json.contains("reason"));
        assert!(json.contains("replaced"));
    }
}
