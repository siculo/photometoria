// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::config::worker_pool::parse_duration;
use crate::config::{Config, ProviderConfig};
use crate::models::activity::ActivityStatus;
use crate::services::ai::ProviderRegistry;
use crate::storage::{ActivityStore, PhotoStore, ProjectStore};

use super::processor::PhotoProcessor;
use super::queue::PhotoBuffer;
use super::worker::Worker;

pub struct WorkerPool {
    /// Worker task handles (one per GPU)
    workers: Vec<JoinHandle<()>>,

    /// Single shared photo buffer — all workers pull from here
    buffer: Arc<Mutex<PhotoBuffer>>,

    /// Activity store for polling
    activity_store: Arc<dyn ActivityStore>,

    /// How often the discovery loop polls for new queued activities.
    discovery_poll_interval: Duration,

    /// Activity discovery task handle
    discovery_task: Option<JoinHandle<()>>,
}

impl WorkerPool {
    pub fn new(
        config: &Config,
        activity_store: Arc<dyn ActivityStore>,
        photo_store: Arc<dyn PhotoStore>,
        project_store: Arc<dyn ProjectStore>,
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
                activity_store.clone(),
                photo_store.clone(),
                project_store.clone(),
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
            activity_store,
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
    /// For use in tests where activity processing should not run.
    #[cfg(test)]
    pub fn new_inactive(activity_store: Arc<dyn ActivityStore>) -> Self {
        Self {
            workers: Vec::new(),
            buffer: Arc::new(Mutex::new(PhotoBuffer::new())),
            activity_store,
            discovery_poll_interval: Duration::from_secs(5),
            discovery_task: None,
        }
    }

    /// Start the worker pool: recover stale activities, then begin activity discovery.
    pub async fn start(&mut self) {
        Self::recover_stale_activities(self.activity_store.clone()).await;

        let buffer = self.buffer.clone();
        let activity_store = self.activity_store.clone();
        let poll_interval = self.discovery_poll_interval;

        let handle = tokio::spawn(async move {
            Self::activity_discovery_loop(buffer, activity_store, poll_interval).await;
        });

        self.discovery_task = Some(handle);
        info!("Worker pool started");
    }

    /// Reset activities that were in `Processing` state from a previous run.
    ///
    /// These are stale (server crashed/shutdown mid-processing). Activities with
    /// remaining photos are reset to `Queued`; activities with all photos processed
    /// are marked `Completed`.
    async fn recover_stale_activities(activity_store: Arc<dyn ActivityStore>) {
        info!("Checking for stale activities to recover");

        match activity_store.list().await {
            Ok(activities) => {
                let mut recovered_count = 0;

                for mut activity in activities {
                    if activity.status == ActivityStatus::Processing {
                        info!(
                            activity_id = %activity.activity_id,
                            queued = activity.queued_photo_ids.len(),
                            processed = activity.processed_photo_ids.len(),
                            "Recovering stale activity from previous run"
                        );

                        if !activity.queued_photo_ids.is_empty() {
                            activity.status = ActivityStatus::Queued;
                            activity.started_at = None;

                            if let Err(e) = activity_store.update(activity).await {
                                error!(error = %e, "Failed to recover stale activity");
                            } else {
                                recovered_count += 1;
                            }
                        } else {
                            // All photos were already processed; just mark completed
                            activity.complete();

                            if let Err(e) = activity_store.update(activity).await {
                                error!(error = %e, "Failed to complete recovered activity");
                            } else {
                                info!("Recovered activity marked as completed");
                                recovered_count += 1;
                            }
                        }
                    }
                }

                if recovered_count > 0 {
                    info!(count = recovered_count, "Recovered stale activities");
                } else {
                    info!("No stale activities found");
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to list activities for recovery");
            }
        }
    }

    /// Polls the ActivityStore every N seconds for `Queued` activities and enqueues their
    /// photos into the shared buffer. Each activity is only enqueued once.
    async fn activity_discovery_loop(
        buffer: Arc<Mutex<PhotoBuffer>>,
        activity_store: Arc<dyn ActivityStore>,
        poll_interval: Duration,
    ) {
        // Track activities already enqueued to avoid duplicate photos in the buffer
        let mut enqueued_activity_ids: HashSet<Uuid> = HashSet::new();

        loop {
            match activity_store.list().await {
                Ok(activities) => {
                    // Clean up finished activities so their IDs can be reused (edge case)
                    for activity in &activities {
                        if activity.is_finished() {
                            enqueued_activity_ids.remove(&activity.activity_id);
                        }
                    }

                    for activity in activities {
                        if activity.status == ActivityStatus::Queued
                            && !activity.queued_photo_ids.is_empty()
                            && !enqueued_activity_ids.contains(&activity.activity_id)
                        {
                            let mut buf = buffer.lock().await;
                            buf.enqueue_activity(&activity);
                            enqueued_activity_ids.insert(activity.activity_id);

                            info!(
                                activity_id = %activity.activity_id,
                                photo_count = activity.queued_photo_ids.len(),
                                model = %activity.model,
                                "Enqueued activity photos into shared buffer"
                            );
                        }
                    }
                }
                Err(e) => {
                    warn!(error = %e, "Failed to list activities for discovery");
                }
            }

            tokio::time::sleep(poll_interval).await;
        }
    }

    /// Remove all pending photos for an activity from the shared buffer.
    ///
    /// Called when an activity is cancelled: any photos still waiting in the queue
    /// are discarded so workers will not process them. Photos already picked
    /// up by a worker will still complete, but `update_activity` will detect the
    /// terminal state and skip the update.
    ///
    /// Returns the number of photos removed from the buffer.
    pub async fn cancel_activity(&self, activity_id: Uuid) -> usize {
        let mut buf = self.buffer.lock().await;
        buf.remove_activity(activity_id)
    }

    /// Abort all workers and the discovery task immediately.
    ///
    /// Activities in `Processing` state will be recovered at next startup.
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
