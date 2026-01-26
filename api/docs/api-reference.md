# Photometoria API Reference

## Overview

This document provides complete reference documentation for all REST API endpoints in the Photometoria API server.

## Base URL

```
http://localhost:8080/api
```

(Default port is 8080, configurable in `config.toml`)

## Data Flow

### Complete Workflow Example

This example demonstrates a typical workflow from task creation to cleanup:

```
1. Client creates a task
   POST /api/tasks
   {context: "vacation in San Francisco"}
   ← {task_id: "task_abc"}

2. Client uploads photos (single or batch)
   POST /api/tasks/task_abc/photos
   {files: [photo1.jpg, photo2.jpg, ...]}
   ← {photo_ids: ["p1", "p2", "p3"]}

3. Client starts analysis job
   POST /api/tasks/task_abc/jobs
   {
     model: "qwen2-vl:8b",
     photo_ids: null  // null = all photos in task
   }
   ← {job_id: "job_xyz", status: "queued"}

4. Client monitors via SSE
   GET /api/jobs/job_xyz/stream
   ← Real-time events as job progresses

5. Client retrieves results
   GET /api/jobs/job_xyz/results
   ← {results: [{photo_id, status, tags}, ...]}

6. If some photos failed, retry them
   POST /api/jobs/job_xyz/retry
   ← {job_id: "job_new", ...}

7. Or re-analyze with different model
   POST /api/tasks/task_abc/jobs
   {
     model: "llava:latest",
     photo_ids: null
   }
   ← {job_id: "job_uvw"}

8. When done, cleanup
   DELETE /api/tasks/task_abc
   → Deletes task, all photos, and all associated jobs
```

## System Endpoints

### GET /api/config

Returns server configuration and limits relevant to the client.

**Response:**

```json
{
  "upload": {
    "max_photos_per_request": 50,
    "max_photo_size_mb": 20
  },
  "storage": {
    "total_gb": 100,
    "used_gb": 23.5,
    "available_gb": 76.5
  },
  "limits": {
    "max_concurrent_jobs": 2,
    "max_tasks": null
  },
  "version": "0.1.0"
}
```

**Note:** `max_tasks: null` indicates that task count limits are not currently enforced. Future versions may introduce configurable quotas.

### GET /api/models

Returns list of supported AI models that are currently available (both configured and installed in Ollama).

**Response:**

```json
{
  "models": [
    {
      "name": "qwen2-vl:8b",
      "description": "Best quality, slower processing",
      "available": true
    },
    {
      "name": "llava",
      "description": "Faster, good for testing",
      "available": true
    }
  ]
}
```

## Task Endpoints

### POST /api/tasks

Creates a new task. Multiple tasks can be active simultaneously.

**Note:** The current implementation does not enforce task count limits. Future versions may introduce configurable quotas.

**Request:**

```json
{
  "context": "vacation in San Francisco, summer 2024"
}
```

**Response:**

```json
{
  "task_id": "task_abc",
  "context": "vacation in San Francisco, summer 2024",
  "created_at": "2024-01-15T10:30:00Z"
}
```

### GET /api/tasks

Returns list of all tasks.

**Response:**

```json
{
  "tasks": [
    {
      "task_id": "task_abc",
      "context": "...",
      "photo_count": 15,
      "storage_used_mb": 45.2,
      "created_at": "...",
      "job_count": 2
    }
  ]
}
```

### GET /api/tasks/{task_id}

Returns detailed information about a specific task, including all associated jobs.

**Response:**

```json
{
  "task_id": "task_abc",
  "context": "vacation in SF",
  "created_at": "2024-01-15T10:30:00Z",
  "photo_count": 15,
  "storage_used_mb": 45.2,
  "jobs": [
    {
      "job_id": "job_xyz",
      "status": "completed",
      "model": "qwen2-vl:8b",
      "photo_count": 15,
      "created_at": "2024-01-15T10:35:00Z",
      "completed_at": "2024-01-15T10:45:00Z"
    },
    {
      "job_id": "job_uvw",
      "status": "processing",
      "model": "llava",
      "photo_count": 15,
      "created_at": "2024-01-15T10:50:00Z"
    }
  ]
}
```

### PATCH /api/tasks/{task_id}

Updates the task context.

**Request:**

```json
{
  "context": "updated context information"
}
```

**Response:**

```json
{
  "task_id": "task_abc",
  "context": "updated context information"
}
```

### DELETE /api/tasks/{task_id}

Deletes a task and all associated resources (photos, jobs).

**Errors:**

- `409` - Cannot delete task with active jobs

## Photo Endpoints

### POST /api/tasks/{task_id}/photos

Uploads one or more photos to a task using multipart/form-data.

**Request:**

- Content-Type: multipart/form-data
- Field name: "files" (can be repeated for multiple files)
- Limits: max_photos_per_request, max_photo_size_mb (from config)

**Response:**

```json
{
  "photo_ids": [
    "p1",
    "p2",
    "p3"
  ],
  "uploaded_count": 3,
  "total_size_mb": 12.4
}
```

**Errors:**

- `400` - File too large or too many files
- `404` - Task not found
- `507` - Insufficient storage space

### GET /api/tasks/{task_id}/photos

Returns list of photo IDs in the task.

**Response:**

```json
{
  "photo_ids": [
    "p1",
    "p2",
    "p3",
    "p4",
    "p5"
  ],
  "count": 5
}
```

### GET /api/photos/{photo_id}

Returns detailed information about a specific photo.

**Response:**

```json
{
  "photo_id": "p1",
  "task_id": "task_abc",
  "filename": "IMG_1234.jpg",
  "size_mb": 4.2,
  "uploaded_at": "2024-01-15T10:32:00Z"
}
```

### DELETE /api/tasks/{task_id}/photos/{photo_id}

Deletes a specific photo from the task.

**Errors:**

- `409` - Cannot delete photo referenced by active jobs

## Job Endpoints

### POST /api/tasks/{task_id}/jobs

Creates and starts a new analysis job.

**Request:**

```json
{
  "model": "qwen2-vl:8b",
  "photo_ids": null
  // null = all photos, or array of specific IDs
}
```

**Response:**

```json
{
  "job_id": "job_xyz",
  "task_id": "task_abc",
  "status": "queued",
  "photo_count": 15,
  "model": "qwen2-vl:8b",
  "created_at": "2024-01-15T10:35:00Z",
  "queue_position": 1
  // optional, if in queue
}
```

**Errors:**

- `400` - Invalid model or photo_ids
- `404` - Task not found

### POST /api/jobs/{job_id}/retry

Retries only the failed photos from a completed job, using the same model and enriched context from successfully processed photos.

**Response:**

```json
{
  "job_id": "job_new_123",
  "parent_job_id": "job_xyz",
  "task_id": "task_abc",
  "status": "queued",
  "photo_count": 2,
  // only failed photos
  "model": "qwen2-vl:8b",
  "retry": true,
  "created_at": "2024-01-15T10:50:00Z"
}
```

**Errors:**

- `400` - No failed photos to retry
- `409` - Original job still processing

### GET /api/jobs

Returns list of all jobs across all tasks.

**Response:**

```json
{
  "jobs": [
    {
      "job_id": "job_xyz",
      "task_id": "task_abc",
      "status": "completed",
      "model": "qwen2-vl:8b",
      "photo_count": 15,
      "created_at": "...",
      "completed_at": "..."
    }
  ]
}
```

### GET /api/jobs/{job_id}

Returns current state of a specific job.

**Response:**

```json
{
  "job_id": "job_xyz",
  "task_id": "task_abc",
  "status": "processing",
  "model": "qwen2-vl:8b",
  "photo_count": 15,
  "progress": {
    "completed": 7,
    "failed": 1,
    "remaining": 7,
    "current_photo_id": "p8"
  },
  "created_at": "2024-01-15T10:35:00Z"
}
```

### GET /api/jobs/{job_id}/results

Returns analysis results. Available even for jobs in "processing" or "cancelled" state (partial results).

**Response:**

```json
{
  "job_id": "job_xyz",
  "task_id": "task_abc",
  "status": "completed",
  "model": "qwen2-vl:8b",
  "results": [
    {
      "photo_id": "p1",
      "status": "completed",
      "tags": "golden gate bridge, sunset, long exposure, red suspension cables",
      "processed_at": "2024-01-15T10:36:00Z"
    },
    {
      "photo_id": "p2",
      "status": "failed",
      "error": "ollama timeout",
      "tags": null
    },
    {
      "photo_id": "p3",
      "status": "completed",
      "tags": "san francisco bay, sailboat, clear sky, afternoon light"
    }
  ],
  "summary": {
    "total": 15,
    "completed": 13,
    "failed": 2
  }
}
```

### GET /api/jobs/{job_id}/stream

Opens a Server-Sent Events (SSE) stream for real-time job updates.

**Event Types:**

**started:**

```json
{
  "event": "started",
  "job_id": "job_xyz",
  "total_photos": 15
}
```

**progress:**

```json
{
  "event": "progress",
  "photo_id": "p1",
  "status": "completed",
  "progress": "1/15"
}
```

**progress (failed photo):**

```json
{
  "event": "progress",
  "photo_id": "p2",
  "status": "failed",
  "error": "ollama timeout",
  "progress": "2/15"
}
```

**completed:**

```json
{
  "event": "completed",
  "job_id": "job_xyz",
  "total": 15,
  "succeeded": 13,
  "failed": 2
}
```

**cancelled:**

```json
{
  "event": "cancelled",
  "job_id": "job_xyz"
}
```

**Client Disconnection:**

When a client disconnects from the SSE stream, the server detects this and marks the job as "abandoned". Currently, no automatic timeout/cleanup is implemented for abandoned jobs (future enhancement).

### DELETE /api/jobs/{job_id}

Cancels and deletes a job.

**Behavior:**

- If job is running: completes current photo processing, then stops
- Partial results are preserved and retrievable
- Associated photos remain in the task

**Response:**

```json
{
  "job_id": "job_xyz",
  "status": "cancelled"
}
```

## Data Models

### Task

```json
{
  "task_id": "string (UUID)",
  "context": "string",
  "created_at": "ISO 8601 timestamp"
}
```

### Photo

```json
{
  "photo_id": "string (UUID)",
  "task_id": "string (UUID)",
  "filename": "string",
  "size_mb": "number",
  "uploaded_at": "ISO 8601 timestamp"
}
```

### Job

```json
{
  "job_id": "string (UUID)",
  "task_id": "string (UUID)",
  "status": "queued|processing|completed|failed|cancelled",
  "model": "string",
  "photo_ids": [
    "string (UUID)"
  ]
  |
  null,
  "created_at": "ISO 8601 timestamp",
  "started_at": "ISO 8601 timestamp | null",
  "completed_at": "ISO 8601 timestamp | null"
}
```

### Result

```json
{
  "photo_id": "string (UUID)",
  "status": "completed|failed",
  "tags": "string (comma-separated) | null",
  "error": "string | null",
  "processed_at": "ISO 8601 timestamp | null"
}
```

## Error Handling

### Error Response Format

All errors follow a consistent JSON format:

```json
{
  "error": "error_code",
  "message": "Human-readable error description"
}
```

### Common Error Codes

- `task_not_found` (404) - Specified task does not exist
- `job_not_found` (404) - Specified job does not exist
- `photo_not_found` (404) - Specified photo does not exist
- `invalid_model` (400) - Model not in supported/available list
- `invalid_photo_ids` (400) - Photo IDs invalid or not in task
- `file_too_large` (400) - Uploaded file exceeds max_photo_size_mb
- `too_many_files` (400) - Upload exceeds max_photos_per_request
- `insufficient_storage` (507) - Storage quota exceeded
- `resource_in_use` (409) - Cannot delete resource referenced by active jobs
- `no_failed_photos` (400) - Retry requested but no failed photos
- `job_still_processing` (409) - Operation not allowed on active job

## See Also

- [Architecture](architecture.md) - System design and core concepts
- [Configuration](configuration.md) - Server configuration reference
- [Development Guide](development.md) - Testing and workflow
