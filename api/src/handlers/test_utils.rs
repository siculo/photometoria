// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! Test utilities for handler tests
//!
//! This module provides common test setup functions and types used across
//! all handler test modules to avoid duplication.

#[cfg(test)]
pub mod fixtures {
    use async_trait::async_trait;

    use crate::app_state::AppState;
    use crate::config::Config;
    use crate::services::ai::{
        AIProvider, AIProviderError, AIProviderResult, AnalyzeImageRequest, AnalyzeImageResponse,
        HealthStatus, ModelInfo, ProviderRegistry,
    };
    use crate::services::worker::WorkerPool;
    use crate::storage::{FileSystemJobStore, FileSystemPhotoStore, FileSystemTaskStore};
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::Mutex;

    /// Minimal AI provider for handler tests.
    ///
    /// Returns a fixed set of configured model IDs so that model validation
    /// in `create_job` passes for the models used across handler tests.
    /// Network methods are never called in handler tests.
    struct TestProvider;

    #[async_trait]
    impl AIProvider for TestProvider {
        fn name(&self) -> &str {
            "test"
        }

        fn configured_model_ids(&self) -> Vec<String> {
            vec!["qwen3-vl:8b".to_string(), "llava".to_string()]
        }

        fn configured_model_details(&self) -> Vec<crate::services::ai::ConfiguredModelInfo> {
            vec![]
        }

        async fn check_health(&self) -> AIProviderResult<HealthStatus> {
            Err(AIProviderError::Unavailable {
                provider: "test".into(),
                message: "not available in tests".into(),
            })
        }

        async fn list_models(&self, _vision_only: bool) -> AIProviderResult<Vec<ModelInfo>> {
            Ok(vec![])
        }

        async fn analyze_image(
            &self,
            _request: AnalyzeImageRequest,
        ) -> AIProviderResult<AnalyzeImageResponse> {
            Err(AIProviderError::Unavailable {
                provider: "test".into(),
                message: "not available in tests".into(),
            })
        }
    }

    /// Test state wrapper that holds an AppState and keeps the temp directory alive
    pub struct TestState {
        pub state: AppState,
        #[allow(dead_code)]
        temp_dir: TempDir,
    }

    /// Creates a fresh test state with filesystem-backed stores for each test
    ///
    /// # Example
    /// ```
    /// use crate::handlers::test_utils::fixtures::create_test_state;
    ///
    /// #[tokio::test]
    /// async fn my_test() {
    ///     let ts = create_test_state().await;
    ///     // Use ts.state for testing
    /// }
    /// ```
    pub async fn create_test_state() -> TestState {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();
        let config = Config::default();
        let task_store = Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
        let photo_store = Arc::new(FileSystemPhotoStore::new(storage_path.clone()).await);
        let job_store = Arc::new(FileSystemJobStore::new(storage_path).await);

        let mut registry = ProviderRegistry::new();
        registry.register("test", Arc::new(TestProvider));
        registry.set_default("test").unwrap();
        let ai_providers = Arc::new(registry);

        let worker_pool = Arc::new(Mutex::new(WorkerPool::new_inactive(job_store.clone())));
        TestState {
            state: AppState::new(
                config,
                task_store,
                photo_store,
                job_store,
                ai_providers,
                worker_pool,
            ),
            temp_dir,
        }
    }
}
