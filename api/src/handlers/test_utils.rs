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
    /// Simulates a healthy provider with two configured and available models:
    /// `"qwen3-vl:8b"` and `"llava"`. Used by most handler tests so that
    /// `check_model_available` passes without network calls.
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
            vec![
                crate::services::ai::ConfiguredModelInfo {
                    id: "qwen3-vl:8b".to_string(),
                    backend_model_name: "qwen3-vl:8b".to_string(),
                    description: None,
                    supported_languages: vec![],
                },
                crate::services::ai::ConfiguredModelInfo {
                    id: "llava".to_string(),
                    backend_model_name: "llava".to_string(),
                    description: None,
                    supported_languages: vec![],
                },
            ]
        }

        async fn check_health(&self) -> AIProviderResult<HealthStatus> {
            Ok(HealthStatus {
                healthy: true,
                message: None,
                available_models: Some(2),
            })
        }

        async fn list_models(&self, _vision_only: bool) -> AIProviderResult<Vec<ModelInfo>> {
            Ok(vec![
                ModelInfo {
                    id: "qwen3-vl:8b".to_string(),
                    name: "qwen3-vl:8b".to_string(),
                    description: None,
                    supports_vision: true,
                    provider: "test".to_string(),
                },
                ModelInfo {
                    id: "llava".to_string(),
                    name: "llava".to_string(),
                    description: None,
                    supports_vision: true,
                    provider: "test".to_string(),
                },
            ])
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

    /// AI provider that has `"qwen3-vl:8b"` configured but returns an empty
    /// model list from the backend — simulating a model not installed in Ollama.
    struct TestProviderModelNotAvailable;

    #[async_trait]
    impl AIProvider for TestProviderModelNotAvailable {
        fn name(&self) -> &str {
            "test"
        }

        fn configured_model_ids(&self) -> Vec<String> {
            vec!["qwen3-vl:8b".to_string()]
        }

        fn configured_model_details(&self) -> Vec<crate::services::ai::ConfiguredModelInfo> {
            vec![crate::services::ai::ConfiguredModelInfo {
                id: "qwen3-vl:8b".to_string(),
                backend_model_name: "qwen3-vl:8b".to_string(),
                description: None,
                supported_languages: vec![],
            }]
        }

        async fn check_health(&self) -> AIProviderResult<HealthStatus> {
            Ok(HealthStatus {
                healthy: true,
                message: None,
                available_models: Some(0),
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

    /// AI provider that simulates an unreachable backend (Ollama is down).
    struct TestProviderUnavailable;

    #[async_trait]
    impl AIProvider for TestProviderUnavailable {
        fn name(&self) -> &str {
            "test"
        }

        fn configured_model_ids(&self) -> Vec<String> {
            vec!["qwen3-vl:8b".to_string()]
        }

        fn configured_model_details(&self) -> Vec<crate::services::ai::ConfiguredModelInfo> {
            vec![crate::services::ai::ConfiguredModelInfo {
                id: "qwen3-vl:8b".to_string(),
                backend_model_name: "qwen3-vl:8b".to_string(),
                description: None,
                supported_languages: vec![],
            }]
        }

        async fn check_health(&self) -> AIProviderResult<HealthStatus> {
            Err(AIProviderError::Unavailable {
                provider: "test".into(),
                message: "cannot connect".into(),
            })
        }

        async fn list_models(&self, _vision_only: bool) -> AIProviderResult<Vec<ModelInfo>> {
            Err(AIProviderError::Unavailable {
                provider: "test".into(),
                message: "cannot connect".into(),
            })
        }

        async fn analyze_image(
            &self,
            _request: AnalyzeImageRequest,
        ) -> AIProviderResult<AnalyzeImageResponse> {
            Err(AIProviderError::Unavailable {
                provider: "test".into(),
                message: "cannot connect".into(),
            })
        }
    }

    /// Test state wrapper that holds an AppState and keeps the temp directory alive
    pub struct TestState {
        pub state: AppState,
        #[allow(dead_code)]
        temp_dir: TempDir,
    }

    /// Returns a fixed catalog UUID for use in handler tests.
    pub fn test_catalog_id() -> uuid::Uuid {
        uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }

    /// Creates a test state where the configured model is not installed in the backend.
    pub async fn create_test_state_model_not_available() -> TestState {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();
        let config = Config::default();
        let task_store = Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
        let photo_store =
            Arc::new(FileSystemPhotoStore::new(storage_path.clone(), task_store.clone()).await);
        let job_store = Arc::new(FileSystemJobStore::new(storage_path, task_store.clone()).await);

        let mut registry = ProviderRegistry::new();
        registry.register("test", Arc::new(TestProviderModelNotAvailable));
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

    /// Creates a test state where the AI provider backend is unreachable.
    pub async fn create_test_state_provider_unavailable() -> TestState {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();
        let config = Config::default();
        let task_store = Arc::new(FileSystemTaskStore::new(storage_path.clone()).await);
        let photo_store =
            Arc::new(FileSystemPhotoStore::new(storage_path.clone(), task_store.clone()).await);
        let job_store = Arc::new(FileSystemJobStore::new(storage_path, task_store.clone()).await);

        let mut registry = ProviderRegistry::new();
        registry.register("test", Arc::new(TestProviderUnavailable));
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
        let photo_store =
            Arc::new(FileSystemPhotoStore::new(storage_path.clone(), task_store.clone()).await);
        let job_store = Arc::new(FileSystemJobStore::new(storage_path, task_store.clone()).await);

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
