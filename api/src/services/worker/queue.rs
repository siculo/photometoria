use std::collections::HashMap;
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::models::job::Job;

// ============================================================================
// QueuedPhoto
// ============================================================================

/// A photo waiting to be processed, with its job context.
#[derive(Debug, Clone)]
pub struct QueuedPhoto {
    pub job_id: Uuid,
    pub photo_id: Uuid,
    pub task_id: Uuid,
    pub model: String,
}

// ============================================================================
// PhotoQueue trait
// ============================================================================

/// Trait for a photo queue: encapsulates both storage and selection logic.
///
/// Implementations are free to choose their internal data structure and
/// selection strategy. The Worker calls only `enqueue_job` and `dequeue`.
pub trait PhotoQueue: Send {
    /// Add all pending photos from a job to the queue.
    fn enqueue_job(&mut self, job: &Job);

    /// Dequeue the next photo according to the implementation's selection logic.
    /// Returns `None` if the queue is empty.
    fn dequeue(&mut self) -> Option<QueuedPhoto>;

    /// Returns `true` if the queue contains no photos.
    fn is_empty(&self) -> bool;

    /// Returns the number of photos currently in the queue.
    fn len(&self) -> usize;
}

// ============================================================================
// HybridThresholdQueue
// ============================================================================

/// Photo queue with hybrid threshold selection strategy.
///
/// Prefers the currently loaded model until **both** thresholds are exceeded:
/// - `min_photos_before_swap`: minimum number of photos processed with the current model
/// - `max_time_before_swap`: maximum time elapsed since the current model was loaded
///
/// When both thresholds are met, the next dequeue **prefers a different model**
/// to ensure temporal fairness (no job waits indefinitely), falling back to the
/// current model only if no other model has queued photos.
pub struct HybridThresholdQueue {
    /// Photos grouped by model for efficient preferred-model lookup.
    photos_by_model: HashMap<String, VecDeque<QueuedPhoto>>,

    /// Total number of photos across all model queues.
    total: usize,

    /// Minimum photos to process before allowing a model swap.
    min_photos_before_swap: usize,

    /// Maximum time with the same model before forcing a swap.
    max_time_before_swap: Duration,

    /// The model currently loaded (None until the first photo is dequeued).
    current_model: Option<String>,

    /// Number of photos dequeued with the current model.
    photos_processed_with_model: usize,

    /// When the current model was last loaded.
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

    /// Returns `true` if the hybrid threshold allows switching to a different model.
    ///
    /// A swap is allowed when no model is loaded yet, or when BOTH conditions hold:
    /// - at least `min_photos_before_swap` photos have been processed with the current model
    /// - at least `max_time_before_swap` has elapsed since the current model was loaded
    fn can_swap_model(&self) -> bool {
        match self.current_model {
            None => true,
            Some(_) => {
                let photos_ok =
                    self.photos_processed_with_model >= self.min_photos_before_swap;
                let time_ok = self.model_loaded_at.elapsed() >= self.max_time_before_swap;
                photos_ok && time_ok
            }
        }
    }

    /// Called after dequeuing a photo to update selection state.
    fn on_photo_dequeued(&mut self, model: &str) {
        if self.current_model.as_deref() != Some(model) {
            self.current_model = Some(model.to_string());
            self.photos_processed_with_model = 1;
            self.model_loaded_at = Instant::now();
        } else {
            self.photos_processed_with_model += 1;
        }
    }

    /// Pops the front photo from a specific model's queue.
    fn pop_model(&mut self, model: &str) -> Option<QueuedPhoto> {
        self.photos_by_model
            .get_mut(model)
            .and_then(|q| q.pop_front())
    }

    /// Pops the front photo from any non-empty model queue.
    fn pop_any(&mut self) -> Option<QueuedPhoto> {
        self.photos_by_model
            .values_mut()
            .find(|q| !q.is_empty())
            .and_then(|q| q.pop_front())
    }

    /// Pops the front photo from any model except the given one.
    fn pop_other_model(&mut self, exclude: &str) -> Option<QueuedPhoto> {
        self.photos_by_model
            .iter_mut()
            .find(|(model, q)| !q.is_empty() && model.as_str() != exclude)
            .and_then(|(_, q)| q.pop_front())
    }
}

impl PhotoQueue for HybridThresholdQueue {
    fn enqueue_job(&mut self, job: &Job) {
        for &photo_id in &job.queued_photo_ids {
            self.photos_by_model
                .entry(job.model.clone())
                .or_default()
                .push_back(QueuedPhoto {
                    job_id: job.job_id,
                    photo_id,
                    task_id: job.task_id,
                    model: job.model.clone(),
                });
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
            Some(current) if self.can_swap_model() => self
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::job::Job;

    fn make_job(model: &str, photo_count: usize) -> Job {
        let photo_ids = (0..photo_count).map(|_| Uuid::new_v4()).collect();
        Job::new(Uuid::new_v4(), model.to_string(), photo_ids)
    }

    fn make_queue() -> HybridThresholdQueue {
        // High thresholds: swap never allowed in normal tests
        HybridThresholdQueue::new(100, Duration::from_secs(3600))
    }

    fn make_permissive_queue() -> HybridThresholdQueue {
        // Zero thresholds: swap always allowed
        HybridThresholdQueue::new(0, Duration::ZERO)
    }

    // --- enqueue_job ---

    #[test]
    fn test_enqueue_job_adds_photos() {
        let mut q = make_queue();
        let job = make_job("llava", 3);
        q.enqueue_job(&job);
        assert_eq!(q.len(), 3);
        assert!(!q.is_empty());
    }

    #[test]
    fn test_enqueue_job_empty_job() {
        let mut q = make_queue();
        let job = make_job("llava", 0);
        q.enqueue_job(&job);
        assert!(q.is_empty());
    }

    #[test]
    fn test_enqueue_multiple_jobs_same_model() {
        let mut q = make_queue();
        q.enqueue_job(&make_job("llava", 2));
        q.enqueue_job(&make_job("llava", 3));
        assert_eq!(q.len(), 5);
    }

    #[test]
    fn test_enqueue_multiple_jobs_different_models() {
        let mut q = make_queue();
        q.enqueue_job(&make_job("llava", 2));
        q.enqueue_job(&make_job("qwen2-vl", 3));
        assert_eq!(q.len(), 5);
    }

    // --- dequeue / is_empty / len ---

    #[test]
    fn test_dequeue_empty_returns_none() {
        let mut q = make_queue();
        assert!(q.dequeue().is_none());
    }

    #[test]
    fn test_dequeue_decrements_len() {
        let mut q = make_queue();
        q.enqueue_job(&make_job("llava", 3));
        q.dequeue();
        assert_eq!(q.len(), 2);
    }

    #[test]
    fn test_dequeue_all_until_empty() {
        let mut q = make_queue();
        q.enqueue_job(&make_job("llava", 3));
        q.dequeue();
        q.dequeue();
        q.dequeue();
        assert!(q.is_empty());
        assert!(q.dequeue().is_none());
    }

    #[test]
    fn test_dequeue_returns_correct_job_context() {
        let mut q = make_queue();
        let job = make_job("qwen2-vl", 1);
        let expected_job_id = job.job_id;
        let expected_task_id = job.task_id;
        let expected_photo_id = job.queued_photo_ids[0];
        q.enqueue_job(&job);

        let photo = q.dequeue().unwrap();
        assert_eq!(photo.job_id, expected_job_id);
        assert_eq!(photo.task_id, expected_task_id);
        assert_eq!(photo.photo_id, expected_photo_id);
        assert_eq!(photo.model, "qwen2-vl");
    }

    // --- model preference ---

    #[test]
    fn test_prefers_current_model_when_below_thresholds() {
        let mut q = make_queue(); // thresholds: 100 photos, 1 hour

        q.enqueue_job(&make_job("llava", 5));
        q.enqueue_job(&make_job("qwen2-vl", 5));

        // First dequeue: no current model yet, any photo
        let first = q.dequeue().unwrap();
        let first_model = first.model.clone();

        // All subsequent dequeues should return the same model (threshold not met)
        for _ in 0..4 {
            let p = q.dequeue().unwrap();
            assert_eq!(p.model, first_model, "should prefer current model");
        }
    }

    #[test]
    fn test_falls_back_to_other_model_when_current_exhausted() {
        let mut q = make_queue(); // won't allow swap, but current model is empty

        q.enqueue_job(&make_job("llava", 1));
        q.enqueue_job(&make_job("qwen2-vl", 2));

        // Dequeue "llava" (or whichever comes first)
        let first = q.dequeue().unwrap();
        // That model's queue is now empty; fallback must yield the other model
        if first.model == "llava" {
            let next = q.dequeue().unwrap();
            assert_eq!(next.model, "qwen2-vl");
        } else {
            // first was qwen2-vl; still one qwen2-vl left so next is qwen2-vl
            let next = q.dequeue().unwrap();
            assert_eq!(next.model, "qwen2-vl");
        }
    }

    #[test]
    fn test_allows_swap_when_both_thresholds_met() {
        // Zero thresholds: swap allowed immediately
        let mut q = make_permissive_queue();

        q.enqueue_job(&make_job("llava", 3));
        q.enqueue_job(&make_job("qwen2-vl", 3));

        let mut models: Vec<String> = Vec::new();
        for _ in 0..6 {
            if let Some(p) = q.dequeue() {
                models.push(p.model);
            }
        }

        // With zero thresholds both models should appear
        assert!(models.iter().any(|m| m == "llava"), "expected llava in results");
        assert!(
            models.iter().any(|m| m == "qwen2-vl"),
            "expected qwen2-vl in results"
        );
    }

    #[test]
    fn test_swap_allowed_prefers_different_model() {
        // Zero thresholds: swap always allowed
        let mut q = make_permissive_queue();

        q.enqueue_job(&make_job("llava", 3));
        q.enqueue_job(&make_job("qwen2-vl", 3));

        // First dequeue: no current model, picks any model
        let first = q.dequeue().unwrap();
        let first_model = first.model.clone();

        // Second dequeue: swap is allowed (both thresholds are zero).
        // Since another model has photos queued, should switch to it.
        let second = q.dequeue().unwrap();
        assert_ne!(
            second.model, first_model,
            "When swap is allowed and another model has queued photos, \
             dequeue should prefer a different model"
        );
    }

    #[test]
    fn test_counter_resets_on_model_change() {
        // Allow swap after exactly 1 photo + 0 time
        let mut q = HybridThresholdQueue::new(1, Duration::ZERO);

        q.enqueue_job(&make_job("llava", 2));
        q.enqueue_job(&make_job("qwen2-vl", 1));

        // Dequeue one photo: establishes current model, counter = 1
        let p1 = q.dequeue().unwrap();
        // Both thresholds met (>= 1 photo, >= 0 time): can swap
        // Dequeue again — may switch model
        let p2 = q.dequeue().unwrap();

        if p1.model != p2.model {
            // Model changed: counter should have reset to 1
            assert_eq!(q.photos_processed_with_model, 1);
            assert_eq!(q.current_model.as_deref(), Some(p2.model.as_str()));
        }
    }

    #[test]
    fn test_photo_queue_trait_object() {
        // Ensure HybridThresholdQueue can be used as a trait object
        let mut q: Box<dyn PhotoQueue> =
            Box::new(HybridThresholdQueue::new(10, Duration::from_secs(120)));

        let job = make_job("llava", 2);
        q.enqueue_job(&job);
        assert_eq!(q.len(), 2);
        assert!(q.dequeue().is_some());
        assert_eq!(q.len(), 1);
    }
}
