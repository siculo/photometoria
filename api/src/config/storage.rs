use super::ByteSize;
use serde::Deserialize;

/// Storage configuration section
#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    pub path: String,
    pub max_size: ByteSize,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            path: "./storage".to_string(),
            max_size: "10GiB".parse().unwrap(),
        }
    }
}
