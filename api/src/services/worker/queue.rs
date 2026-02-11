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

/// Pure storage buffer for photos pending processing.
///
/// Photos are grouped by model for efficient per-model lookup.
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
                    queue.push_back(QueuedPhoto {
                        job_id: job.job_id,
                        photo_id,
                        task_id: job.task_id,
                        model: job.model.clone(),
                    });
                }
                None => {
                    let mut queue = VecDeque::new();
                    queue.push_back(QueuedPhoto {
                        job_id: job.job_id,
                        photo_id,
                        task_id: job.task_id,
                        model: job.model.clone(),
                    });
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
        buf.enqueue_job(&make_job("qwen2-vl", 3));
        assert_eq!(buf.len(), 5);
    }

    // --- pop_by_model ---

    #[test]
    fn test_pop_by_model_returns_correct_photo() {
        let mut buf = PhotoBuffer::new();
        let job = make_job("qwen2-vl", 1);
        let expected_job_id = job.job_id;
        let expected_task_id = job.task_id;
        let expected_photo_id = job.queued_photo_ids[0];
        buf.enqueue_job(&job);

        let photo = buf.pop_by_model("qwen2-vl").unwrap();
        assert_eq!(photo.job_id, expected_job_id);
        assert_eq!(photo.task_id, expected_task_id);
        assert_eq!(photo.photo_id, expected_photo_id);
        assert_eq!(photo.model, "qwen2-vl");
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
        assert!(buf.pop_by_model("qwen2-vl").is_none());
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
        buf.enqueue_job(&make_job("qwen2-vl", 2));
        for _ in 0..4 {
            assert!(buf.pop_any().is_some());
        }
        assert!(buf.is_empty());
        assert!(buf.pop_any().is_none());
    }

    // --- available_models ---

    #[test]
    fn test_available_models_empty_buffer() {
        let buf = PhotoBuffer::new();
        assert!(buf.available_models().is_empty());
    }

    #[test]
    fn test_available_models_lists_enqueued_models() {
        let mut buf = PhotoBuffer::new();
        buf.enqueue_job(&make_job("llava", 1));
        buf.enqueue_job(&make_job("qwen2-vl", 1));
        let mut models = buf.available_models();
        models.sort();
        assert_eq!(models, vec!["llava", "qwen2-vl"]);
    }

    #[test]
    fn test_available_models_excludes_exhausted_model() {
        let mut buf = PhotoBuffer::new();
        buf.enqueue_job(&make_job("llava", 1));
        buf.enqueue_job(&make_job("qwen2-vl", 2));
        buf.pop_by_model("llava");
        let models = buf.available_models();
        assert!(!models.contains(&"llava".to_string()));
        assert!(models.contains(&"qwen2-vl".to_string()));
    }
}
