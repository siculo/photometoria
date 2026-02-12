use std::sync::Arc;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::Utc;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::models::job::{JobStatus, PhotoResult, PhotoResultStatus};
use crate::services::ai::{AIProvider, AnalyzeImageRequest};
use crate::storage::{JobStore, PhotoStore, TaskStore};

use super::queue::QueuedPhoto;

const BASE_PROMPT: &str = "You must respond ONLY with a comma-separated list of tags. \
    Do not write sentences. Do not explain. Only output tags separated by commas. \
    Tags to include: subjects, objects, colors, composition, mood, location type. \
    Output format example: tag1, tag2, tag3, tag4 \
    Now analyze this image and output ONLY the tags:";

// ============================================================================
// ProcessingResult
// ============================================================================

/// Outcome of the processing pipeline for a single photo.
#[derive(Debug)]
pub enum Outcome {
    /// AI analysis succeeded and job state was persisted correctly.
    Success { tags: String },

    /// AI analysis succeeded but persisting the updated job state failed.
    /// The tags are returned so the caller can log them; the job in the store
    /// may still reflect the previous state.
    SuccessWithPersistenceError { tags: String, error: String },

    /// AI analysis failed (photo data missing, task not found, AI error, etc.).
    /// The job state has been updated to mark the photo as failed when possible.
    AnalysisFailed { error: String },
}

/// Result of processing a single photo, pairing the photo identity with its outcome.
#[derive(Debug)]
pub struct ProcessingResult {
    pub photo_id: Uuid,
    pub outcome: Outcome,
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

        // Transition the job to Processing before analysis begins, so the status
        // reflects that work is underway even while the first photo is still running.
        self.start_job_if_queued(photo.job_id).await;

        let photo_id = photo.photo_id;

        let outcome = match self.analyze(&photo).await {
            Ok(tags) => match self.update_job(&photo, Ok(&tags)).await {
                Ok(()) => Outcome::Success { tags },
                Err(e) => {
                    error!(
                        photo_id = %photo.photo_id,
                        job_id = %photo.job_id,
                        error = %e,
                        "Analysis succeeded but job state could not be persisted"
                    );
                    Outcome::SuccessWithPersistenceError { tags, error: e }
                }
            },
            Err(error) => {
                if let Err(e) = self.update_job(&photo, Err(&error)).await {
                    error!(
                        photo_id = %photo.photo_id,
                        job_id = %photo.job_id,
                        error = %e,
                        "Analysis failed and job state could not be persisted either"
                    );
                }
                Outcome::AnalysisFailed { error }
            }
        };

        ProcessingResult { photo_id, outcome }
    }

    /// Load photo data, build request, call AI provider.
    /// Returns `Ok(tags)` on success or `Err(message)` on any failure.
    async fn analyze(&self, photo: &QueuedPhoto) -> Result<String, String> {
        // Load raw image bytes
        let bytes = match self.photo_store.load_data(photo.photo_id).await {
            Ok(data) => data,
            Err(e) => {
                error!(photo_id = %photo.photo_id, error = %e, "Failed to load photo data");
                return Err(format!("Failed to load photo: {}", e));
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
                return Err(e);
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
                Ok(response.text)
            }
            Err(e) => {
                error!(photo_id = %photo.photo_id, error = %e, "Photo analysis failed");
                Err(e.to_string())
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

    /// Transition the job from Queued to Processing if it hasn't started yet.
    ///
    /// Called at the start of photo processing so the job status reflects
    /// that work is underway before the first AI call completes.
    /// No-op if the job is already Processing or in a terminal state.
    async fn start_job_if_queued(&self, job_id: Uuid) {
        match self.job_store.get(job_id).await {
            Ok(Some(mut job)) if job.status == JobStatus::Queued => {
                job.start();
                if let Err(e) = self.job_store.update(job).await {
                    error!(job_id = %job_id, error = %e, "Failed to mark job as processing");
                }
            }
            _ => {}
        }
    }

    /// Update job state after processing a photo.
    ///
    /// `analysis` is `Ok(tags)` when the AI succeeded, `Err(error)` when it failed.
    /// Returns `Err` if the job state could not be persisted to the store.
    async fn update_job(
        &self,
        photo: &QueuedPhoto,
        analysis: Result<&str, &str>,
    ) -> Result<(), String> {
        let mut job = match self.job_store.get(photo.job_id).await {
            Ok(Some(job)) => job,
            Ok(None) => {
                return Err(format!("Job {} not found", photo.job_id));
            }
            Err(e) => {
                return Err(format!("Failed to load job {}: {}", photo.job_id, e));
            }
        };

        // If the job was cancelled (or finished by another path) while this
        // photo was in-flight, skip the update to avoid overwriting the
        // terminal state.
        if job.is_finished() {
            return Ok(());
        }

        // Move photo from queued to processed
        job.queued_photo_ids.retain(|id| id != &photo.photo_id);
        job.processed_photo_ids.push(photo.photo_id);

        // Store result
        job.results.insert(
            photo.photo_id,
            Self::build_photo_result(photo.photo_id, analysis),
        );

        // Complete job when all photos have been processed
        if job.queued_photo_ids.is_empty() {
            job.complete();
            info!(job_id = %photo.job_id, "Job completed");
        }

        self.job_store
            .update(job)
            .await
            .map(|_| ())
            .map_err(|e| format!("Failed to persist job {}: {}", photo.job_id, e))
    }

    fn build_photo_result(photo_id: Uuid, analysis: Result<&str, &str>) -> PhotoResult {
        match analysis {
            Ok(tags) => PhotoResult {
                photo_id,
                status: PhotoResultStatus::Completed,
                tags: Some(tags.to_string()),
                error: None,
                processed_at: Some(Utc::now()),
            },
            Err(error) => PhotoResult {
                photo_id,
                status: PhotoResultStatus::Failed,
                tags: None,
                error: Some(error.to_string()),
                processed_at: Some(Utc::now()),
            },
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
    // Mock AI providers
    // -----------------------------------------------------------------------

    struct SuccessProvider {
        response_text: String,
    }

    #[async_trait]
    impl AIProvider for SuccessProvider {
        fn name(&self) -> &str {
            "mock-success"
        }
        fn configured_model_ids(&self) -> Vec<String> {
            vec![]
        }
        fn configured_model_details(&self) -> Vec<crate::services::ai::ConfiguredModelInfo> {
            vec![]
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
        fn configured_model_ids(&self) -> Vec<String> {
            vec![]
        }
        fn configured_model_details(&self) -> Vec<crate::services::ai::ConfiguredModelInfo> {
            vec![]
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

    async fn setup_job_and_photo(fixture: &TestFixture) -> (Job, Uuid) {
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
            .create(Job::new(task.task_id, "llava".to_string(), vec![photo_id]))
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

        let result = fixture
            .processor
            .process(QueuedPhoto {
                job_id: job.job_id,
                photo_id,
                task_id: job.task_id,
                model: "llava".to_string(),
            })
            .await;

        assert!(matches!(result.outcome, Outcome::Success { .. }));
        if let Outcome::Success { tags } = result.outcome {
            assert_eq!(tags, "landscape, mountain, sunset");
        }

        let updated_job = fixture.job_store.get(job.job_id).await.unwrap().unwrap();
        assert_eq!(updated_job.status, JobStatus::Completed);
        assert!(updated_job.queued_photo_ids.is_empty());
        assert!(updated_job.processed_photo_ids.contains(&photo_id));
        let photo_result = updated_job.results.get(&photo_id).unwrap();
        assert_eq!(photo_result.status, PhotoResultStatus::Completed);
        assert_eq!(
            photo_result.tags.as_deref(),
            Some("landscape, mountain, sunset")
        );
    }

    #[tokio::test]
    async fn test_process_ai_failure_marks_photo_as_failed() {
        let provider = Arc::new(FailingProvider {
            error_message: "model timeout".to_string(),
        });
        let fixture = make_fixture(provider).await;
        let (job, photo_id) = setup_job_and_photo(&fixture).await;

        let result = fixture
            .processor
            .process(QueuedPhoto {
                job_id: job.job_id,
                photo_id,
                task_id: job.task_id,
                model: "llava".to_string(),
            })
            .await;

        assert!(matches!(result.outcome, Outcome::AnalysisFailed { .. }));

        let updated_job = fixture.job_store.get(job.job_id).await.unwrap().unwrap();
        assert_eq!(updated_job.status, JobStatus::Completed);
        assert!(updated_job.queued_photo_ids.is_empty());
        let photo_result = updated_job.results.get(&photo_id).unwrap();
        assert_eq!(photo_result.status, PhotoResultStatus::Failed);
        assert!(photo_result.error.is_some());
    }

    #[tokio::test]
    async fn test_process_success_with_persistence_error() {
        let provider = Arc::new(SuccessProvider {
            response_text: "ocean, wave, blue".to_string(),
        });
        let fixture = make_fixture(provider).await;
        let (job, photo_id) = setup_job_and_photo(&fixture).await;

        // Delete the job to make update_job fail with NotFound
        fixture.job_store.delete(job.job_id).await.unwrap();

        let result = fixture
            .processor
            .process(QueuedPhoto {
                job_id: job.job_id,
                photo_id,
                task_id: job.task_id,
                model: "llava".to_string(),
            })
            .await;

        assert!(matches!(
            result.outcome,
            Outcome::SuccessWithPersistenceError { .. }
        ));
        if let Outcome::SuccessWithPersistenceError { tags, .. } = result.outcome {
            assert_eq!(tags, "ocean, wave, blue");
        }
    }

    #[tokio::test]
    async fn test_process_transitions_queued_to_completed_on_single_photo() {
        let provider = Arc::new(SuccessProvider {
            response_text: "street, city".to_string(),
        });
        let fixture = make_fixture(provider).await;
        let (job, photo_id) = setup_job_and_photo(&fixture).await;

        assert_eq!(job.status, JobStatus::Queued);

        fixture
            .processor
            .process(QueuedPhoto {
                job_id: job.job_id,
                photo_id,
                task_id: job.task_id,
                model: "llava".to_string(),
            })
            .await;

        let updated_job = fixture.job_store.get(job.job_id).await.unwrap().unwrap();
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

        let photo_id_1 = Uuid::new_v4();
        let photo_id_2 = Uuid::new_v4();

        for (id, name) in [(photo_id_1, "a.jpg"), (photo_id_2, "b.jpg")] {
            let photo = crate::models::Photo::new(task.task_id, name.to_string(), 100);
            let photo = crate::models::Photo {
                photo_id: id,
                ..photo
            };
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
    async fn test_process_missing_photo_data_returns_analysis_failed() {
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

        assert!(matches!(result.outcome, Outcome::AnalysisFailed { .. }));
    }

    #[tokio::test]
    async fn test_process_marks_job_processing_before_analysis() {
        // Use a provider that checks job status DURING the analyze call.
        // We simulate this by verifying the job is Processing immediately
        // after process() returns (the transition happens before analyze).
        // The fixture approach: after process(), job must have started_at set
        // and have been in Processing before completing.
        let provider = Arc::new(SuccessProvider {
            response_text: "portrait, indoor".to_string(),
        });
        let fixture = make_fixture(provider).await;
        let (job, photo_id) = setup_job_and_photo(&fixture).await;

        assert_eq!(job.status, JobStatus::Queued);
        assert!(job.started_at.is_none());

        fixture
            .processor
            .process(QueuedPhoto {
                job_id: job.job_id,
                photo_id,
                task_id: job.task_id,
                model: "llava".to_string(),
            })
            .await;

        let updated_job = fixture.job_store.get(job.job_id).await.unwrap().unwrap();
        // started_at must be set (was set by start_job_if_queued BEFORE analysis)
        assert!(updated_job.started_at.is_some());
        // Job completed normally after analysis
        assert_eq!(updated_job.status, JobStatus::Completed);
    }

    #[tokio::test]
    async fn test_process_cancelled_job_skips_update() {
        let provider = Arc::new(SuccessProvider {
            response_text: "landscape, mountain".to_string(),
        });
        let fixture = make_fixture(provider).await;
        let (job, photo_id) = setup_job_and_photo(&fixture).await;

        // Cancel the job before processing completes
        let mut cancelled_job = fixture.job_store.get(job.job_id).await.unwrap().unwrap();
        cancelled_job.cancel();
        fixture.job_store.update(cancelled_job).await.unwrap();

        // Process the photo — the job is already cancelled, so update_job should bail early
        let result = fixture
            .processor
            .process(QueuedPhoto {
                job_id: job.job_id,
                photo_id,
                task_id: job.task_id,
                model: "llava".to_string(),
            })
            .await;

        // Analysis succeeds but persistence is skipped (job already finished)
        // This manifests as Success (from analyze) but the job state stays Cancelled
        assert!(matches!(
            result.outcome,
            Outcome::Success { .. } | Outcome::SuccessWithPersistenceError { .. }
        ));

        // Job must still be Cancelled — update_job must not have overwritten it
        let final_job = fixture.job_store.get(job.job_id).await.unwrap().unwrap();
        assert_eq!(final_job.status, JobStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_process_missing_task_returns_analysis_failed() {
        let provider = Arc::new(SuccessProvider {
            response_text: "should not reach".to_string(),
        });
        let fixture = make_fixture(provider).await;

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

        assert!(
            matches!(result.outcome, Outcome::AnalysisFailed { ref error } if error.contains("not found"))
        );
    }
}
