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
    ///     "IMG_1234.jpg".to_string(),
    ///     4_200_000,
    /// );
    /// ```
    pub fn new(task_id: Uuid, filename: String, size_bytes: u64) -> Self {
        Self {
            photo_id: Uuid::new_v4(),
            task_id,
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
///     {"photo_id": "p1", "filename": "IMG_001.jpg", "size_bytes": 4200000},
///     {"photo_id": "p2", "filename": "IMG_002.jpg", "size_bytes": 3800000}
///   ],
///   "failed": [
///     {"filename": "IMG_003.jpg", "reason": "file_too_large"}
///   ],
///   "uploaded_size_bytes": 8000000
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadPhotosResponse {
    /// Successfully uploaded photos
    pub uploaded: Vec<UploadedPhoto>,

    /// Photos that failed to upload
    pub failed: Vec<FailedUpload>,

    /// Total size of successfully uploaded photos in bytes
    pub uploaded_size_bytes: u64,
}

/// Information about a successfully uploaded photo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadedPhoto {
    /// Client-provided identifier for tracking
    pub client_id: String,

    /// Unique photo identifier
    pub photo_id: Uuid,

    /// Original filename
    pub filename: String,

    /// File size in bytes
    pub size_bytes: u64,
}

/// Information about a photo that failed to upload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedUpload {
    /// Client-provided identifier for tracking
    pub client_id: String,

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
///   "photo_ids": ["550e8400-e29b-41d4-a716-446655440000", "550e8400-e29b-41d4-a716-446655440001"],
///   "count": 2
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhotoListResponse {
    /// IDs of photos in the task
    pub photo_ids: Vec<Uuid>,

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
            filename: photo.filename,
            size_bytes: photo.size_bytes,
        }
    }
}

impl From<&Photo> for PhotoSummary {
    fn from(photo: &Photo) -> Self {
        Self {
            photo_id: photo.photo_id,
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
        let photo = Photo::new(task_id, "test.jpg".to_string(), 1_000_000);
        assert!(!photo.photo_id.is_nil());
    }

    #[test]
    fn test_photo_new_sets_fields() {
        let task_id = Uuid::new_v4();
        let photo = Photo::new(task_id, "vacation.jpg".to_string(), 5_242_880);
        assert_eq!(photo.task_id, task_id);
        assert_eq!(photo.filename, "vacation.jpg");
        assert_eq!(photo.size_bytes, 5_242_880);
    }

    #[test]
    fn test_photo_to_response_conversion() {
        let task_id = Uuid::new_v4();
        let photo = Photo::new(task_id, "test.jpg".to_string(), 2_097_152);
        let response: PhotoResponse = (&photo).into();

        assert_eq!(response.photo_id, photo.photo_id);
        assert_eq!(response.task_id, photo.task_id);
        assert_eq!(response.filename, photo.filename);
        assert_eq!(response.size_bytes, photo.size_bytes);
        assert_eq!(response.uploaded_at, photo.uploaded_at);
    }

    #[test]
    fn test_photo_to_summary_conversion() {
        let task_id = Uuid::new_v4();
        let photo = Photo::new(task_id, "test.jpg".to_string(), 1_048_576);
        let summary: PhotoSummary = (&photo).into();

        assert_eq!(summary.photo_id, photo.photo_id);
        assert_eq!(summary.filename, photo.filename);
        assert_eq!(summary.size_bytes, photo.size_bytes);
    }

    #[test]
    fn test_photo_serialization() {
        let task_id = Uuid::new_v4();
        let photo = Photo::new(task_id, "test.jpg".to_string(), 1_000_000);
        let json = serde_json::to_string(&photo).unwrap();

        assert!(json.contains("photo_id"));
        assert!(json.contains("task_id"));
        assert!(json.contains("filename"));
        assert!(json.contains("size_bytes"));
        assert!(json.contains("uploaded_at"));
    }

    #[test]
    fn test_photo_response_serialization() {
        let response = PhotoResponse {
            photo_id: Uuid::new_v4(),
            task_id: Uuid::new_v4(),
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
                    client_id: "/path/to/IMG_001.jpg".to_string(),
                    photo_id: Uuid::new_v4(),
                    filename: "IMG_001.jpg".to_string(),
                    size_bytes: 4_200_000,
                },
                UploadedPhoto {
                    client_id: "/path/to/IMG_002.jpg".to_string(),
                    photo_id: Uuid::new_v4(),
                    filename: "IMG_002.jpg".to_string(),
                    size_bytes: 4_300_000,
                },
            ],
            failed: vec![FailedUpload {
                client_id: "/path/to/IMG_003.jpg".to_string(),
                filename: "IMG_003.jpg".to_string(),
                reason: "file_too_large".to_string(),
            }],
            uploaded_size_bytes: 8_500_000,
        };
        let json = serde_json::to_string(&response).unwrap();

        assert!(json.contains("uploaded"));
        assert!(json.contains("failed"));
        assert!(json.contains("uploaded_size_bytes"));
        assert!(json.contains("client_id"));
        assert!(json.contains("photo_id"));
        assert!(json.contains("filename"));
        assert!(json.contains("reason"));
    }
}
