use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::Multipart;
use axum::{
    extract::{Json, Path, State},
    http::StatusCode,
};

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
    // Verify task exists
    let task_exists = state
        .task_store
        .exists(&task_id)
        .await
        .map_err(to_internal_server_error)?;
    if !task_exists {
        return Err(StatusCode::NOT_FOUND);
    }

    let (uploaded, failed, total_size_bytes) = process_multipart(&state, &task_id, &mut multipart).await?;

    let status = if uploaded.is_empty() {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };

    let response = UploadPhotosResponse {
        uploaded,
        failed,
        total_size_bytes,
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
    let mut total_size_bytes: u64 = 0;

    while let Some(field) = multipart.next_field().await.map_err(to_bad_request)? {
        let filename = field.file_name().unwrap_or_default().to_string();
        let data = field.bytes().await.map_err(to_bad_request)?;

        let result = process_field(
            data,
            filename,
            &task_id,
            uploaded.len(),
            used_storage,
            &state.config,
            &state.photo_store,
        )
            .await;

        match result {
            ProcessedField::Uploaded(photo, size) => {
                used_storage += size;
                total_size_bytes += size;
                uploaded.push(photo);
            }
            ProcessedField::Failed(failure) => {
                failed.push(failure);
            }
        }
    }
    Ok((uploaded, failed, total_size_bytes))
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
            // TODO: Save actual image data to disk
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

fn to_bad_request<E>(_: E) -> StatusCode {
    StatusCode::BAD_REQUEST
}

fn to_internal_server_error<E>(_: E) -> StatusCode {
    StatusCode::INTERNAL_SERVER_ERROR
}
