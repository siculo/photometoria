// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! Catalog handlers
//!
//! This module contains handler functions for catalog-related endpoints.

use std::collections::HashMap;

use axum::Json;
use axum::extract::State;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::handlers::app_error::AppError;
use crate::models::CatalogSummary;

/// Handler for GET /api/catalogs
///
/// Lists all catalogs that have at least one task, with aggregated statistics.
/// The `created_at` field reflects the earliest task creation date in the catalog.
pub async fn list_catalogs(
    State(state): State<AppState>,
) -> Result<Json<Vec<CatalogSummary>>, AppError> {
    let tasks = state
        .task_store
        .list()
        .await
        .map_err(|e| AppError::internal_error(e.to_string()))?;

    let mut catalog_map: HashMap<Uuid, (usize, DateTime<Utc>)> = HashMap::new();
    for task in tasks {
        let entry = catalog_map
            .entry(task.catalog_id)
            .or_insert((0, task.created_at));
        entry.0 += 1;
        if task.created_at < entry.1 {
            entry.1 = task.created_at;
        }
    }

    let mut summaries: Vec<CatalogSummary> = catalog_map
        .into_iter()
        .map(|(catalog_id, (task_count, created_at))| CatalogSummary {
            catalog_id,
            task_count,
            created_at,
        })
        .collect();

    summaries.sort_by_key(|s| s.created_at);

    Ok(Json(summaries))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::app_error::{AppJson, AppPath};
    use crate::handlers::tasks::create_task;
    use crate::handlers::test_utils::fixtures::{create_test_state, test_catalog_id};
    use crate::models::CreateTaskRequest;
    use axum::extract::State;

    #[tokio::test]
    async fn test_list_catalogs_empty() {
        let ts = create_test_state().await;

        let Json(catalogs) = list_catalogs(State(ts.state.clone())).await.unwrap();

        assert!(catalogs.is_empty());
    }

    #[tokio::test]
    async fn test_list_catalogs_single_catalog_task_count() {
        let ts = create_test_state().await;
        let catalog_id = test_catalog_id();

        let _ = create_task(
            State(ts.state.clone()),
            AppPath(catalog_id),
            AppJson(CreateTaskRequest {
                name: "Task 1".to_string(),
                context: "ctx".to_string(),
            }),
        )
        .await
        .unwrap();
        let _ = create_task(
            State(ts.state.clone()),
            AppPath(catalog_id),
            AppJson(CreateTaskRequest {
                name: "Task 2".to_string(),
                context: "ctx".to_string(),
            }),
        )
        .await
        .unwrap();

        let Json(catalogs) = list_catalogs(State(ts.state.clone())).await.unwrap();

        assert_eq!(catalogs.len(), 1);
        assert_eq!(catalogs[0].catalog_id, catalog_id);
        assert_eq!(catalogs[0].task_count, 2);
    }

    #[tokio::test]
    async fn test_list_catalogs_multiple_catalogs() {
        let ts = create_test_state().await;
        let catalog_a = test_catalog_id();
        let catalog_b = Uuid::new_v4();

        let _ = create_task(
            State(ts.state.clone()),
            AppPath(catalog_a),
            AppJson(CreateTaskRequest {
                name: "Cat A Task".to_string(),
                context: "ctx".to_string(),
            }),
        )
        .await
        .unwrap();
        let _ = create_task(
            State(ts.state.clone()),
            AppPath(catalog_b),
            AppJson(CreateTaskRequest {
                name: "Cat B Task 1".to_string(),
                context: "ctx".to_string(),
            }),
        )
        .await
        .unwrap();
        let _ = create_task(
            State(ts.state.clone()),
            AppPath(catalog_b),
            AppJson(CreateTaskRequest {
                name: "Cat B Task 2".to_string(),
                context: "ctx".to_string(),
            }),
        )
        .await
        .unwrap();

        let Json(catalogs) = list_catalogs(State(ts.state.clone())).await.unwrap();

        assert_eq!(catalogs.len(), 2);

        let cat_a = catalogs.iter().find(|c| c.catalog_id == catalog_a).unwrap();
        assert_eq!(cat_a.task_count, 1);

        let cat_b = catalogs.iter().find(|c| c.catalog_id == catalog_b).unwrap();
        assert_eq!(cat_b.task_count, 2);
    }

    #[tokio::test]
    async fn test_list_catalogs_created_at_is_earliest_task() {
        let ts = create_test_state().await;
        let catalog_id = test_catalog_id();

        let (_, axum::Json(task1)) = create_task(
            State(ts.state.clone()),
            AppPath(catalog_id),
            AppJson(CreateTaskRequest {
                name: "First Task".to_string(),
                context: "ctx".to_string(),
            }),
        )
        .await
        .unwrap();
        let (_, axum::Json(task2)) = create_task(
            State(ts.state.clone()),
            AppPath(catalog_id),
            AppJson(CreateTaskRequest {
                name: "Second Task".to_string(),
                context: "ctx".to_string(),
            }),
        )
        .await
        .unwrap();

        let Json(catalogs) = list_catalogs(State(ts.state.clone())).await.unwrap();

        let earliest = task1.created_at.min(task2.created_at);
        assert_eq!(catalogs[0].created_at, earliest);
    }
}
