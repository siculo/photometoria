use std::sync::Arc;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::Utc;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::models::job::{JobStatus, PhotoResult, PhotoResultStatus};
use crate::services::ai::{AIProvider, AnalyzeImageRequest};
use crate::storage::{JobStore, PhotoStore, TaskStore};

use super::queue::QueuedPhoto;

const BASE_PROMPT: &str = "Analyze this image and generate keywords for a photo library. \
    Return a comma-separated list of relevant keywords covering: subject matter, \
    colors, mood, style, and any notable elements. \
    Return only the comma-separated keywords, no other text.";

// ============================================================================
// ProcessingResult
// ============================================================================

/// Result of processing a single photo.
#[derive(Debug)]
pub struct ProcessingResult {
    pub photo_id: Uuid,
    pub tags: Option<String>,
    pub success: bool,
    pub error: Option<String>,
}

// ============================================================================
// PhotoProcessor
// ============================================================================

/// Handles the AI analysis and job state update for a single photo.
pub struct PhotoProcessor {
    ai_provider: Arc<dyn AIProvider>,
    job_store: Arc<dyn JobStore>,
    photo_store: Arc<dyn PhotoStore>,
    task_store: Arc<dyn TaskStore>,
}

impl PhotoProcessor {
    pub fn new(
        ai_provider: Arc<dyn AIProvider>,
        job_store: Arc<dyn JobStore>,
        photo_store: Arc<dyn PhotoStore>,
        task_store: Arc<dyn TaskStore>,
    ) -> Self {
        Self {
            ai_provider,
            job_store,
            photo_store,
            task_store,
        }
    }

    /// Process a single photo: load data, call AI, update job state.
    pub async fn process(&self, photo: QueuedPhoto) -> ProcessingResult {
        debug!(
            photo_id = %photo.photo_id,
            job_id = %photo.job_id,
            model = %photo.model,
            "Processing photo"
        );

        let result = self.analyze(&photo).await;
        self.update_job(&photo, &result).await;
        result
    }

    /// Load photo data, build request, call AI provider.
    async fn analyze(&self, photo: &QueuedPhoto) -> ProcessingResult {
        // Load raw image bytes
        let bytes = match self.photo_store.load_data(photo.photo_id).await {
            Ok(data) => data,
            Err(e) => {
                error!(photo_id = %photo.photo_id, error = %e, "Failed to load photo data");
                return ProcessingResult {
                    photo_id: photo.photo_id,
                    tags: None,
                    success: false,
                    error: Some(format!("Failed to load photo: {}", e)),
                };
            }
        };

        // Load task context — task must exist: a job cannot outlive its task
        let context = match self.load_task_context(photo.task_id).await {
            Ok(ctx) => ctx,
            Err(e) => {
                error!(
                    photo_id = %photo.photo_id,
                    task_id = %photo.task_id,
                    error = %e,
                    "Task not found or unavailable — cannot process photo"
                );
                return ProcessingResult {
                    photo_id: photo.photo_id,
                    tags: None,
                    success: false,
                    error: Some(e),
                };
            }
        };

        let prompt = if context.is_empty() {
            BASE_PROMPT.to_string()
        } else {
            format!("Context: {}\n\n{}", context, BASE_PROMPT)
        };

        let request = AnalyzeImageRequest {
            model: photo.model.clone(),
            image_base64: STANDARD.encode(&bytes),
            prompt,
        };

        match self.ai_provider.analyze_image(request).await {
            Ok(response) => {
                info!(
                    photo_id = %photo.photo_id,
                    model = %response.model,
                    "Photo analysis completed"
                );
                ProcessingResult {
                    photo_id: photo.photo_id,
                    tags: Some(response.text),
                    success: true,
                    error: None,
                }
            }
            Err(e) => {
                error!(photo_id = %photo.photo_id, error = %e, "Photo analysis failed");
                ProcessingResult {
                    photo_id: photo.photo_id,
                    tags: None,
                    success: false,
                    error: Some(e.to_string()),
                }
            }
        }
    }

    /// Loads the task context string.
    ///
    /// Returns `Ok(context)` if the task exists (context may be empty if the
    /// user provided none). Returns `Err` if the task is not found or if the
    /// store fails — a missing task is a logic error because a job cannot
    /// outlive its parent task.
    async fn load_task_context(&self, task_id: Uuid) -> Result<String, String> {
        match self.task_store.get(task_id).await {
            Ok(Some(task)) => Ok(task.context),
            Ok(None) => Err(format!("Task {} not found", task_id)),
            Err(e) => Err(format!("Failed to load task {}: {}", task_id, e)),
        }
    }

    /// Update job state after processing a photo (success or failure).
    async fn update_job(&self, photo: &QueuedPhoto, result: &ProcessingResult) {
        let mut job = match self.job_store.get(photo.job_id).await {
            Ok(Some(job)) => job,
            Ok(None) => {
                error!(job_id = %photo.job_id, "Job not found when updating after photo processing");
                return;
            }
            Err(e) => {
                error!(job_id = %photo.job_id, error = %e, "Failed to load job for update");
                return;
            }
        };

        // Transition Queued → Processing on the first photo
        if job.status == JobStatus::Queued {
            job.start();
        }

        // Move photo from queued to processed
        job.queued_photo_ids.retain(|id| id != &photo.photo_id);
        job.processed_photo_ids.push(photo.photo_id);

        // Store result
        let photo_result = Self::build_photo_result(photo.photo_id, result);
        job.results.insert(photo.photo_id, photo_result);

        // Complete job when all photos have been processed
        if job.queued_photo_ids.is_empty() {
            job.complete();
            info!(job_id = %photo.job_id, "Job completed");
        }

        if let Err(e) = self.job_store.update(job).await {
            error!(job_id = %photo.job_id, error = %e, "Failed to persist job update");
        }
    }

    fn build_photo_result(photo_id: Uuid, result: &ProcessingResult) -> PhotoResult {
        if result.success {
            PhotoResult {
                photo_id,
                status: PhotoResultStatus::Completed,
                tags: result.tags.clone(),
                error: None,
                processed_at: Some(Utc::now()),
            }
        } else {
            PhotoResult {
                photo_id,
                status: PhotoResultStatus::Failed,
                tags: None,
                error: result.error.clone(),
                processed_at: Some(Utc::now()),
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;

    use async_trait::async_trait;
    use tempfile::TempDir;
    use uuid::Uuid;

    use crate::models::{Job, Task};
    use crate::services::ai::{
        AIProviderError, AIProviderResult, AnalyzeImageResponse, HealthStatus, ModelInfo,
    };
    use crate::storage::{FileSystemJobStore, FileSystemPhotoStore, FileSystemTaskStore};

    // -----------------------------------------------------------------------
    // Mock AI provider
    // -----------------------------------------------------------------------

    struct SuccessProvider {
        response_text: String,
    }

    #[async_trait]
    impl AIProvider for SuccessProvider {
        fn name(&self) -> &str {
            "mock-success"
        }
        async fn check_health(&self) -> AIProviderResult<HealthStatus> {
            unimplemented!()
        }
        async fn list_models(&self, _vision_only: bool) -> AIProviderResult<Vec<ModelInfo>> {
            unimplemented!()
        }
        async fn analyze_image(
            &self,
            request: AnalyzeImageRequest,
        ) -> AIProviderResult<AnalyzeImageResponse> {
            Ok(AnalyzeImageResponse {
                text: self.response_text.clone(),
                model: request.model,
                tokens_used: None,
            })
        }
    }

    struct FailingProvider {
        error_message: String,
    }

    #[async_trait]
    impl AIProvider for FailingProvider {
        fn name(&self) -> &str {
            "mock-fail"
        }
        async fn check_health(&self) -> AIProviderResult<HealthStatus> {
            unimplemented!()
        }
        async fn list_models(&self, _vision_only: bool) -> AIProviderResult<Vec<ModelInfo>> {
            unimplemented!()
        }
        async fn analyze_image(
            &self,
            _request: AnalyzeImageRequest,
        ) -> AIProviderResult<AnalyzeImageResponse> {
            Err(AIProviderError::RequestFailed {
                provider: "mock-fail".to_string(),
                message: self.error_message.clone(),
            })
        }
    }

    // -----------------------------------------------------------------------
    // Test fixtures
    // -----------------------------------------------------------------------

    struct TestFixture {
        processor: PhotoProcessor,
        job_store: Arc<FileSystemJobStore>,
        photo_store: Arc<FileSystemPhotoStore>,
        task_store: Arc<FileSystemTaskStore>,
        #[allow(dead_code)]
        temp_dir: TempDir,
    }

    async fn make_fixture(ai_provider: Arc<dyn AIProvider>) -> TestFixture {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().to_path_buf();
        let task_store = Arc::new(FileSystemTaskStore::new(path.clone()).await);
        let photo_store = Arc::new(FileSystemPhotoStore::new(path.clone()).await);
        let job_store = Arc::new(FileSystemJobStore::new(path.clone()).await);

        let processor = PhotoProcessor::new(
            ai_provider,
            job_store.clone(),
            photo_store.clone(),
            task_store.clone(),
        );

        TestFixture {
            processor,
            job_store,
            photo_store,
            task_store,
            temp_dir,
        }
    }

    async fn setup_job_and_photo(
        fixture: &TestFixture,
    ) -> (Job, Uuid) {
        let task = fixture
            .task_store
            .create(Task::new("test context".to_string()))
            .await
            .unwrap();

        let photo_id = Uuid::new_v4();
        let photo = crate::models::Photo::new(task.task_id, "test.jpg".to_string(), 100);
        let photo = crate::models::Photo { photo_id, ..photo };
        fixture.photo_store.create(photo).await.unwrap();
        fixture
            .photo_store
            .save_data(photo_id, b"fake-image-bytes")
            .await
            .unwrap();

        let job = fixture
            .job_store
            .create(Job::new(
                task.task_id,
                "llava".to_string(),
                vec![photo_id],
            ))
            .await
            .unwrap();

        (job, photo_id)
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_process_success_updates_job_to_completed() {
        let provider = Arc::new(SuccessProvider {
            response_text: "landscape, mountain, sunset".to_string(),
        });
        let fixture = make_fixture(provider).await;
        let (job, photo_id) = setup_job_and_photo(&fixture).await;

        let queued_photo = QueuedPhoto {
            job_id: job.job_id,
            photo_id,
            task_id: job.task_id,
            model: "llava".to_string(),
        };

        let result = fixture.processor.process(queued_photo).await;

        assert!(result.success);
        assert_eq!(result.tags.as_deref(), Some("landscape, mountain, sunset"));
        assert!(result.error.is_none());

        let updated_job = fixture.job_store.get(job.job_id).await.unwrap().unwrap();
        assert_eq!(updated_job.status, JobStatus::Completed);
        assert!(updated_job.queued_photo_ids.is_empty());
        assert!(updated_job.processed_photo_ids.contains(&photo_id));
        let photo_result = updated_job.results.get(&photo_id).unwrap();
        assert_eq!(photo_result.status, PhotoResultStatus::Completed);
        assert_eq!(photo_result.tags.as_deref(), Some("landscape, mountain, sunset"));
    }

    #[tokio::test]
    async fn test_process_failure_marks_photo_as_failed() {
        let provider = Arc::new(FailingProvider {
            error_message: "model timeout".to_string(),
        });
        let fixture = make_fixture(provider).await;
        let (job, photo_id) = setup_job_and_photo(&fixture).await;

        let queued_photo = QueuedPhoto {
            job_id: job.job_id,
            photo_id,
            task_id: job.task_id,
            model: "llava".to_string(),
        };

        let result = fixture.processor.process(queued_photo).await;

        assert!(!result.success);
        assert!(result.tags.is_none());
        assert!(result.error.is_some());

        let updated_job = fixture.job_store.get(job.job_id).await.unwrap().unwrap();
        assert_eq!(updated_job.status, JobStatus::Completed);
        assert!(updated_job.queued_photo_ids.is_empty());
        let photo_result = updated_job.results.get(&photo_id).unwrap();
        assert_eq!(photo_result.status, PhotoResultStatus::Failed);
        assert!(photo_result.error.is_some());
    }

    #[tokio::test]
    async fn test_process_transitions_queued_to_processing_then_completed() {
        let provider = Arc::new(SuccessProvider {
            response_text: "street, city".to_string(),
        });
        let fixture = make_fixture(provider).await;
        let (job, photo_id) = setup_job_and_photo(&fixture).await;

        assert_eq!(job.status, JobStatus::Queued);

        let queued_photo = QueuedPhoto {
            job_id: job.job_id,
            photo_id,
            task_id: job.task_id,
            model: "llava".to_string(),
        };

        fixture.processor.process(queued_photo).await;

        let updated_job = fixture.job_store.get(job.job_id).await.unwrap().unwrap();
        // Started and immediately completed (single photo job)
        assert_eq!(updated_job.status, JobStatus::Completed);
        assert!(updated_job.started_at.is_some());
        assert!(updated_job.completed_at.is_some());
    }

    #[tokio::test]
    async fn test_process_multi_photo_job_not_completed_until_last_photo() {
        let provider = Arc::new(SuccessProvider {
            response_text: "test".to_string(),
        });
        let fixture = make_fixture(provider).await;

        let task = fixture
            .task_store
            .create(Task::new("context".to_string()))
            .await
            .unwrap();

        // Create two photos
        let photo_id_1 = Uuid::new_v4();
        let photo_id_2 = Uuid::new_v4();

        for (id, name) in [(photo_id_1, "a.jpg"), (photo_id_2, "b.jpg")] {
            let photo = crate::models::Photo::new(task.task_id, name.to_string(), 100);
            let photo = crate::models::Photo { photo_id: id, ..photo };
            fixture.photo_store.create(photo).await.unwrap();
            fixture.photo_store.save_data(id, b"bytes").await.unwrap();
        }

        let job = fixture
            .job_store
            .create(Job::new(
                task.task_id,
                "llava".to_string(),
                vec![photo_id_1, photo_id_2],
            ))
            .await
            .unwrap();

        // Process first photo
        fixture
            .processor
            .process(QueuedPhoto {
                job_id: job.job_id,
                photo_id: photo_id_1,
                task_id: task.task_id,
                model: "llava".to_string(),
            })
            .await;

        let job_after_first = fixture.job_store.get(job.job_id).await.unwrap().unwrap();
        assert_eq!(job_after_first.status, JobStatus::Processing);
        assert_eq!(job_after_first.queued_photo_ids.len(), 1);
        assert_eq!(job_after_first.processed_photo_ids.len(), 1);

        // Process second photo
        fixture
            .processor
            .process(QueuedPhoto {
                job_id: job.job_id,
                photo_id: photo_id_2,
                task_id: task.task_id,
                model: "llava".to_string(),
            })
            .await;

        let job_after_second = fixture.job_store.get(job.job_id).await.unwrap().unwrap();
        assert_eq!(job_after_second.status, JobStatus::Completed);
        assert!(job_after_second.queued_photo_ids.is_empty());
        assert_eq!(job_after_second.processed_photo_ids.len(), 2);
    }

    #[tokio::test]
    async fn test_process_missing_photo_data_returns_failure() {
        let provider = Arc::new(SuccessProvider {
            response_text: "should not reach".to_string(),
        });
        let fixture = make_fixture(provider).await;

        let task = fixture
            .task_store
            .create(Task::new(String::new()))
            .await
            .unwrap();

        // Create photo metadata but no binary data
        let photo_id = Uuid::new_v4();
        let photo = crate::models::Photo::new(task.task_id, "ghost.jpg".to_string(), 0);
        let photo = crate::models::Photo { photo_id, ..photo };
        fixture.photo_store.create(photo).await.unwrap();

        let job = fixture
            .job_store
            .create(Job::new(task.task_id, "llava".to_string(), vec![photo_id]))
            .await
            .unwrap();

        let result = fixture
            .processor
            .process(QueuedPhoto {
                job_id: job.job_id,
                photo_id,
                task_id: task.task_id,
                model: "llava".to_string(),
            })
            .await;

        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn test_process_missing_task_returns_failure() {
        let provider = Arc::new(SuccessProvider {
            response_text: "should not reach".to_string(),
        });
        let fixture = make_fixture(provider).await;

        // Create the task and photo normally, then delete the task to
        // simulate the case where a task is removed while a job is still running
        // (should not happen in production, but must be handled defensively).
        let (job, photo_id) = setup_job_and_photo(&fixture).await;
        fixture.task_store.delete(job.task_id).await.unwrap();

        let result = fixture
            .processor
            .process(QueuedPhoto {
                job_id: job.job_id,
                photo_id,
                task_id: job.task_id,
                model: "llava".to_string(),
            })
            .await;

        assert!(!result.success);
        assert!(result.error.as_deref().unwrap().contains("not found"));
    }
}
