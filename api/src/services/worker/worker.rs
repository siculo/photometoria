// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::processor::{Outcome, PhotoProcessor};
use super::queue::{PhotoBuffer, QueuedPhoto};
use crate::services::worker::ProcessingResult;
use tokio::sync::Mutex;
use tracing::{debug, error, info};

/// Worker executes photo processing with a hybrid threshold scheduling strategy.
///
/// Each Worker owns its scheduling state (current model, counters, timer) and
/// uses the shared `PhotoBuffer`'s primitive pop methods to implement fairness.
pub struct Worker {
    id: usize,
    gpu_device: u32,
    buffer: Arc<Mutex<PhotoBuffer>>,
    processor: PhotoProcessor,

    // --- Scheduling state ---
    min_photos_before_swap: usize,
    max_time_before_swap: Duration,
    current_model: Option<String>,
    photos_processed_with_model: usize,
    model_loaded_at: Instant,

    /// How long to sleep when the buffer is empty.
    idle_sleep: Duration,
}

impl Worker {
    pub fn new(
        id: usize,
        gpu_device: u32,
        buffer: Arc<Mutex<PhotoBuffer>>,
        processor: PhotoProcessor,
        min_photos_before_swap: usize,
        max_time_before_swap: Duration,
        idle_sleep: Duration,
    ) -> Self {
        Self {
            id,
            gpu_device,
            buffer,
            processor,
            min_photos_before_swap,
            max_time_before_swap,
            current_model: None,
            photos_processed_with_model: 0,
            model_loaded_at: Instant::now(),
            idle_sleep,
        }
    }

    /// Returns true if both thresholds are exceeded and a model swap is allowed.
    fn can_swap_model(&self) -> bool {
        match self.current_model {
            None => true,
            Some(_) => {
                let photos_ok = self.photos_processed_with_model >= self.min_photos_before_swap;
                let time_ok = self.model_loaded_at.elapsed() >= self.max_time_before_swap;
                photos_ok && time_ok
            }
        }
    }

    /// Updates scheduling state after picking a photo.
    fn on_photo_picked(&mut self, model: &str) {
        if self.current_model.as_deref() != Some(model) {
            debug!(
                worker_id = self.id,
                previous_model = ?self.current_model,
                new_model = model,
                "Model swap"
            );
            self.current_model = Some(model.to_string());
            self.photos_processed_with_model = 1;
            self.model_loaded_at = Instant::now();
        } else {
            self.photos_processed_with_model += 1;
        }
    }

    /// Pick the next photo from the shared buffer using the hybrid threshold strategy.
    async fn next_photo(&mut self) -> Option<QueuedPhoto> {
        // Scope the lock so it's released before calling `on_photo_picked` (&mut self).
        let photo = {
            let mut buf = self.buffer.lock().await;

            match self.current_model.clone() {
                // No model loaded yet: take any photo
                None => buf.pop_any(),
                // Both thresholds met: prefer a different model for fairness (round-robin)
                Some(ref current) if self.can_swap_model() => buf
                    .pop_excluding(current)
                    .or_else(|| buf.pop_by_model(current)),
                // Below thresholds: prefer current model for efficiency
                Some(ref current) => buf.pop_by_model(current).or_else(|| buf.pop_any()),
            }
            // `buf` (MutexGuard) is dropped here
        };

        if let Some(ref p) = photo {
            self.on_photo_picked(&p.model);
        }

        photo
    }

    /// Main worker loop: dequeues and processes photos indefinitely.
    pub async fn run(mut self) {
        info!(
            worker_id = self.id,
            gpu_device = self.gpu_device,
            "Worker started"
        );

        loop {
            match self.next_photo().await {
                Some(photo) => {
                    debug!(
                        worker_id = self.id,
                        photo_id = %photo.photo_id,
                        model = %photo.model,
                        "Dequeued photo"
                    );
                    let processing_result = self.processor.process(photo).await;
                    self.log_processing_result(processing_result);
                }
                None => {
                    debug!(worker_id = self.id, "Buffer empty, waiting");
                    tokio::time::sleep(self.idle_sleep).await;
                }
            }
        }
    }

    fn log_processing_result(&self, processing_result: ProcessingResult) {
        match processing_result.outcome {
            Outcome::Success { .. } => {
                info!(
                    worker_id = self.id,
                    photo_id = %processing_result.photo_id,
                    "Photo analysis completed successfully",
                );
            }
            Outcome::SuccessWithPersistenceError { error, .. } => {
                error!(
                    worker_id = self.id,
                    photo_id = %processing_result.photo_id,
                    error = %error,
                    "Photo analysis succeeded but job state could not be persisted",
                );
            }
            Outcome::AnalysisFailed { error } => {
                error!(
                    worker_id = self.id,
                    photo_id = %processing_result.photo_id,
                    error = %error,
                    "Photo analysis failed",
                );
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
    use tokio::sync::Mutex;
    use uuid::Uuid;

    use crate::models::Job;
    use crate::services::ai::{
        AIProvider, AIProviderError, AIProviderResult, AnalyzeImageRequest, AnalyzeImageResponse,
        HealthStatus, ModelInfo,
    };
    use crate::services::worker::queue::PhotoBuffer;
    use crate::storage::{FileSystemJobStore, FileSystemPhotoStore, FileSystemTaskStore};

    // Minimal mock — never called in scheduling tests (only next_photo() is invoked).
    struct NoopProvider;

    #[async_trait]
    impl AIProvider for NoopProvider {
        fn name(&self) -> &str {
            "noop"
        }
        fn configured_model_ids(&self) -> Vec<String> {
            vec![]
        }
        fn configured_model_details(&self) -> Vec<crate::services::ai::ConfiguredModelInfo> {
            vec![]
        }
        async fn check_health(&self) -> AIProviderResult<HealthStatus> {
            Err(AIProviderError::Unavailable {
                provider: "noop".into(),
                message: "noop".into(),
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
                provider: "noop".into(),
                message: "noop".into(),
            })
        }
    }

    // Creates a Worker for scheduling tests.
    // The processor is never called since we only invoke `next_photo()`.
    async fn make_worker(
        buffer: Arc<Mutex<PhotoBuffer>>,
        min_photos: usize,
        max_time: Duration,
    ) -> (Worker, TempDir) {
        let temp = TempDir::new().unwrap();
        let path = temp.path().to_path_buf();

        let task_store = Arc::new(FileSystemTaskStore::new(path.clone()).await);
        let photo_store =
            Arc::new(FileSystemPhotoStore::new(path.clone(), task_store.clone()).await);
        let job_store = Arc::new(FileSystemJobStore::new(path, task_store.clone()).await);
        let ai_provider: Arc<dyn AIProvider> = Arc::new(NoopProvider);

        let processor = PhotoProcessor::new(ai_provider, job_store, photo_store, task_store);
        let worker = Worker::new(
            0,
            0,
            buffer,
            processor,
            min_photos,
            max_time,
            Duration::from_millis(10),
        );
        (worker, temp)
    }

    fn buffer_with_two_models(count_a: usize, count_b: usize) -> Arc<Mutex<PhotoBuffer>> {
        let mut buf = PhotoBuffer::new();
        if count_a > 0 {
            let job = Job::new(
                Uuid::new_v4(),
                "modelA".to_string(),
                (0..count_a).map(|_| Uuid::new_v4()).collect(),
            );
            buf.enqueue_job(&job);
        }
        if count_b > 0 {
            let job = Job::new(
                Uuid::new_v4(),
                "modelB".to_string(),
                (0..count_b).map(|_| Uuid::new_v4()).collect(),
            );
            buf.enqueue_job(&job);
        }
        Arc::new(Mutex::new(buf))
    }

    #[tokio::test]
    async fn test_prefers_current_model_below_thresholds() {
        // High thresholds: both will never be exceeded during this test
        let buf = buffer_with_two_models(5, 5);
        let (mut worker, _temp) = make_worker(buf, 10, Duration::from_secs(3600)).await;

        // First pick: any model (sets current_model)
        let first = worker.next_photo().await.unwrap();
        let first_model = first.model.clone();

        // Next picks: should prefer the same model (below thresholds)
        for _ in 0..3 {
            let photo = worker.next_photo().await.unwrap();
            assert_eq!(
                photo.model, first_model,
                "Should prefer current model when below thresholds"
            );
        }
    }

    #[tokio::test]
    async fn test_swaps_model_after_both_thresholds_exceeded() {
        // Low thresholds: 1 photo + 1ns (effectively immediate)
        let buf = buffer_with_two_models(5, 5);
        let (mut worker, _temp) = make_worker(buf, 1, Duration::from_nanos(1)).await;

        // First pick: sets current_model
        let first = worker.next_photo().await.unwrap();
        let first_model = first.model.clone();

        // Ensure time threshold is exceeded
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Next pick should prefer a DIFFERENT model for fairness
        let second = worker.next_photo().await.unwrap();
        assert_ne!(
            second.model, first_model,
            "Should swap model after both thresholds exceeded"
        );
    }

    #[tokio::test]
    async fn test_falls_back_to_any_when_current_model_queue_empty() {
        // modelA has 1 photo, modelB has 3; thresholds never exceeded
        let buf = buffer_with_two_models(1, 3);
        let (mut worker, _temp) = make_worker(buf, 10, Duration::from_secs(3600)).await;

        // Pick all photos; we must be able to get all 4
        let mut picked = 0;
        while let Some(_) = worker.next_photo().await {
            picked += 1;
        }
        assert_eq!(picked, 4, "Should pick all photos across both models");
    }

    #[tokio::test]
    async fn test_counter_resets_on_model_change() {
        let buf = buffer_with_two_models(5, 5);
        let (mut worker, _temp) = make_worker(buf, 1, Duration::from_nanos(1)).await;

        // First pick
        let _ = worker.next_photo().await.unwrap();
        assert_eq!(worker.photos_processed_with_model, 1);
        let model_before = worker.current_model.clone().unwrap();

        // Exceed time threshold then pick again (forces swap)
        tokio::time::sleep(Duration::from_millis(10)).await;
        let _ = worker.next_photo().await.unwrap();

        let model_after = worker.current_model.clone().unwrap();
        assert_ne!(model_before, model_after, "Model should have swapped");
        assert_eq!(
            worker.photos_processed_with_model, 1,
            "Counter should reset to 1 on model change"
        );
    }

    #[tokio::test]
    async fn test_stays_on_current_model_when_no_other_available() {
        // Only one model available
        let mut buf = PhotoBuffer::new();
        let job = Job::new(
            Uuid::new_v4(),
            "solo".to_string(),
            (0..3).map(|_| Uuid::new_v4()).collect(),
        );
        buf.enqueue_job(&job);
        let buf = Arc::new(Mutex::new(buf));

        // Thresholds already exceeded
        let (mut worker, _temp) = make_worker(buf, 1, Duration::from_nanos(1)).await;
        tokio::time::sleep(Duration::from_millis(10)).await;

        let first = worker.next_photo().await.unwrap();
        assert_eq!(first.model, "solo");

        // Even though swap is allowed, no other model exists: stays on "solo"
        tokio::time::sleep(Duration::from_millis(10)).await;
        let second = worker.next_photo().await.unwrap();
        assert_eq!(second.model, "solo");
    }
}
