use serde::Deserialize;

/// Server configuration section.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// Bind address for the HTTP server.
    pub host: String,
    /// TCP port to listen on.
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 3000,
        }
    }
}
