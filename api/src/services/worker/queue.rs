use std::collections::{HashMap, VecDeque};
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
// PhotoBuffer
// ============================================================================

/// Storage buffer for photos pending processing.
///
/// Photos are grouped by model for efficient per-model lookup.
/// Model insertion order is tracked for fair round-robin in `pop_excluding`.
pub struct PhotoBuffer {
    photos_by_model: HashMap<String, VecDeque<QueuedPhoto>>,
    /// Insertion order of models — used for round-robin in `pop_excluding`.
    model_order: Vec<String>,
    /// Next index in `model_order` to try in `pop_excluding`.
    rotation_index: usize,
    total: usize,
}

impl PhotoBuffer {
    pub fn new() -> Self {
        Self {
            photos_by_model: HashMap::new(),
            model_order: Vec::new(),
            rotation_index: 0,
            total: 0,
        }
    }

    /// Add all pending photos from a job to the buffer.
    pub fn enqueue_job(&mut self, job: &Job) {
        if job.queued_photo_ids.is_empty() {
            return;
        }

        // Track insertion order for round-robin (only on first appearance of the model)
        if !self.photos_by_model.contains_key(&job.model) {
            self.model_order.push(job.model.clone());
        }

        let queue = self.photos_by_model.entry(job.model.clone()).or_default();
        for &photo_id in &job.queued_photo_ids {
            queue.push_back(QueuedPhoto {
                job_id: job.job_id,
                photo_id,
                task_id: job.task_id,
                model: job.model.clone(),
            });
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

    /// Pop from any non-excluded, non-empty model queue using round-robin.
    ///
    /// Models are served in their insertion order, rotating after each call so
    /// that no model is systematically preferred over others when swapping.
    /// Returns `None` if all non-excluded queues are empty.
    pub fn pop_excluding(&mut self, exclude: &str) -> Option<QueuedPhoto> {
        let n = self.model_order.len();
        for i in 0..n {
            let idx = (self.rotation_index + i) % n;
            let model = self.model_order[idx].as_str();
            if model == exclude {
                continue;
            }
            if let Some(queue) = self.photos_by_model.get_mut(model) {
                if let Some(photo) = queue.pop_front() {
                    self.total -= 1;
                    self.rotation_index = (idx + 1) % n;
                    return Some(photo);
                }
            }
        }
        None
    }

    /// Remove all pending photos for a specific job from the buffer.
    ///
    /// Returns the number of photos removed. Used to cancel a job that has
    /// photos waiting in the queue.
    pub fn remove_job(&mut self, job_id: Uuid) -> usize {
        let mut removed = 0;
        for queue in self.photos_by_model.values_mut() {
            let before = queue.len();
            queue.retain(|p| p.job_id != job_id);
            removed += before - queue.len();
        }
        self.total -= removed;
        removed
    }

    /// Returns `true` if the buffer contains no photos.
    pub fn is_empty(&self) -> bool {
        self.total == 0
    }

    /// Returns the number of photos currently in the buffer.
    pub fn len(&self) -> usize {
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

    // --- enqueue_job ---

    #[test]
    fn test_enqueue_job_adds_photos() {
        let mut buf = PhotoBuffer::new();
        buf.enqueue_job(&make_job("llava", 3));
        assert_eq!(buf.len(), 3);
        assert!(!buf.is_empty());
    }

    #[test]
    fn test_enqueue_job_empty_job() {
        let mut buf = PhotoBuffer::new();
        buf.enqueue_job(&make_job("llava", 0));
        assert!(buf.is_empty());
    }

    #[test]
    fn test_enqueue_multiple_jobs_same_model() {
        let mut buf = PhotoBuffer::new();
        buf.enqueue_job(&make_job("llava", 2));
        buf.enqueue_job(&make_job("llava", 3));
        assert_eq!(buf.len(), 5);
    }

    #[test]
    fn test_enqueue_multiple_jobs_different_models() {
        let mut buf = PhotoBuffer::new();
        buf.enqueue_job(&make_job("llava", 2));
        buf.enqueue_job(&make_job("qwen3-vl", 3));
        assert_eq!(buf.len(), 5);
    }

    // --- pop_by_model ---

    #[test]
    fn test_pop_by_model_returns_correct_photo() {
        let mut buf = PhotoBuffer::new();
        let job = make_job("qwen3-vl", 1);
        let expected_job_id = job.job_id;
        let expected_task_id = job.task_id;
        let expected_photo_id = job.queued_photo_ids[0];
        buf.enqueue_job(&job);

        let photo = buf.pop_by_model("qwen3-vl").unwrap();
        assert_eq!(photo.job_id, expected_job_id);
        assert_eq!(photo.task_id, expected_task_id);
        assert_eq!(photo.photo_id, expected_photo_id);
        assert_eq!(photo.model, "qwen3-vl");
    }

    #[test]
    fn test_pop_by_model_decrements_len() {
        let mut buf = PhotoBuffer::new();
        buf.enqueue_job(&make_job("llava", 3));
        buf.pop_by_model("llava");
        assert_eq!(buf.len(), 2);
    }

    #[test]
    fn test_pop_by_model_unknown_model_returns_none() {
        let mut buf = PhotoBuffer::new();
        buf.enqueue_job(&make_job("llava", 2));
        assert!(buf.pop_by_model("qwen3-vl").is_none());
        assert_eq!(buf.len(), 2);
    }

    #[test]
    fn test_pop_by_model_exhausted_returns_none() {
        let mut buf = PhotoBuffer::new();
        buf.enqueue_job(&make_job("llava", 1));
        buf.pop_by_model("llava");
        assert!(buf.pop_by_model("llava").is_none());
        assert!(buf.is_empty());
    }

    // --- pop_any ---

    #[test]
    fn test_pop_any_empty_returns_none() {
        let mut buf = PhotoBuffer::new();
        assert!(buf.pop_any().is_none());
    }

    #[test]
    fn test_pop_any_decrements_len() {
        let mut buf = PhotoBuffer::new();
        buf.enqueue_job(&make_job("llava", 3));
        buf.pop_any();
        assert_eq!(buf.len(), 2);
    }

    #[test]
    fn test_pop_any_drains_buffer() {
        let mut buf = PhotoBuffer::new();
        buf.enqueue_job(&make_job("llava", 2));
        buf.enqueue_job(&make_job("qwen3-vl", 2));
        for _ in 0..4 {
            assert!(buf.pop_any().is_some());
        }
        assert!(buf.is_empty());
        assert!(buf.pop_any().is_none());
    }

    // --- pop_excluding ---

    #[test]
    fn test_pop_excluding_skips_excluded_model() {
        let mut buf = PhotoBuffer::new();
        buf.enqueue_job(&make_job("llava", 2));
        buf.enqueue_job(&make_job("qwen3-vl", 2));

        let photo = buf.pop_excluding("llava").unwrap();
        assert_eq!(photo.model, "qwen3-vl");
    }

    #[test]
    fn test_pop_excluding_returns_none_when_only_excluded_has_photos() {
        let mut buf = PhotoBuffer::new();
        buf.enqueue_job(&make_job("llava", 2));

        assert!(buf.pop_excluding("llava").is_none());
        assert_eq!(buf.len(), 2, "Photos should remain untouched");
    }

    #[test]
    fn test_pop_excluding_returns_none_when_empty() {
        let mut buf = PhotoBuffer::new();
        assert!(buf.pop_excluding("llava").is_none());
    }

    #[test]
    fn test_pop_excluding_round_robin_three_models() {
        let mut buf = PhotoBuffer::new();
        // Enqueue in order: A, B, C
        buf.enqueue_job(&make_job("A", 4));
        buf.enqueue_job(&make_job("B", 4));
        buf.enqueue_job(&make_job("C", 4));

        // Excluding "A": should alternate B → C → B → C
        let p1 = buf.pop_excluding("A").unwrap();
        let p2 = buf.pop_excluding("A").unwrap();
        let p3 = buf.pop_excluding("A").unwrap();
        let p4 = buf.pop_excluding("A").unwrap();

        assert_eq!(p1.model, "B");
        assert_eq!(p2.model, "C");
        assert_eq!(p3.model, "B");
        assert_eq!(p4.model, "C");
    }

    #[test]
    fn test_pop_excluding_skips_empty_queues_in_rotation() {
        let mut buf = PhotoBuffer::new();
        buf.enqueue_job(&make_job("A", 4));
        buf.enqueue_job(&make_job("B", 1)); // only 1 photo
        buf.enqueue_job(&make_job("C", 4));

        // Excluding "A": B → C → (B exhausted) → C → C → ...
        let p1 = buf.pop_excluding("A").unwrap();
        let p2 = buf.pop_excluding("A").unwrap();
        assert_eq!(p1.model, "B");
        assert_eq!(p2.model, "C");

        // B is now empty; next should go to C
        let p3 = buf.pop_excluding("A").unwrap();
        assert_eq!(p3.model, "C");
    }

    #[test]
    fn test_pop_excluding_decrements_len() {
        let mut buf = PhotoBuffer::new();
        buf.enqueue_job(&make_job("llava", 2));
        buf.enqueue_job(&make_job("qwen3-vl", 2));

        buf.pop_excluding("llava");
        assert_eq!(buf.len(), 3);
    }

    // --- remove_job ---

    #[test]
    fn test_remove_job_removes_all_photos_for_job() {
        let mut buf = PhotoBuffer::new();
        let job = make_job("llava", 3);
        let job_id = job.job_id;
        buf.enqueue_job(&job);
        assert_eq!(buf.len(), 3);

        let removed = buf.remove_job(job_id);
        assert_eq!(removed, 3);
        assert!(buf.is_empty());
    }

    #[test]
    fn test_remove_job_only_removes_target_job() {
        let mut buf = PhotoBuffer::new();
        let job1 = make_job("llava", 2);
        let job2 = make_job("llava", 3);
        let job1_id = job1.job_id;
        buf.enqueue_job(&job1);
        buf.enqueue_job(&job2);
        assert_eq!(buf.len(), 5);

        let removed = buf.remove_job(job1_id);
        assert_eq!(removed, 2);
        assert_eq!(buf.len(), 3);
    }

    #[test]
    fn test_remove_job_across_models() {
        let mut buf = PhotoBuffer::new();
        let job1 = make_job("llava", 2);
        let job2 = make_job("qwen3-vl", 3);
        let job1_id = job1.job_id;
        buf.enqueue_job(&job1);
        buf.enqueue_job(&job2);

        let removed = buf.remove_job(job1_id);
        assert_eq!(removed, 2);
        assert_eq!(buf.len(), 3);
    }

    #[test]
    fn test_remove_job_unknown_job_returns_zero() {
        let mut buf = PhotoBuffer::new();
        buf.enqueue_job(&make_job("llava", 2));

        let removed = buf.remove_job(Uuid::new_v4());
        assert_eq!(removed, 0);
        assert_eq!(buf.len(), 2);
    }
}
