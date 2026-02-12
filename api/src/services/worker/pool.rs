use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::config::worker_pool::parse_duration;
use crate::config::{Config, ProviderConfig};
use crate::models::job::JobStatus;
use crate::services::ai::ProviderRegistry;
use crate::storage::{JobStore, PhotoStore, TaskStore};

use super::processor::PhotoProcessor;
use super::queue::PhotoBuffer;
use super::worker::Worker;

pub struct WorkerPool {
    /// Worker task handles (one per GPU)
    workers: Vec<JoinHandle<()>>,

    /// Single shared photo buffer — all workers pull from here
    buffer: Arc<Mutex<PhotoBuffer>>,

    /// Job store for polling
    job_store: Arc<dyn JobStore>,

    /// How often the discovery loop polls for new queued jobs.
    discovery_poll_interval: Duration,

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
        let worker_config = &config.worker_pool;
        let min_photos = worker_config.min_photos_before_swap;
        let max_time =
            parse_duration(&worker_config.max_time_before_swap).unwrap_or(Duration::from_secs(120));
        let idle_sleep =
            parse_duration(&worker_config.worker_idle_sleep).unwrap_or(Duration::from_millis(500));
        let discovery_poll_interval = parse_duration(&worker_config.discovery_poll_interval)
            .unwrap_or(Duration::from_secs(5));

        let gpu_assignments = Self::gpu_assignment_from_providers_config(&config.ai.providers);

        info!(
            worker_count = gpu_assignments.len(),
            gpu_assignments = ?gpu_assignments,
            min_photos_before_swap = min_photos,
            max_time_before_swap_secs = max_time.as_secs(),
            worker_idle_sleep_ms = idle_sleep.as_millis(),
            discovery_poll_interval_secs = discovery_poll_interval.as_secs(),
            "Initializing worker pool"
        );

        let ai_provider = ai_providers
            .default_provider()
            .expect("No default AI provider configured");

        // Single shared buffer — natural load balancing across workers
        let buffer: Arc<Mutex<PhotoBuffer>> = Arc::new(Mutex::new(PhotoBuffer::new()));

        // Spawn one worker per GPU
        let mut workers = Vec::new();
        for (id, &gpu_device) in gpu_assignments.iter().enumerate() {
            let processor = PhotoProcessor::new(
                ai_provider.clone(),
                job_store.clone(),
                photo_store.clone(),
                task_store.clone(),
            );

            let worker = Worker::new(
                id,
                gpu_device,
                buffer.clone(),
                processor,
                min_photos,
                max_time,
                idle_sleep,
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
            discovery_poll_interval,
            discovery_task: None,
        }
    }

    /// Determine GPU assignments from Ollama providers config
    fn gpu_assignment_from_providers_config(poc: &HashMap<String, ProviderConfig>) -> Vec<u32> {
        poc.get("ollama")
            .and_then(|p| match p {
                ProviderConfig::Ollama(c) => Some(c.devices.clone()),
            })
            .filter(|d| !d.is_empty())
            .unwrap_or_else(|| vec![0])
    }

    /// Creates a pool with no workers and no discovery task.
    /// For use in tests where job processing should not run.
    #[cfg(test)]
    pub fn new_inactive(job_store: Arc<dyn JobStore>) -> Self {
        Self {
            workers: Vec::new(),
            buffer: Arc::new(Mutex::new(PhotoBuffer::new())),
            job_store,
            discovery_poll_interval: Duration::from_secs(5),
            discovery_task: None,
        }
    }

    /// Start the worker pool: recover stale jobs, then begin job discovery.
    pub async fn start(&mut self) {
        Self::recover_stale_jobs(self.job_store.clone()).await;

        let buffer = self.buffer.clone();
        let job_store = self.job_store.clone();
        let poll_interval = self.discovery_poll_interval;

        let handle = tokio::spawn(async move {
            Self::job_discovery_loop(buffer, job_store, poll_interval).await;
        });

        self.discovery_task = Some(handle);
        info!("Worker pool started");
    }

    /// Reset jobs that were in `Processing` state from a previous run.
    ///
    /// These are stale (server crashed/shutdown mid-processing). Jobs with
    /// remaining photos are reset to `Queued`; jobs with all photos processed
    /// are marked `Completed`.
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

                        if !job.queued_photo_ids.is_empty() {
                            job.status = JobStatus::Queued;
                            job.started_at = None;

                            if let Err(e) = job_store.update(job).await {
                                error!(error = %e, "Failed to recover stale job");
                            } else {
                                recovered_count += 1;
                            }
                        } else {
                            // All photos were already processed; just mark completed
                            job.complete();

                            if let Err(e) = job_store.update(job).await {
                                error!(error = %e, "Failed to complete recovered job");
                            } else {
                                info!("Recovered job marked as completed");
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

    /// Polls the JobStore every 5 seconds for `Queued` jobs and enqueues their
    /// photos into the shared buffer. Each job is only enqueued once.
    async fn job_discovery_loop(
        buffer: Arc<Mutex<PhotoBuffer>>,
        job_store: Arc<dyn JobStore>,
        poll_interval: Duration,
    ) {
        // Track jobs already enqueued to avoid duplicate photos in the buffer
        let mut enqueued_job_ids: HashSet<Uuid> = HashSet::new();

        loop {
            match job_store.list().await {
                Ok(jobs) => {
                    // Clean up finished jobs so their IDs can be reused (edge case)
                    for job in &jobs {
                        if job.is_finished() {
                            enqueued_job_ids.remove(&job.job_id);
                        }
                    }

                    for job in jobs {
                        if job.status == JobStatus::Queued
                            && !job.queued_photo_ids.is_empty()
                            && !enqueued_job_ids.contains(&job.job_id)
                        {
                            let mut buf = buffer.lock().await;
                            buf.enqueue_job(&job);
                            enqueued_job_ids.insert(job.job_id);

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

            tokio::time::sleep(poll_interval).await;
        }
    }

    /// Remove all pending photos for a job from the shared buffer.
    ///
    /// Called when a job is cancelled: any photos still waiting in the queue
    /// are discarded so workers will not process them. Photos already picked
    /// up by a worker will still complete, but `update_job` will detect the
    /// terminal state and skip the update.
    ///
    /// Returns the number of photos removed from the buffer.
    pub async fn cancel_job(&self, job_id: Uuid) -> usize {
        let mut buf = self.buffer.lock().await;
        buf.remove_job(job_id)
    }

    /// Abort all workers and the discovery task immediately.
    ///
    /// Jobs in `Processing` state will be recovered at next startup.
    pub async fn shutdown(&mut self) {
        info!("Shutting down worker pool");

        if let Some(task) = self.discovery_task.take() {
            task.abort();
        }

        for worker in self.workers.drain(..) {
            worker.abort();
        }

        info!("Worker pool shut down");
    }
}
