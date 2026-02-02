use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::Multipart;
use axum::{
    extract::{Json, Path, State},
    http::StatusCode,
};
use tracing::{debug, error, info, warn};

use crate::app_state::AppState;
use crate::config::Config;
use crate::models::{FailedUpload, Photo, UploadPhotosResponse, UploadedPhoto};
use crate::storage::PhotoStore;

/// Result of processing a single upload field.
enum ProcessedField {
    Uploaded(UploadedPhoto, u64),
    Failed(FailedUpload),
}

/// Handler for POST /api/tasks/{task_id}/photos
pub async fn upload_photos(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<UploadPhotosResponse>), StatusCode> {
    debug!("Upload photos request for task_id={}", task_id);

    // Verify task exists
    let task_exists = state
        .task_store
        .exists(&task_id)
        .await
        .map_err(to_internal_server_error)?;
    if !task_exists {
        warn!("Task not found: {}", task_id);
        return Err(StatusCode::NOT_FOUND);
    }

    let (uploaded, failed, uploaded_size_bytes) = process_multipart(&state, &task_id, &mut multipart).await?;

    let status = if uploaded.is_empty() {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };

    info!(
        "Upload completed for task_id={}: {} uploaded, {} failed, {} bytes",
        task_id,
        uploaded.len(),
        failed.len(),
        uploaded_size_bytes
    );

    let response = UploadPhotosResponse {
        uploaded,
        failed,
        uploaded_size_bytes,
    };

    Ok((status, Json(response)))
}

/// Processes the entire multipart request, returning the result the load operation.
async fn process_multipart(state: &AppState, task_id: &String, multipart: &mut Multipart) -> Result<(Vec<UploadedPhoto>, Vec<FailedUpload>, u64), StatusCode> {
    let mut used_storage = state
        .photo_store
        .total_size()
        .await
        .map_err(to_internal_server_error)?;
    let mut uploaded: Vec<UploadedPhoto> = vec![];
    let mut failed: Vec<FailedUpload> = vec![];
    let mut uploaded_size_bytes: u64 = 0;

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(e) => {
                error!("Error reading multipart field: {:?}", e);
                return Err(StatusCode::BAD_REQUEST);
            }
        };

        let field_name = field.name().map(|s| s.to_string());
        let filename = field.file_name().unwrap_or_default().to_string();
        debug!("Processing field: name={:?}, filename={}", field_name, filename);

        let data = match field.bytes().await {
            Ok(data) => data,
            Err(e) => {
                error!("Error reading field bytes for '{}': {:?}", filename, e);
                return Err(StatusCode::BAD_REQUEST);
            }
        };
        debug!("Read {} bytes for '{}'", data.len(), filename);

        let result = process_field(
            data,
            filename,
            task_id,
            uploaded.len(),
            used_storage,
            &state.config,
            &state.photo_store,
        )
            .await;

        match result {
            ProcessedField::Uploaded(photo, size) => {
                debug!("Photo uploaded: {} ({} bytes)", photo.photo_id, size);
                used_storage += size;
                uploaded_size_bytes += size;
                uploaded.push(photo);
            }
            ProcessedField::Failed(failure) => {
                warn!("Photo failed: {} - {}", failure.filename, failure.reason);
                failed.push(failure);
            }
        }
    }
    Ok((uploaded, failed, uploaded_size_bytes))
}

/// Process a single upload field: validate and save to store.
async fn process_field(
    data: Bytes,
    filename: String,
    task_id: &str,
    uploaded_count: usize,
    used_storage: u64,
    config: &Config,
    photo_store: &Arc<dyn PhotoStore>,
) -> ProcessedField {
    let data_size = data.len() as u64;

    // Validate photo
    if let Some(reason) = validate_photo(&data, uploaded_count, used_storage, config) {
        return ProcessedField::Failed(FailedUpload { filename, reason });
    }

    // Create photo and save to store
    let photo = Photo::new(task_id.to_string(), filename.clone(), data_size);
    match photo_store.create(photo.clone()).await {
        Ok(_) => {
            // Save actual image data
            if let Err(e) = photo_store.save_data(&photo.photo_id, &data).await {
                error!("Failed to save image data for '{}': {:?}", filename, e);
                // Try to clean up the metadata we just created
                let _ = photo_store.delete(&photo.photo_id).await;
                return ProcessedField::Failed(FailedUpload {
                    filename,
                    reason: "storage_error".to_string(),
                });
            }
            ProcessedField::Uploaded(
                UploadedPhoto {
                    photo_id: photo.photo_id,
                    filename,
                    size_bytes: data_size,
                },
                data_size,
            )
        }
        Err(_) => ProcessedField::Failed(FailedUpload {
            filename,
            reason: "storage_error".to_string(),
        }),
    }
}

/// Validate photo data against configuration limits.
/// Returns None if valid, Some(reason) if invalid.
fn validate_photo(
    data: &[u8],
    uploaded_count: usize,
    used_storage: u64,
    config: &Config,
) -> Option<String> {
    let data_size = data.len() as u64;

    // Check format using magic bytes
    let is_supported = infer::get(data)
        .map(|k| matches!(k.mime_type(), "image/jpeg" | "image/png"))
        .unwrap_or(false);
    if !is_supported {
        return Some("invalid_format".to_string());
    }

    // Check count limit
    if uploaded_count >= config.upload.max_photos_per_request {
        return Some("too_many_files".to_string());
    }

    // Check file size
    if data_size > config.upload.max_photo_size.0 {
        return Some("file_too_large".to_string());
    }

    // Check storage space
    if used_storage + data_size > config.storage_max_size() {
        return Some("storage_full".to_string());
    }

    None
}

fn to_internal_server_error<E>(_: E) -> StatusCode {
    StatusCode::INTERNAL_SERVER_ERROR
}

#[cfg(test)]
mod tests {
    use super::*;

    // JPEG magic bytes: FF D8 FF
    fn create_jpeg_data(size: usize) -> Vec<u8> {
        let mut data = vec![0xFF, 0xD8, 0xFF, 0xE0];
        data.resize(size, 0x00);
        data
    }

    // PNG magic bytes: 89 50 4E 47 0D 0A 1A 0A
    fn create_png_data(size: usize) -> Vec<u8> {
        let mut data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        data.resize(size, 0x00);
        data
    }

    fn create_test_config(max_photos: usize, max_photo_size_bytes: u64, max_storage_bytes: u64) -> Config {
        Config {
            upload: crate::config::UploadConfig {
                max_photos_per_request: max_photos,
                max_photo_size: crate::config::ByteSize(max_photo_size_bytes),
            },
            storage: crate::config::StorageConfig {
                path: "/tmp".to_string(),
                max_size: crate::config::ByteSize(max_storage_bytes),
            },
            ..Default::default()
        }
    }

    #[test]
    fn test_validate_photo_valid_jpeg() {
        let data = create_jpeg_data(1000);
        let config = create_test_config(10, 10_000, 100_000);

        let result = validate_photo(&data, 0, 0, &config);

        assert!(result.is_none());
    }

    #[test]
    fn test_validate_photo_valid_png() {
        let data = create_png_data(1000);
        let config = create_test_config(10, 10_000, 100_000);

        let result = validate_photo(&data, 0, 0, &config);

        assert!(result.is_none());
    }

    #[test]
    fn test_validate_photo_invalid_format() {
        let data = vec![0x00, 0x01, 0x02, 0x03]; // Not a valid image
        let config = create_test_config(10, 10_000, 100_000);

        let result = validate_photo(&data, 0, 0, &config);

        assert_eq!(result, Some("invalid_format".to_string()));
    }

    #[test]
    fn test_validate_photo_invalid_format_pdf() {
        // PDF magic bytes: 25 50 44 46 (%PDF)
        let data = vec![0x25, 0x50, 0x44, 0x46, 0x2D, 0x31, 0x2E, 0x34];
        let config = create_test_config(10, 10_000, 100_000);

        let result = validate_photo(&data, 0, 0, &config);

        assert_eq!(result, Some("invalid_format".to_string()));
    }

    #[test]
    fn test_validate_photo_too_many_files() {
        let data = create_jpeg_data(1000);
        let config = create_test_config(5, 10_000, 100_000);

        // Already uploaded 5 files (limit is 5)
        let result = validate_photo(&data, 5, 0, &config);

        assert_eq!(result, Some("too_many_files".to_string()));
    }

    #[test]
    fn test_validate_photo_at_limit_ok() {
        let data = create_jpeg_data(1000);
        let config = create_test_config(5, 10_000, 100_000);

        // Already uploaded 4 files (limit is 5), so one more is OK
        let result = validate_photo(&data, 4, 0, &config);

        assert!(result.is_none());
    }

    #[test]
    fn test_validate_photo_file_too_large() {
        let data = create_jpeg_data(15_000);
        let config = create_test_config(10, 10_000, 100_000); // max 10KB

        let result = validate_photo(&data, 0, 0, &config);

        assert_eq!(result, Some("file_too_large".to_string()));
    }

    #[test]
    fn test_validate_photo_exactly_at_size_limit() {
        let data = create_jpeg_data(10_000);
        let config = create_test_config(10, 10_000, 100_000); // max 10KB

        // Exactly at limit should be OK
        let result = validate_photo(&data, 0, 0, &config);

        assert!(result.is_none());
    }

    #[test]
    fn test_validate_photo_storage_full() {
        let data = create_jpeg_data(5_000);
        let config = create_test_config(10, 10_000, 10_000); // max storage 10KB

        // Already used 6KB, trying to add 5KB = 11KB > 10KB
        let result = validate_photo(&data, 0, 6_000, &config);

        assert_eq!(result, Some("storage_full".to_string()));
    }

    #[test]
    fn test_validate_photo_storage_exactly_fits() {
        let data = create_jpeg_data(4_000);
        let config = create_test_config(10, 10_000, 10_000); // max storage 10KB

        // Already used 6KB, trying to add 4KB = 10KB = exactly at limit
        let result = validate_photo(&data, 0, 6_000, &config);

        assert!(result.is_none());
    }

    #[test]
    fn test_validate_photo_checks_format_first() {
        // Invalid format should be reported even if other limits are exceeded
        let data = vec![0x00, 0x01, 0x02, 0x03];
        let config = create_test_config(1, 1, 1); // All limits very low

        let result = validate_photo(&data, 10, 1000, &config);

        // Should report invalid_format, not other errors
        assert_eq!(result, Some("invalid_format".to_string()));
    }
}
