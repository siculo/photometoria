// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Multipart, State};
use axum::http::StatusCode;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::config::Config;
use crate::handlers::app_error::{AppError, AppPath};
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
    AppPath(task_id): AppPath<Uuid>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<UploadPhotosResponse>), AppError> {
    debug!("Upload photos request for task_id={}", task_id);

    // Verify task exists
    let task_exists = state
        .task_store
        .exists(task_id)
        .await
        .map_err(|e| AppError::internal_error(e.to_string()))?;
    if !task_exists {
        return Err(AppError::task_not_found(task_id));
    }

    let (uploaded, failed, uploaded_size_bytes) =
        process_multipart(&state, task_id, &mut multipart).await?;

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

/// Collected file data from multipart.
struct FileData {
    filename: String,
    data: Bytes,
}

/// Processes the entire multipart request, returning the result of the upload operation.
///
/// Expects:
/// - An optional `client_ids` field with a JSON array of strings
/// - One or more `files` fields with the actual image data
///
/// If `client_ids` is provided, its length must match the number of files.
/// If omitted, all photos are stored with `client_id = None`.
async fn process_multipart(
    state: &AppState,
    task_id: Uuid,
    multipart: &mut Multipart,
) -> Result<(Vec<UploadedPhoto>, Vec<FailedUpload>, u64), AppError> {
    let mut client_ids: Option<Vec<String>> = None;
    let mut files: Vec<FileData> = vec![];

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(e) => {
                error!("Error reading multipart field: {:?}", e);
                return Err(AppError::bad_request(
                    "invalid_multipart",
                    format!("Error reading multipart field: {}", e),
                ));
            }
        };

        let field_name = field.name().unwrap_or_default().to_string();

        match field_name.as_str() {
            "client_ids" => {
                let text = field.text().await.map_err(|e| {
                    error!("Error reading client_ids field: {:?}", e);
                    AppError::bad_request(
                        "invalid_client_ids",
                        format!("Error reading client_ids field: {}", e),
                    )
                })?;

                let ids: Vec<String> = serde_json::from_str(&text).map_err(|e| {
                    error!("Error parsing client_ids JSON: {:?}", e);
                    AppError::bad_request(
                        "invalid_client_ids",
                        format!("client_ids must be a JSON array of strings: {}", e),
                    )
                })?;

                debug!("Received {} client_ids", ids.len());
                client_ids = Some(ids);
            }
            "files" => {
                let filename = field.file_name().unwrap_or_default().to_string();
                let data = field.bytes().await.map_err(|e| {
                    error!("Error reading file bytes for '{}': {:?}", filename, e);
                    AppError::bad_request(
                        "invalid_file",
                        format!("Error reading file '{}': {}", filename, e),
                    )
                })?;

                debug!("Received file '{}' ({} bytes)", filename, data.len());
                files.push(FileData { filename, data });
            }
            _ => {
                warn!("Ignoring unknown field: {}", field_name);
            }
        }
    }

    // Validate: if client_ids is provided, counts must match
    if let Some(ref ids) = client_ids {
        if ids.len() != files.len() {
            error!(
                "Mismatch: {} client_ids but {} files",
                ids.len(),
                files.len()
            );
            return Err(AppError::bad_request(
                "client_ids_mismatch",
                format!(
                    "Number of client_ids ({}) does not match number of files ({})",
                    ids.len(),
                    files.len()
                ),
            ));
        }
    }

    // Process files
    let mut used_storage = state
        .photo_store
        .total_size()
        .await
        .map_err(|e| AppError::internal_error(e.to_string()))?;
    let mut uploaded: Vec<UploadedPhoto> = vec![];
    let mut failed: Vec<FailedUpload> = vec![];
    let mut uploaded_size_bytes: u64 = 0;

    for (idx, file) in files.into_iter().enumerate() {
        let client_id = client_ids.as_ref().map(|ids| ids[idx].clone());

        let result = process_field(
            file.data,
            file.filename,
            client_id,
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
    client_id: Option<String>,
    task_id: Uuid,
    uploaded_count: usize,
    used_storage: u64,
    config: &Config,
    photo_store: &Arc<dyn PhotoStore>,
) -> ProcessedField {
    let data_size = data.len() as u64;

    // Validate photo
    if let Some(reason) = validate_photo(&data, uploaded_count, used_storage, config) {
        return ProcessedField::Failed(FailedUpload {
            client_id,
            filename,
            reason,
        });
    }

    // Create photo and save to store
    let photo = Photo::new(task_id, client_id.clone(), filename.clone(), data_size);
    match photo_store.create(photo.clone()).await {
        Ok(_) => {
            // Save actual image data
            if let Err(e) = photo_store.save_data(photo.photo_id, &data).await {
                error!("Failed to save image data for '{}': {:?}", filename, e);
                // Try to clean up the metadata we just created
                let _ = photo_store.delete(photo.photo_id).await;
                return ProcessedField::Failed(FailedUpload {
                    client_id,
                    filename,
                    reason: "storage_error".to_string(),
                });
            }
            ProcessedField::Uploaded(
                UploadedPhoto {
                    client_id,
                    photo_id: photo.photo_id,
                    filename,
                    size_bytes: data_size,
                },
                data_size,
            )
        }
        Err(_) => ProcessedField::Failed(FailedUpload {
            client_id,
            filename,
            reason: "storage_error".to_string(),
        }),
    }
}

/// MIME types accepted for photo uploads, validated via magic bytes.
pub const ALLOWED_MIME_TYPES: &[&str] = &["image/jpeg", "image/png"];

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
        .map(|k| ALLOWED_MIME_TYPES.contains(&k.mime_type()))
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

    fn create_test_config(
        max_photos: usize,
        max_photo_size_bytes: u64,
        max_storage_bytes: u64,
    ) -> Config {
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
