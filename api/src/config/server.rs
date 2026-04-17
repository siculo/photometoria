// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

use serde::Deserialize;

fn default_server_name() -> String {
    "Photometoria Server".to_string()
}

/// Server configuration section.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// Human-readable name for this server instance.
    #[serde(default = "default_server_name")]
    pub name: String,
    /// Bind address for the HTTP server.
    pub host: String,
    /// TCP port to listen on.
    pub port: u16,
}

impl ServerConfig {
    /// Appends this section's summary to the output buffer.
    pub fn format_summary(&self, out: &mut String) {
        out.push_str(&format!("  [server]\n"));
        out.push_str(&format!("    name: {}\n", self.name));
        out.push_str(&format!("    host: {}\n", self.host));
        out.push_str(&format!("    port: {}\n", self.port));
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            name: default_server_name(),
            host: "0.0.0.0".to_string(),
            port: 3000,
        }
    }
}
