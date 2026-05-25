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
///
/// `Uploaded` carries `(photo, new_size, old_size)` — `old_size` is `0` for new
/// photos and the previous `size_bytes` for replacements, enabling callers to
/// compute the net storage delta.
enum ProcessedField {
    Uploaded(UploadedPhoto, u64, u64),
    Failed(FailedUpload),
}

/// Handler for POST /api/tasks/{task_id}/photos
pub async fn upload_photos(
    State(state): State<AppState>,
    AppPath(task_id): AppPath<Uuid>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<UploadPhotosResponse>), AppError> {
    debug!("Upload photos request for task_id={}", task_id);

    let task_exists = state
        .project_store
        .exists(task_id)
        .await
        .map_err(|e| AppError::internal_error(e.to_string()))?;
    if !task_exists {
        return Err(AppError::project_not_found(task_id));
    }

    let (client_ids, files) = extract_multipart_fields(&mut multipart).await?;
    let (uploaded, failed, uploaded_size_bytes) =
        process_files(&state, task_id, client_ids, files).await?;

    let status = if uploaded.is_empty() {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };

    let created_count = uploaded.iter().filter(|p| !p.replaced).count();
    let replaced_count = uploaded.iter().filter(|p| p.replaced).count();
    let failed_count = failed.len();

    info!(
        "Upload completed for task_id={}: {} created, {} replaced, {} failed, {} bytes",
        task_id, created_count, replaced_count, failed_count, uploaded_size_bytes
    );

    let response = UploadPhotosResponse {
        uploaded,
        failed,
        created_count,
        replaced_count,
        failed_count,
        uploaded_size_bytes,
    };

    Ok((status, Json(response)))
}

/// Collected file data from multipart.
struct FileData {
    filename: String,
    data: Bytes,
}

/// Reads all fields from the multipart stream and validates structural consistency.
///
/// Expects:
/// - An optional `client_ids` field with a JSON array of strings
/// - One or more `files` fields with the actual image data
///
/// If `client_ids` is provided, its length must match the number of files.
/// If omitted, all photos are stored with `client_id = None`.
async fn extract_multipart_fields(
    multipart: &mut Multipart,
) -> Result<(Option<Vec<String>>, Vec<FileData>), AppError> {
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

    Ok((client_ids, files))
}

/// Iterates the collected files, saving each one to the store.
async fn process_files(
    state: &AppState,
    task_id: Uuid,
    client_ids: Option<Vec<String>>,
    files: Vec<FileData>,
) -> Result<(Vec<UploadedPhoto>, Vec<FailedUpload>, u64), AppError> {
    let mut used_storage = state
        .photo_store
        .total_size()
        .await
        .map_err(|e| AppError::internal_error(e.to_string()))?;
    let mut uploaded: Vec<UploadedPhoto> = vec![];
    let mut failed: Vec<FailedUpload> = vec![];
    let mut uploaded_size_bytes: u64 = 0;

    let pairs: Vec<(FileData, Option<String>)> = match client_ids {
        Some(ids) => files.into_iter().zip(ids.into_iter().map(Some)).collect(),
        None => files.into_iter().map(|f| (f, None)).collect(),
    };

    for (file, client_id) in pairs {
        let result = process_field(
            file,
            client_id,
            task_id,
            uploaded.len(),
            used_storage,
            &state.config,
            &state.photo_store,
        )
        .await;

        match result {
            ProcessedField::Uploaded(photo, new_size, old_size) => {
                debug!("Photo uploaded: {} ({} bytes)", photo.photo_id, new_size);
                used_storage = used_storage.saturating_sub(old_size) + new_size;
                uploaded_size_bytes += new_size;
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

/// Finds an existing photo by `client_id` within a task, returning the first match.
async fn find_existing_photo(
    photo_store: &Arc<dyn PhotoStore>,
    task_id: Uuid,
    client_id: &str,
) -> Result<Option<crate::models::Photo>, ()> {
    match photo_store.find_by_client_id(task_id, client_id).await {
        Ok(mut photos) => Ok(photos.drain(..).next()),
        Err(e) => {
            error!(
                "Failed to look up photo by client_id '{}' in task {}: {:?}",
                client_id, task_id, e
            );
            Err(())
        }
    }
}

/// Validates and saves a new photo, cleaning up metadata if the data write fails.
async fn create_photo(
    photo_store: &Arc<dyn PhotoStore>,
    task_id: Uuid,
    client_id: Option<String>,
    filename: String,
    data: &Bytes,
) -> ProcessedField {
    let data_size = data.len() as u64;
    let photo = Photo::new(task_id, client_id.clone(), filename.clone(), data_size);
    let photo_id = photo.photo_id;

    match photo_store.create(photo).await {
        Ok(_) => {
            if let Err(e) = photo_store.save_data(photo_id, data).await {
                error!("Failed to save image data for '{}': {:?}", filename, e);
                let _ = photo_store.delete(photo_id).await;
                return ProcessedField::Failed(FailedUpload {
                    client_id,
                    filename,
                    reason: "storage_error".to_string(),
                });
            }
            ProcessedField::Uploaded(
                UploadedPhoto {
                    client_id,
                    photo_id,
                    filename,
                    size_bytes: data_size,
                    replaced: false,
                },
                data_size,
                0,
            )
        }
        Err(_) => ProcessedField::Failed(FailedUpload {
            client_id,
            filename,
            reason: "storage_error".to_string(),
        }),
    }
}

/// Updates an existing photo's metadata and overwrites its image data.
async fn replace_photo(
    photo_store: &Arc<dyn PhotoStore>,
    existing: crate::models::Photo,
    client_id: Option<String>,
    filename: String,
    data: &Bytes,
) -> ProcessedField {
    let data_size = data.len() as u64;
    let old_size = existing.size_bytes;
    let photo_id = existing.photo_id;

    match photo_store
        .update(photo_id, filename.clone(), data_size)
        .await
    {
        Ok(updated) => {
            if let Err(e) = photo_store.save_data(photo_id, data).await {
                error!("Failed to overwrite image data for '{}': {:?}", filename, e);
                return ProcessedField::Failed(FailedUpload {
                    client_id,
                    filename,
                    reason: "storage_error".to_string(),
                });
            }
            ProcessedField::Uploaded(
                UploadedPhoto {
                    client_id: updated.client_id,
                    photo_id,
                    filename: updated.filename,
                    size_bytes: data_size,
                    replaced: true,
                },
                data_size,
                old_size,
            )
        }
        Err(_) => ProcessedField::Failed(FailedUpload {
            client_id,
            filename,
            reason: "storage_error".to_string(),
        }),
    }
}

/// Validates a single file and saves it to the store (upsert semantics).
///
/// When `client_id` is `Some` and a photo with the same `client_id` already
/// exists in the task, the existing photo is replaced (metadata updated, image
/// data overwritten). Otherwise a new photo is created.
async fn process_field(
    file: FileData,
    client_id: Option<String>,
    task_id: Uuid,
    uploaded_count: usize,
    used_storage: u64,
    config: &Config,
    photo_store: &Arc<dyn PhotoStore>,
) -> ProcessedField {
    let existing = match client_id {
        Some(ref cid) => match find_existing_photo(photo_store, task_id, cid).await {
            Ok(photo) => photo,
            Err(()) => {
                return ProcessedField::Failed(FailedUpload {
                    client_id,
                    filename: file.filename,
                    reason: "storage_error".to_string(),
                });
            }
        },
        None => None,
    };

    let effective_storage = match &existing {
        Some(p) => used_storage.saturating_sub(p.size_bytes),
        None => used_storage,
    };

    if let Some(reason) = validate_photo(&file.data, uploaded_count, effective_storage, config) {
        return ProcessedField::Failed(FailedUpload {
            client_id,
            filename: file.filename,
            reason,
        });
    }

    match existing {
        Some(existing_photo) => {
            replace_photo(
                photo_store,
                existing_photo,
                client_id,
                file.filename,
                &file.data,
            )
            .await
        }
        None => create_photo(photo_store, task_id, client_id, file.filename, &file.data).await,
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

    let is_supported = infer::get(data)
        .map(|k| ALLOWED_MIME_TYPES.contains(&k.mime_type()))
        .unwrap_or(false);
    if !is_supported {
        return Some("invalid_format".to_string());
    }

    if uploaded_count >= config.upload.max_photos_per_request {
        return Some("too_many_files".to_string());
    }

    if data_size > config.upload.max_photo_size.0 {
        return Some("file_too_large".to_string());
    }

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

    // =========================================================================
    // Handler tests (upsert behaviour)
    // =========================================================================

    use crate::handlers::app_error::AppPath;
    use crate::handlers::test_utils::fixtures::{create_test_state, test_catalog_id};
    use crate::models::Project;
    use axum::extract::FromRequest;
    use axum::http::Request;
    use chrono::Utc;

    fn jpeg_data() -> Vec<u8> {
        create_jpeg_data(200)
    }

    /// Builds a raw multipart/form-data body.
    ///
    /// `client_ids`: if Some, serialised as a JSON array in the `client_ids` field.
    /// `files`: list of (filename, bytes) pairs sent as `files` fields.
    fn build_multipart_body(
        boundary: &str,
        client_ids: Option<Vec<&str>>,
        files: Vec<(&str, Vec<u8>)>,
    ) -> Vec<u8> {
        let mut body = Vec::new();

        if let Some(ids) = client_ids {
            let json = serde_json::to_string(&ids).unwrap();
            body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
            body.extend_from_slice(b"Content-Disposition: form-data; name=\"client_ids\"\r\n\r\n");
            body.extend_from_slice(json.as_bytes());
            body.extend_from_slice(b"\r\n");
        }

        for (filename, data) in files {
            body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
            body.extend_from_slice(
                format!(
                    "Content-Disposition: form-data; name=\"files\"; filename=\"{}\"\r\n",
                    filename
                )
                .as_bytes(),
            );
            body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
            body.extend_from_slice(&data);
            body.extend_from_slice(b"\r\n");
        }

        body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());
        body
    }

    async fn make_multipart(boundary: &str, body: Vec<u8>) -> Multipart {
        let content_type = format!("multipart/form-data; boundary={}", boundary);
        let request = Request::builder()
            .method("POST")
            .header("content-type", content_type)
            .body(axum::body::Body::from(body))
            .unwrap();
        Multipart::from_request(request, &()).await.unwrap()
    }

    async fn create_task_in_store(
        ts: &crate::handlers::test_utils::fixtures::TestState,
    ) -> Project {
        let task = Project {
            project_id: Uuid::new_v4(),
            catalog_id: test_catalog_id(),
            name: "test task".to_string(),
            context: "test context".to_string(),
            created_at: Utc::now(),
        };
        ts.state.project_store.create(task.clone()).await.unwrap();
        task
    }

    #[tokio::test]
    async fn test_upload_creates_new_photo() {
        let ts = create_test_state().await;
        let task = create_task_in_store(&ts).await;
        let boundary = "boundary_new";
        let body = build_multipart_body(
            boundary,
            Some(vec!["lr:001"]),
            vec![("photo.jpg", jpeg_data())],
        );
        let multipart = make_multipart(boundary, body).await;

        let (status, Json(response)) =
            upload_photos(State(ts.state.clone()), AppPath(task.project_id), multipart)
                .await
                .unwrap();

        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(response.created_count, 1);
        assert_eq!(response.replaced_count, 0);
        assert_eq!(response.failed_count, 0);
        assert!(!response.uploaded[0].replaced);
        assert_eq!(response.uploaded[0].client_id, Some("lr:001".to_string()));
    }

    #[tokio::test]
    async fn test_upload_replaces_existing_photo() {
        let ts = create_test_state().await;
        let task = create_task_in_store(&ts).await;

        // First upload
        let body1 = build_multipart_body(
            "boundary_1",
            Some(vec!["lr:001"]),
            vec![("photo_v1.jpg", jpeg_data())],
        );
        upload_photos(
            State(ts.state.clone()),
            AppPath(task.project_id),
            make_multipart("boundary_1", body1).await,
        )
        .await
        .unwrap();

        let photos_before = ts
            .state
            .photo_store
            .list_by_task(task.project_id)
            .await
            .unwrap();
        assert_eq!(photos_before.len(), 1);
        let original_photo_id = photos_before[0].photo_id;

        // Re-upload with same client_id
        let body2 = build_multipart_body(
            "boundary_2",
            Some(vec!["lr:001"]),
            vec![("photo_v2.jpg", jpeg_data())],
        );
        let (status, Json(response)) = upload_photos(
            State(ts.state.clone()),
            AppPath(task.project_id),
            make_multipart("boundary_2", body2).await,
        )
        .await
        .unwrap();

        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(response.created_count, 0);
        assert_eq!(response.replaced_count, 1);
        assert_eq!(response.failed_count, 0);
        assert!(response.uploaded[0].replaced);
        assert_eq!(response.uploaded[0].photo_id, original_photo_id);
        assert_eq!(response.uploaded[0].filename, "photo_v2.jpg");
        assert_eq!(response.uploaded[0].client_id, Some("lr:001".to_string()));

        // photo_id preserved and store still has only one photo
        let photos_after = ts
            .state
            .photo_store
            .list_by_task(task.project_id)
            .await
            .unwrap();
        assert_eq!(photos_after.len(), 1);
        assert_eq!(photos_after[0].photo_id, original_photo_id);
        assert_eq!(photos_after[0].filename, "photo_v2.jpg");
    }

    #[tokio::test]
    async fn test_upload_no_client_id_always_creates_new() {
        let ts = create_test_state().await;
        let task = create_task_in_store(&ts).await;

        for _ in 0..2 {
            let body = build_multipart_body("boundary", None, vec![("photo.jpg", jpeg_data())]);
            upload_photos(
                State(ts.state.clone()),
                AppPath(task.project_id),
                make_multipart("boundary", body).await,
            )
            .await
            .unwrap();
        }

        let photos = ts
            .state
            .photo_store
            .list_by_task(task.project_id)
            .await
            .unwrap();
        assert_eq!(photos.len(), 2);
    }

    #[tokio::test]
    async fn test_upload_mixed_new_and_replace_in_one_request() {
        let ts = create_test_state().await;
        let task = create_task_in_store(&ts).await;

        // Seed: one existing photo with lr:001
        let body1 = build_multipart_body(
            "boundary_seed",
            Some(vec!["lr:001"]),
            vec![("existing.jpg", jpeg_data())],
        );
        upload_photos(
            State(ts.state.clone()),
            AppPath(task.project_id),
            make_multipart("boundary_seed", body1).await,
        )
        .await
        .unwrap();

        // Upload: lr:001 (replace) + lr:002 (new) in one request
        let body2 = build_multipart_body(
            "boundary_batch",
            Some(vec!["lr:001", "lr:002"]),
            vec![("existing_v2.jpg", jpeg_data()), ("new.jpg", jpeg_data())],
        );
        let (_, Json(response)) = upload_photos(
            State(ts.state.clone()),
            AppPath(task.project_id),
            make_multipart("boundary_batch", body2).await,
        )
        .await
        .unwrap();

        assert_eq!(response.created_count, 1);
        assert_eq!(response.replaced_count, 1);
        assert_eq!(response.failed_count, 0);

        let photos = ts
            .state
            .photo_store
            .list_by_task(task.project_id)
            .await
            .unwrap();
        assert_eq!(photos.len(), 2);
    }
}
