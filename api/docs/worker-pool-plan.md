# Implementation Plan: Worker Pool for Job Processing (Issue #8)

## Context

This plan implements a worker pool system for concurrent job execution with GPU resource management. The system will process AI analysis jobs created through the API, updating job status and progress as photos are analyzed.

**Why this change is needed:**

- Currently, jobs are created in "Queued" state but nothing processes them
- Need concurrent job execution to utilize multiple GPUs efficiently
- Need job status tracking and progress updates during processing
- Foundation for future SSE streaming (when Lightroom plugin is developed)

**Intended outcome:**

- Jobs automatically processed by worker pool after creation
- Configurable concurrency based on available GPUs
- Smart photo selection with hybrid threshold strategy for efficiency and fairness
- Job status updated in JobStore (allowing API polling for progress)
- Graceful startup and shutdown

## Todo List

- [x] Phase 1: Configuration — Add `WorkerPoolConfig` to `api/src/config/mod.rs`
- [x] Phase 2: Photo Queue — Create `api/src/services/worker/queue.rs` (trait + `HybridThresholdQueue`)
- [x] Phase 2b: PhotoBuffer Refactor — Refactor `queue.rs`: remove `PhotoQueue` trait, extract scheduling state into `Worker`, rename to `PhotoBuffer`
- [x] Phase 3: Photo Processor — Create `api/src/services/worker/processor.rs`
- [x] Phase 4: Worker Implementation — Create `api/src/services/worker/worker.rs` (includes scheduling state)
- [x] Phase 5: Worker Pool — Create `api/src/services/worker/pool.rs` (single shared buffer)
- [x] Phase 6: Module Organization — Create `api/src/services/worker/mod.rs`, update `api/src/services/mod.rs`
- [x] Phase 7: AppState Integration — Update `api/src/app_state.rs` and `api/src/startup.rs`
- [x] Phase 8: Model Extensions — Extend `api/src/models/job.rs` with `PhotoResult`

## Architecture Overview

### Design Choice: Approach 2 (Photo-Level with Smart Selection)

Following `api/docs/architecture.md` recommendations, we implement **photo-level processing** with **hybrid threshold strategy** rather than simple job-level processing.

**Why Approach 2:**

- Better temporal fairness between jobs
- Good balance of efficiency vs responsiveness
- Superior user experience (progress updates per photo)
- Adapts to photo complexity automatically

**Hybrid Threshold Strategy:**

- `min_photos_before_swap: 10` - Minimum photos before allowing model swap
- `max_time_before_swap: 120s` - Maximum time before forcing model swap
- Workers prefer same model until BOTH thresholds exceeded
- Provides temporal fairness AND overhead protection

### Core Components

```
┌──────────────────────────────────────────────────────┐
│                    AppState                          │
│  - Config                                            │
│  - TaskStore, PhotoStore, JobStore                   │
│  - AIProviderRegistry                                │
│  - WorkerPool ← NEW                                  │
└──────────────────────────────────────────────────────┘
                       │
                       ▼
┌──────────────────────────────────────────────────────┐
│                  WorkerPool                          │
│  - Configuration (devices, thresholds)               │
│  - PhotoBuffer (shared, single instance)             │
│  - Worker management (spawn/shutdown)                │
│  - Job discovery (poll JobStore for Queued jobs)     │
│  - Worker count = GPU count (1:1 mapping)            │
└──────────────────────────────────────────────────────┘
                       │
          ┌────────────┴────────────┐
          │  shared Arc<Mutex<>>   │
          ▼                         ▼
┌──────────────────┐     ┌──────────────────────────────┐
│   PhotoBuffer    │     │      Worker (Tokio task)     │
│  - photos_by_    │     │  - Assigned GPU device       │
│    model         │◄────│  - Scheduling state          │
│  - enqueue_job   │     │    (current_model, counters) │
│  - pop_by_model  │     │  - Calls pop_by_model /      │
│  - pop_any       │     │    pop_any based on state    │
└──────────────────┘     └──────────────────────────────┘
                                    │
                                    │ uses
                                    ▼
                         ┌──────────────────────────────┐
                         │       PhotoProcessor         │
                         │  - AI provider calls         │
                         │  - JobStore updates          │
                         │  - Error handling            │
                         │  - Result storage            │
                         └──────────────────────────────┘
```

**Separation of Concerns:**
- **PhotoBuffer**: Pure storage (photos grouped by model). No selection logic.
- **Worker**: Owns both orchestration and scheduling state (current model, counters, timer). Calls `pop_by_model` or `pop_any` based on its local state.
- **PhotoProcessor**: Processing logic (AI calls, storage updates).

**Why a single shared buffer (not per-worker queues):**
Load balancing comes naturally from the shared buffer — the fastest worker picks up the most work. With per-worker queues, a slow worker accumulates a backlog while fast workers are idle.

### Data Flow

```
API Handler                JobStore              WorkerPool
    │                         │                      │
    ├─ create job ───────────>│                      │
    │  (Queued)               │                      │
    │                         │<──── poll Queued ────┤
    │                         │                      │
    │                         │<──── update job ─────┤
    │                         │    (Processing)      │
    │                         │                      │
    │                         │<──── update job ─────┤
    │                         │    (progress)        │
    │                         │                      │
    │<──── get job ───────────┤                      │
    │  (polling)              │                      │
    │                         │                      │
    │                         │<──── update job ─────┤
    │                         │    (Completed)       │
```

## Implementation Plan

### Phase 1: Configuration

**File:** `api/src/config/mod.rs`

Add worker pool configuration for the dequeue strategy:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct WorkerPoolConfig {
    /// Minimum photos to process before allowing model swap
    #[serde(default = "default_min_photos_before_swap")]
    pub min_photos_before_swap: usize,

    /// Maximum time with same model before forcing swap (e.g., "120s", "2m")
    #[serde(default = "default_max_time_before_swap")]
    pub max_time_before_swap: String,
}

fn default_min_photos_before_swap() -> usize { 10 }
fn default_max_time_before_swap() -> String { "120s".to_string() }
```

Add to root Config:

```rust
pub struct Config {
    pub server: ServerConfig,
    pub storage: StorageConfig,
    pub upload: UploadConfig,
    pub ai: AIConfig,
    pub worker_pool: WorkerPoolConfig,  // ← NEW
}
```

**TOML example:**

```toml
[worker_pool]
min_photos_before_swap = 10
max_time_before_swap = "120s"
```

**Worker count determination:**
- Number of workers = number of GPUs in `ai.providers.ollama.devices`
- If `devices` is empty, defaults to 1 worker on device 0
- **No separate max_workers config** - one worker per GPU

**Validation:** Add validation in config loading:

- `min_photos_before_swap >= 1`
- `max_time_before_swap` parseable as Duration

### Phase 2: Photo Queue Trait and HybridThresholdQueue

**File:** `api/src/services/worker/queue.rs`

`PhotoQueue` is a trait that encapsulates both the storage and the selection logic. Each implementation chooses its internal data structure freely. The Worker only calls `enqueue_job` and `dequeue` — it knows nothing about how selection works.

```rust
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};
use uuid::Uuid;
use crate::models::job::Job;

/// Photo to be processed with its job context.
#[derive(Debug, Clone)]
pub struct QueuedPhoto {
    pub job_id: Uuid,
    pub photo_id: Uuid,
    pub task_id: Uuid,
    pub model: String,
}

/// Trait for a photo queue: encapsulates both storage and selection logic.
pub trait PhotoQueue: Send {
    /// Add all pending photos from a job to the queue.
    fn enqueue_job(&mut self, job: &Job);

    /// Dequeue the next photo according to the implementation's selection logic.
    fn dequeue(&mut self) -> Option<QueuedPhoto>;

    /// Returns true if the queue is empty.
    fn is_empty(&self) -> bool;

    /// Returns the number of photos in the queue.
    fn len(&self) -> usize;
}

/// Photo queue with hybrid threshold selection strategy.
///
/// Prefers the currently loaded model until BOTH thresholds are exceeded,
/// then prefers a different model for fairness, falling back to the current
/// model only if no other model has queued photos.
pub struct HybridThresholdQueue {
    // Internal storage: photos grouped by model for efficient lookup
    photos_by_model: HashMap<String, VecDeque<QueuedPhoto>>,
    total: usize,

    // Strategy state
    min_photos_before_swap: usize,
    max_time_before_swap: Duration,
    current_model: Option<String>,
    photos_processed_with_model: usize,
    model_loaded_at: Instant,
}

impl HybridThresholdQueue {
    pub fn new(min_photos_before_swap: usize, max_time_before_swap: Duration) -> Self {
        Self {
            photos_by_model: HashMap::new(),
            total: 0,
            min_photos_before_swap,
            max_time_before_swap,
            current_model: None,
            photos_processed_with_model: 0,
            model_loaded_at: Instant::now(),
        }
    }

    fn should_allow_model_swap(&self) -> bool {
        if self.current_model.is_none() {
            return true;
        }
        let photos_ok = self.photos_processed_with_model >= self.min_photos_before_swap;
        let time_ok = self.model_loaded_at.elapsed() >= self.max_time_before_swap;
        photos_ok && time_ok
    }

    fn on_photo_dequeued(&mut self, model: &str) {
        if self.current_model.as_deref() != Some(model) {
            self.current_model = Some(model.to_string());
            self.photos_processed_with_model = 0;
            self.model_loaded_at = Instant::now();
        } else {
            self.photos_processed_with_model += 1;
        }
    }

    fn pop_model(&mut self, model: &str) -> Option<QueuedPhoto> { /* pop from specific model */ }
    fn pop_any(&mut self) -> Option<QueuedPhoto> { /* pop from any non-empty queue */ }
    fn pop_other_model(&mut self, exclude: &str) -> Option<QueuedPhoto> { /* pop from any model except exclude */ }
}

impl PhotoQueue for HybridThresholdQueue {
    fn enqueue_job(&mut self, job: &Job) {
        for photo_id in &job.queued_photo_ids {
            let photo = QueuedPhoto {
                job_id: job.job_id,
                photo_id: *photo_id,
                task_id: job.task_id,
                model: job.model.clone(),
            };
            self.photos_by_model
                .entry(job.model.clone())
                .or_default()
                .push_back(photo);
            self.total += 1;
        }
    }

    fn dequeue(&mut self) -> Option<QueuedPhoto> {
        if self.total == 0 {
            return None;
        }

        let photo = match self.current_model.clone() {
            // No model loaded yet: take any photo
            None => self.pop_any(),
            // Thresholds met: prefer a different model for fairness
            Some(current) if self.should_allow_model_swap() => self
                .pop_other_model(&current)
                .or_else(|| self.pop_model(&current)),
            // Below thresholds: prefer current model for efficiency
            Some(current) => self
                .pop_model(&current)
                .or_else(|| self.pop_any()),
        };

        if let Some(ref p) = photo {
            self.total -= 1;
            self.on_photo_dequeued(&p.model);
        }

        photo
    }

    fn is_empty(&self) -> bool {
        self.total == 0
    }

    fn len(&self) -> usize {
        self.total
    }
}
```

### Phase 2b: PhotoBuffer Refactor

**File:** `api/src/services/worker/queue.rs`

Refactor the existing `queue.rs` to separate storage from scheduling:

- Remove the `PhotoQueue` trait (no longer needed — the Worker calls the buffer directly)
- Remove all scheduling state from `HybridThresholdQueue` (`current_model`, `photos_processed_with_model`, `model_loaded_at`, `min_photos_before_swap`, `max_time_before_swap`)
- Remove scheduling methods (`can_swap_model`, `on_photo_dequeued`, `dequeue`, `pop_other_model`)
- Rename `HybridThresholdQueue` → `PhotoBuffer`
- Expose `pop_by_model` and `pop_any` as public methods (previously private)
- Add `available_models()` for the Worker's scheduling logic

```rust
use std::collections::{HashMap, VecDeque};
use uuid::Uuid;
use crate::models::job::Job;

#[derive(Debug, Clone)]
pub struct QueuedPhoto {
    pub job_id: Uuid,
    pub photo_id: Uuid,
    pub task_id: Uuid,
    pub model: String,
}

/// Pure storage buffer for photos pending processing.
/// Photos are grouped by model for efficient lookup.
/// Scheduling logic (which model to pick next) lives in the Worker.
pub struct PhotoBuffer {
    photos_by_model: HashMap<String, VecDeque<QueuedPhoto>>,
    total: usize,
}

impl PhotoBuffer {
    pub fn new() -> Self {
        Self {
            photos_by_model: HashMap::new(),
            total: 0,
        }
    }

    /// Add all pending photos from a job to the buffer.
    pub fn enqueue_job(&mut self, job: &Job) {
        for &photo_id in &job.queued_photo_ids {
            match self.photos_by_model.get_mut(&job.model) {
                Some(queue) => {
                    queue.push_back(QueuedPhoto { job_id: job.job_id, photo_id, task_id: job.task_id, model: job.model.clone() });
                }
                None => {
                    let mut queue = VecDeque::new();
                    queue.push_back(QueuedPhoto { job_id: job.job_id, photo_id, task_id: job.task_id, model: job.model.clone() });
                    self.photos_by_model.insert(job.model.clone(), queue);
                }
            }
            self.total += 1;
        }
    }

    /// Pop the front photo from a specific model's queue.
    /// Returns `None` if no photos are queued for that model.
    pub fn pop_by_model(&mut self, model: &str) -> Option<QueuedPhoto> {
        let queue = self.photos_by_model.get_mut(model)?;
        let photo = queue.pop_front()?;
        self.total -= 1;
        Some(photo)
    }

    /// Pop the front photo from any non-empty model queue.
    pub fn pop_any(&mut self) -> Option<QueuedPhoto> {
        let queue = self.photos_by_model.values_mut().find(|q| !q.is_empty())?;
        let photo = queue.pop_front()?;
        self.total -= 1;
        Some(photo)
    }

    /// Returns the list of models that have at least one photo queued.
    pub fn available_models(&self) -> Vec<String> {
        self.photos_by_model
            .iter()
            .filter(|(_, q)| !q.is_empty())
            .map(|(model, _)| model.clone())
            .collect()
    }

    pub fn is_empty(&self) -> bool { self.total == 0 }
    pub fn len(&self) -> usize { self.total }
}
```

**Tests to update:**
- Remove tests for `PhotoQueue` trait object usage
- Remove scheduling tests (`test_prefers_current_model_*`, `test_allows_swap_*`, `test_counter_resets_*`) — these move to `worker.rs`
- Keep and update: `enqueue_job`, `pop_by_model`, `pop_any`, `is_empty`, `len`

### Phase 3: Photo Processor

**File:** `api/src/services/worker/processor.rs`

Separate photo processing logic:

```rust
use std::sync::Arc;
use uuid::Uuid;
use tracing::{info, error, debug};
use base64::{engine::general_purpose::STANDARD, Engine as _};

use crate::services::ai::{AIProvider, AnalyzeImageRequest};
use crate::storage::{JobStore, PhotoStore, TaskStore};
use crate::models::job::{JobStatus, PhotoResult, PhotoResultStatus};
use super::queue::QueuedPhoto;

const BASE_PROMPT: &str = "Analyze this image and generate keywords for a photo library. \
    Return a comma-separated list of relevant keywords covering: subject matter, \
    colors, mood, style, and any notable elements. \
    Return only the comma-separated keywords, no other text.";

/// Outcome of the processing pipeline for a single photo.
#[derive(Debug)]
pub enum Outcome {
    /// AI analysis succeeded and job state was persisted correctly.
    Success { tags: String },

    /// AI analysis succeeded but persisting the updated job state failed.
    /// The tags are returned so the caller can log them.
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
    ) -> Self { /* ... */ }

    /// Process a single photo: load data, call AI, update job state.
    pub async fn process(&self, photo: QueuedPhoto) -> ProcessingResult {
        let photo_id = photo.photo_id;
        let outcome = match self.analyze(&photo).await {
            Ok(tags) => {
                match self.update_job(&photo, Ok(&tags)).await {
                    Ok(()) => Outcome::Success { tags },
                    Err(e) => Outcome::SuccessWithPersistenceError { tags, error: e },
                }
            }
            Err(error) => {
                let _ = self.update_job(&photo, Err(&error)).await;
                Outcome::AnalysisFailed { error }
            }
        };
        ProcessingResult { photo_id, outcome }
    }

    /// Load photo bytes, load task context, call AI provider.
    /// Returns `Ok(tags)` on success or `Err(message)` on any failure.
    ///
    /// A missing task is treated as an error (not a silent fallback) because
    /// a job cannot outlive its parent task.
    async fn analyze(&self, photo: &QueuedPhoto) -> Result<String, String> {
        let bytes = self.photo_store.load_data(photo.photo_id).await
            .map_err(|e| format!("Failed to load photo: {}", e))?;

        let context = self.load_task_context(photo.task_id).await?;

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

        self.ai_provider.analyze_image(request).await
            .map(|r| r.text)
            .map_err(|e| e.to_string())
    }

    /// Loads the task context string, or returns `Err` if the task is not found.
    async fn load_task_context(&self, task_id: Uuid) -> Result<String, String> {
        match self.task_store.get(task_id).await {
            Ok(Some(task)) => Ok(task.context),
            Ok(None) => Err(format!("Task {} not found", task_id)),
            Err(e) => Err(format!("Failed to load task {}: {}", task_id, e)),
        }
    }

    /// Update job state after processing a photo.
    /// `analysis` is `Ok(tags)` on AI success, `Err(error)` on AI failure.
    /// Returns `Err` if the job state could not be persisted.
    async fn update_job(&self, photo: &QueuedPhoto, analysis: Result<&str, &str>) -> Result<(), String> {
        let mut job = self.job_store.get(photo.job_id).await
            .map_err(|e| format!("Failed to load job: {}", e))?
            .ok_or_else(|| format!("Job {} not found", photo.job_id))?;

        if job.status == JobStatus::Queued { job.start(); }

        job.queued_photo_ids.retain(|id| id != &photo.photo_id);
        job.processed_photo_ids.push(photo.photo_id);
        job.results.insert(photo.photo_id, Self::build_photo_result(photo.photo_id, analysis));

        if job.queued_photo_ids.is_empty() { job.complete(); }

        self.job_store.update(job).await
            .map(|_| ())
            .map_err(|e| format!("Failed to persist job: {}", e))
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
```

### Phase 4: Worker Implementation

**File:** `api/src/services/worker/worker.rs`

Worker owns the scheduling state and uses `PhotoBuffer`'s primitive pop methods to implement the hybrid threshold strategy locally.

```rust
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{info, debug};

use super::queue::PhotoBuffer;
use super::processor::PhotoProcessor;

pub struct Worker {
    /// Worker ID for logging
    id: usize,

    /// GPU device assigned to this worker
    gpu_device: u32,

    /// Shared photo buffer (storage only, no selection logic)
    buffer: Arc<Mutex<PhotoBuffer>>,

    /// Photo processor (encapsulates processing logic)
    processor: PhotoProcessor,

    // --- Scheduling state (local to this worker / GPU) ---

    /// Minimum photos to process before allowing a model swap.
    min_photos_before_swap: usize,

    /// Maximum time with the same model before forcing a swap.
    max_time_before_swap: Duration,

    /// The model currently loaded on this worker's GPU.
    current_model: Option<String>,

    /// Number of photos dequeued with the current model.
    photos_processed_with_model: usize,

    /// When the current model was last loaded.
    model_loaded_at: Instant,
}

impl Worker {
    pub fn new(
        id: usize,
        gpu_device: u32,
        buffer: Arc<Mutex<PhotoBuffer>>,
        processor: PhotoProcessor,
        min_photos_before_swap: usize,
        max_time_before_swap: Duration,
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

    /// Updates local scheduling state after picking a photo.
    fn on_photo_picked(&mut self, model: &str) {
        if self.current_model.as_deref() != Some(model) {
            self.current_model = Some(model.to_string());
            self.photos_processed_with_model = 1;
            self.model_loaded_at = Instant::now();
        } else {
            self.photos_processed_with_model += 1;
        }
    }

    /// Pick the next photo from the shared buffer using the hybrid threshold strategy.
    async fn next_photo(&mut self) -> Option<super::queue::QueuedPhoto> {
        let mut buf = self.buffer.lock().await;

        let photo = match self.current_model.clone() {
            // No model loaded yet: take any photo
            None => buf.pop_any(),
            // Both thresholds met: prefer a different model for fairness
            Some(ref current) if self.can_swap_model() => {
                let other = buf.available_models()
                    .into_iter()
                    .find(|m| m != current);
                match other {
                    Some(ref m) => buf.pop_by_model(m),
                    None => buf.pop_by_model(current),
                }
            }
            // Below thresholds: prefer current model for efficiency
            Some(ref current) => buf.pop_by_model(current).or_else(|| buf.pop_any()),
        };

        if let Some(ref p) = photo {
            self.on_photo_picked(&p.model);
        }

        photo
    }

    /// Main worker loop
    pub async fn run(mut self) {
        info!(worker_id = self.id, gpu_device = self.gpu_device, "Worker started");

        loop {
            match self.next_photo().await {
                Some(photo) => {
                    debug!(
                        worker_id = self.id,
                        photo_id = %photo.photo_id,
                        model = %photo.model,
                        "Dequeued photo"
                    );
                    self.processor.process(photo).await;
                }
                None => {
                    debug!(worker_id = self.id, "Buffer empty, waiting");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }
}
```

### Phase 5: Worker Pool Implementation

**File:** `api/src/services/worker/pool.rs`

Implement the worker pool manager:

```rust
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::config::Config;
use crate::services::ai::ProviderRegistry;
use crate::storage::{JobStore, PhotoStore, TaskStore};
use crate::models::job::JobStatus;
use super::queue::PhotoBuffer;
use super::worker::Worker;
use super::processor::PhotoProcessor;

pub struct WorkerPool {
    /// Worker task handles
    workers: Vec<JoinHandle<()>>,

    /// Single shared photo buffer (all workers pull from here)
    buffer: Arc<Mutex<PhotoBuffer>>,

    /// Job store for polling
    job_store: Arc<dyn JobStore>,

    /// Job discovery task handle
    discovery_task: Option<JoinHandle<()>>,
}

impl WorkerPool {
    pub fn new(
        config: &Config,
        job_store: Arc<dyn JobStore>,
        photo_store: Arc<dyn PhotoStore>,
        task_store: Arc<dyn TaskStore>,
        ai_providers: Arc<ProviderRegistry>,
    ) -> Self {
        // Get worker pool config
        let worker_config = &config.worker_pool;
        let min_photos = worker_config.min_photos_before_swap;
        let max_time = parse_duration(&worker_config.max_time_before_swap)
            .unwrap_or(Duration::from_secs(120));

        // Get Ollama config for GPU devices
        let ollama_config = config.ai.providers.get("ollama")
            .expect("Ollama provider not configured");

        let devices = &ollama_config.devices;

        // Worker count = GPU count (one worker per GPU)
        let gpu_assignments = if devices.is_empty() {
            vec![0] // Default: 1 worker on device 0
        } else {
            devices.clone() // One worker per configured GPU
        };

        info!(
            worker_count = gpu_assignments.len(),
            gpu_assignments = ?gpu_assignments,
            min_photos_before_swap = min_photos,
            max_time_before_swap_secs = max_time.as_secs(),
            "Initializing worker pool"
        );

        // Get default AI provider
        let ai_provider = ai_providers.default_provider()
            .expect("No default AI provider configured");

        // Single shared buffer — all workers pull from here for natural load balancing
        let buffer: Arc<Mutex<PhotoBuffer>> = Arc::new(Mutex::new(PhotoBuffer::new()));

        // Spawn workers — each owns its scheduling state, shares the buffer
        let mut workers = Vec::new();
        for (id, gpu_device) in gpu_assignments.iter().enumerate() {
            let processor = PhotoProcessor::new(
                ai_provider.clone(),
                job_store.clone(),
                photo_store.clone(),
                task_store.clone(),
            );

            let worker = Worker::new(
                id,
                *gpu_device,
                buffer.clone(),
                processor,
                min_photos,
                max_time,
            );

            let handle = tokio::spawn(async move {
                worker.run().await;
            });

            workers.push(handle);
        }

        Self {
            workers,
            buffer,
            job_store,
            discovery_task: None,
        }
    }

    /// Start the worker pool (recovers stale jobs, then begins job discovery)
    pub async fn start(&mut self) {
        // First, recover any stale jobs from previous shutdown/crash
        Self::recover_stale_jobs(self.job_store.clone()).await;

        // Then start job discovery loop
        let buffer = self.buffer.clone();
        let job_store = self.job_store.clone();

        let handle = tokio::spawn(async move {
            Self::job_discovery_loop(buffer, job_store).await;
        });

        self.discovery_task = Some(handle);
        info!("Worker pool started");
    }

    /// Recover jobs that were in "Processing" state from previous run
    /// These are stale (server shutdown/crash) and should be reset to "Queued"
    async fn recover_stale_jobs(job_store: Arc<dyn JobStore>) {
        info!("Checking for stale jobs to recover");

        match job_store.list().await {
            Ok(jobs) => {
                let mut recovered_count = 0;

                for mut job in jobs {
                    if job.status == JobStatus::Processing {
                        info!(
                            job_id = %job.job_id,
                            queued = job.queued_photo_ids.len(),
                            processed = job.processed_photo_ids.len(),
                            "Recovering stale job from previous run"
                        );

                        // Check if there are photos left to process
                        if !job.queued_photo_ids.is_empty() {
                            // Reset to Queued - will be picked up by discovery loop
                            job.status = JobStatus::Queued;
                            job.started_at = None;

                            if let Err(e) = job_store.update(&job).await {
                                error!(job_id = %job.job_id, error = %e, "Failed to recover job");
                            } else {
                                recovered_count += 1;
                            }
                        } else {
                            // All photos were processed, mark as completed
                            job.complete();

                            if let Err(e) = job_store.update(&job).await {
                                error!(job_id = %job.job_id, error = %e, "Failed to complete recovered job");
                            } else {
                                info!(job_id = %job.job_id, "Recovered job marked as completed");
                                recovered_count += 1;
                            }
                        }
                    }
                }

                if recovered_count > 0 {
                    info!(count = recovered_count, "Recovered stale jobs");
                } else {
                    info!("No stale jobs found");
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to list jobs for recovery");
            }
        }
    }

    /// Job discovery loop: polls JobStore for Queued jobs and enqueues
    /// their photos into the shared buffer. Workers pull from the buffer
    /// at their own pace — no round-robin needed.
    async fn job_discovery_loop(
        buffer: Arc<Mutex<PhotoBuffer>>,
        job_store: Arc<dyn JobStore>,
    ) {
        loop {
            match job_store.list().await {
                Ok(jobs) => {
                    for job in jobs {
                        if job.status == JobStatus::Queued && !job.queued_photo_ids.is_empty() {
                            let mut buf = buffer.lock().await;
                            buf.enqueue_job(&job);

                            info!(
                                job_id = %job.job_id,
                                photo_count = job.queued_photo_ids.len(),
                                model = %job.model,
                                "Enqueued job photos into shared buffer"
                            );
                        }
                    }
                }
                Err(e) => {
                    warn!(error = %e, "Failed to list jobs for discovery");
                }
            }

            // Poll every 5 seconds
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }

    /// Shutdown: abort workers immediately
    /// Jobs in "Processing" state will be recovered at next startup
    pub async fn shutdown(self) {
        info!("Shutting down worker pool");

        // Stop discovery task
        if let Some(task) = self.discovery_task {
            task.abort();
        }

        // Abort workers immediately
        // Note: Current photo will be lost (acceptable - will be reprocessed)
        // Job state is already saved after each completed photo
        for worker in self.workers {
            worker.abort();
        }

        info!("Worker pool shut down");
    }
}

fn parse_duration(s: &str) -> Option<Duration> {
    // Simple parser for "60s", "2m", etc.
    let s = s.trim();

    if let Some(secs) = s.strip_suffix('s') {
        secs.parse::<u64>().ok().map(Duration::from_secs)
    } else if let Some(mins) = s.strip_suffix('m') {
        mins.parse::<u64>().ok().map(|m| Duration::from_secs(m * 60))
    } else {
        None
    }
}
```

### Phase 6: Module Organization

**File:** `api/src/services/worker/mod.rs`

Module exports:

```rust
mod queue;
mod processor;
mod worker;
mod pool;

pub use pool::WorkerPool;
pub use queue::{PhotoBuffer, QueuedPhoto};
pub use processor::{PhotoProcessor, ProcessingResult};
```

**File:** `api/src/services/mod.rs`

Add worker module:

```rust
pub mod ai;
pub mod worker;  // ← NEW
```

### Phase 7: AppState Integration

**File:** `api/src/app_state.rs` (or wherever AppState is defined)

Add WorkerPool to AppState:

```rust
use crate::services::worker::WorkerPool;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub task_store: Arc<dyn TaskStore>,
    pub photo_store: Arc<dyn PhotoStore>,
    pub job_store: Arc<dyn JobStore>,
    pub ai_providers: Arc<ProviderRegistry>,
    pub worker_pool: Arc<Mutex<WorkerPool>>,  // ← NEW (Mutex for start/shutdown)
}
```

**File:** `api/src/startup.rs`

Initialize worker pool:

```rust
pub async fn init_app_state(config: Config) -> Result<AppState, String> {
    // ... existing store initialization ...

    // Initialize worker pool
    let worker_pool = WorkerPool::new(
        &config,
        job_store.clone(),
        photo_store.clone(),
        task_store.clone(),
        ai_providers.clone(),
    );

    let worker_pool = Arc::new(Mutex::new(worker_pool));

    // Start worker pool (recovers stale jobs first)
    {
        let mut pool = worker_pool.lock().await;
        pool.start().await;  // Now async
    }

    Ok(AppState::new(
        config,
        task_store,
        photo_store,
        job_store,
        ai_providers,
        worker_pool,
    ))
}
```

### Phase 8: Model Extensions

**File:** `api/src/models/job.rs`

Extend Job model to store results:

```rust
// Add to Job struct:
pub struct Job {
    // ... existing fields ...

    /// Results per photo (photo_id -> PhotoResult)
    pub results: HashMap<Uuid, PhotoResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhotoResult {
    pub photo_id: Uuid,
    pub status: PhotoResultStatus,
    pub tags: Option<String>,   // comma-separated keywords, None on failure
    pub error: Option<String>,
    pub processed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PhotoResultStatus {
    Completed,
    Failed,
}
```

Update worker to populate results.

## Critical Files

### Files to Create:

- `api/src/services/worker/mod.rs`
- `api/src/services/worker/queue.rs` — `PhotoBuffer` (pure storage) + `QueuedPhoto`
- `api/src/services/worker/processor.rs` — photo processing logic
- `api/src/services/worker/worker.rs` — dequeue loop orchestrator
- `api/src/services/worker/pool.rs` — worker pool manager

### Files to Modify:

- `api/src/config/mod.rs` - Add WorkerPoolConfig
- `api/src/services/mod.rs` - Export worker module
- `api/src/app_state.rs` - Add worker_pool field
- `api/src/startup.rs` - Initialize and start worker pool
- `api/src/models/job.rs` - Add results field and PhotoResult type
- `api/Cargo.toml` - Ensure dependencies (tokio, dashmap)

## Testing Strategy

### Unit Tests

**File:** `api/src/services/worker/queue.rs`

Test `PhotoBuffer` (pure storage):

- `enqueue_job` adds all photos grouped by model
- `pop_by_model` returns photos for the given model, `None` if empty
- `pop_any` returns a photo from any non-empty model queue
- `available_models` reflects current buffer state
- `is_empty` / `len` reflect buffer state correctly

**File:** `api/src/services/worker/worker.rs`

Test Worker scheduling logic (using a mock/real `PhotoBuffer`):

- Prefers current model when below thresholds
- Falls back to `pop_any` when current model queue is empty
- Allows model swap when both thresholds exceeded
- Prefers a different model when swap is allowed and another model has photos
- Counter and timer reset on model change
- Handles empty buffer (wait loop)

**File:** `api/src/services/worker/processor.rs`

Test PhotoProcessor (with mocked AI provider):

- `Outcome::Success`: AI succeeds, job persisted → job moves to Completed
- `Outcome::AnalysisFailed`: AI fails → photo marked as failed in job
- `Outcome::SuccessWithPersistenceError`: AI succeeds but job deleted before update → tags returned, error reported
- Job status transitions: Queued → Processing (first photo) → Completed (last photo)
- Multi-photo job: not completed until last photo processed
- Missing photo binary data → `Outcome::AnalysisFailed`
- Missing task → `Outcome::AnalysisFailed` (task not found is a logic error, not a fallback)

### Integration Tests

**File:** `api/tests/worker_integration_test.rs`

End-to-end test:

1. Create task with photos
2. Create job
3. Wait for worker pool to process
4. Verify job status transitions (Queued → Processing → Completed)
5. Verify all photos processed
6. Verify results stored

Use `tokio::time::timeout` to avoid hanging tests.

### Manual Testing

1. Start server: `cargo run --release`
2. Create task: `POST /api/tasks`
3. Upload photos: `POST /api/tasks/{task_id}/photos`
4. Create job: `POST /api/tasks/{task_id}/jobs`
5. Poll job status: `GET /api/jobs/{job_id}` (requires Issue #9)
6. Verify completion and results

## Verification Steps

### 1. Configuration Validation

```bash
# Check config loads correctly
cargo run -- --config config.toml

# Logs should show:
# - Worker pool initialized with N workers
# - GPU assignments
# - Threshold values
```

### 2. Job Processing

```bash
# Create and process a job
curl -X POST http://localhost:8080/api/tasks \
  -H "Content-Type: application/json" \
  -d '{"context":"test photos"}'

# Upload photos (multipart form)
curl -X POST http://localhost:8080/api/tasks/{task_id}/photos \
  -F "photos=@photo1.jpg" \
  -F "photos=@photo2.jpg"

# Create job
curl -X POST http://localhost:8080/api/tasks/{task_id}/jobs \
  -H "Content-Type: application/json" \
  -d '{"model":"qwen2-vl"}'

# Poll for completion (requires Issue #9)
curl http://localhost:8080/api/jobs/{job_id}
```

### 3. Logs Verification

Monitor logs for:

- `Checking for stale jobs to recover` at startup
- `Recovered stale jobs` if any jobs were recovered from previous run
- `Worker started` messages (one per worker)
- `Queued job photos` when jobs discovered
- `Processing photo` during execution
- `Photo analysis completed` on success
- `Model swap` when switching models
- `Job completed` when finished

### 4. Database Verification

Check JobStore for:

- Job status transitions (Queued → Processing → Completed)
- `queued_photo_ids` decreasing
- `processed_photo_ids` increasing
- Results populated

### 5. Shutdown and Recovery

**Test shutdown:**
```bash
# Create job with multiple photos
curl -X POST http://localhost:8080/api/tasks/{task_id}/jobs \
  -H "Content-Type: application/json" \
  -d '{"model":"qwen2-vl"}'

# Let it start processing (check logs for "Processing photo")

# Send SIGTERM to server
kill -TERM <pid>

# Logs should show:
# - "Shutting down worker pool"
# - "Worker pool shut down"
```

**Test recovery:**
```bash
# Restart server
cargo run --release

# Logs should show:
# - "Checking for stale jobs to recover"
# - "Recovering stale job from previous run" (if any)
# - "Recovered stale jobs" with count
# - Job should resume processing
```

**Verify job state:**
```bash
# Check job status via API (requires Issue #9)
curl http://localhost:8080/api/jobs/{job_id}

# Should show:
# - Status transitions: Processing → (shutdown) → Queued → Processing → Completed
# - processed_photo_ids should retain completed photos
# - queued_photo_ids should contain remaining photos
```

## Implementation Notes

### Job Discovery Design Choice

The implementation uses **polling-based job discovery** (checking JobStore every 5 seconds) rather than channel-based notification.

**Why polling:**

- Simple implementation
- No coupling between API handlers and worker pool
- Works with filesystem storage (no pub/sub)
- 5-second delay is acceptable for current use case

**Future enhancement:**

- When migrating to database/Redis, can use pub/sub notifications
- Handlers send notification to worker pool on job creation
- Lower latency (immediate processing)

### GPU Device Assignment

**Current strategy:** 1:1 mapping (one worker per GPU)

Workers are assigned to GPUs based on `ai.providers.ollama.devices`:
- If `devices = [0, 1]` → 2 workers (worker 0 on GPU 0, worker 1 on GPU 1)
- If `devices = []` → 1 worker on GPU 0 (default)

**Rationale:**
- Each GPU processes one model at a time (serial inference)
- More workers than GPUs adds overhead without benefit
- Simple, predictable configuration

**Future enhancements:**
- Load-based assignment (monitor GPU utilization)
- Multi-GPU model parallelism (requires Ollama support)

### Error Handling

- Photo processing failures: Mark photo as failed, continue with remaining photos
- Job store errors: Log and retry
- AI provider errors: Log, mark photo as failed
- Worker panics: Tokio spawns are isolated, other workers continue

### Graceful Shutdown

**Design Decision: Immediate Abort (Not Graceful)**

The implementation aborts workers immediately on shutdown rather than waiting for current photos to complete.

**Rationale:**
- Simplicity: No need for shutdown signal channels or timeout logic
- Acceptable loss: Only the current photo being processed is lost (~3-10 seconds of GPU work)
- Recovery: Photo will be reprocessed on next startup
- State safety: Job state is already saved to disk after each photo completes

**How it works:**
1. Shutdown aborts workers immediately
2. Job remains in "Processing" state on disk with:
   - `processed_photo_ids`: All completed photos ✅
   - `queued_photo_ids`: Remaining photos + current photo (lost) ✅
3. On startup: `recover_stale_jobs()` resets job to "Queued"
4. Discovery loop picks up job and continues from where it left off

**Trade-off:**
- 🟢 Simple implementation (no coordination needed)
- 🟢 Fast shutdown
- 🔴 One photo re-processed after restart (~5 sec overhead)

**Alternative (not implemented):**
Graceful shutdown with timeout would add complexity (signal channels, coordination) for minimal benefit (saving one photo's work).

## Dependencies

Ensure these are in `Cargo.toml`:

- `tokio` (with full features for channels, time)
- `dashmap` (for concurrent HashMap if needed)
- `tracing` (for logging)

All should already be present based on exploration.

## Success Criteria

- ✅ Jobs automatically processed after creation
- ✅ Configurable worker count and GPU assignments
- ✅ Hybrid threshold strategy implemented
- ✅ Job status updated in JobStore
- ✅ Results stored per photo
- ✅ Error handling for failed photos
- ✅ Graceful startup and shutdown
- ✅ Unit tests for queue and worker logic
- ✅ Integration test for end-to-end processing
- ✅ Manual testing with Postman

## Future Enhancements (Out of Scope)

- SSE streaming (with Lightroom plugin - separate issue)
- Job cancellation (cancel running jobs)
- Job retry (retry failed photos)
- Priority queues (high-priority jobs first)
- Worker pool scaling (dynamic worker count)
- Advanced GPU management (load balancing, affinity)
- Metrics and monitoring (Prometheus, Grafana)
