use serde::Deserialize;
use super::ByteSize;

/// Upload configuration section
#[derive(Debug, Clone, Deserialize)]
pub struct UploadConfig {
    pub max_photos_per_request: usize,
    pub max_photo_size: ByteSize,
}

impl Default for UploadConfig {
    fn default() -> Self {
        Self {
            max_photos_per_request: 100,
            max_photo_size: "20MB".parse().unwrap(),
        }
    }
}
