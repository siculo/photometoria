// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

use axum::Json;
use serde_json::{Value, json};

/// Liveness probe. Returns 200 with a minimal body.
///
/// Registered outside the API version middleware so monitoring tools and the
/// install script can check service health without negotiating an API version.
pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_returns_ok() {
        let Json(body) = health().await;
        assert_eq!(body["status"], "ok");
    }
}
