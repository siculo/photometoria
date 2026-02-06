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
   ← {task_id: "a1b2c3d4-e5f6-7890-abcd-ef1234567890"}

2. Client uploads photos (single or batch)
   POST /api/tasks/a1b2c3d4-e5f6-7890-abcd-ef1234567890/photos
   {files: [photo1.jpg, photo2.jpg, ...]}
   ← {uploaded: [{photo_id: "f0e1d2c3-b4a5-6789-0fed-cba987654321", ...}, ...], failed: [], uploaded_size_bytes: ...}

3. Client starts analysis job
   POST /api/tasks/a1b2c3d4-e5f6-7890-abcd-ef1234567890/jobs
   {
     model: "qwen2-vl:8b",
     photo_ids: null  // null = all photos in task
   }
   ← {job_id: "12345678-abcd-ef01-2345-6789abcdef01", status: "queued"}

4. Client monitors via SSE
   GET /api/jobs/12345678-abcd-ef01-2345-6789abcdef01/stream
   ← Real-time events as job progresses

5. Client retrieves results
   GET /api/jobs/12345678-abcd-ef01-2345-6789abcdef01/results
   ← {results: [{photo_id, status, tags}, ...]}

6. If some photos failed, retry them
   POST /api/jobs/12345678-abcd-ef01-2345-6789abcdef01/retry
   ← {job_id: "fedcba98-7654-3210-fedc-ba9876543210", ...}

7. Or re-analyze with different model
   POST /api/tasks/a1b2c3d4-e5f6-7890-abcd-ef1234567890/jobs
   {
     model: "llava:latest",
     photo_ids: null
   }
   ← {job_id: "abcdef01-2345-6789-abcd-ef0123456789"}

8. When done, cleanup
   DELETE /api/tasks/a1b2c3d4-e5f6-7890-abcd-ef1234567890
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
    "max_photo_size_bytes": 20971520
  },
  "storage": {
    "total_bytes": 107374182400,
    "used_bytes": 25243074560,
    "available_bytes": 82131107840
  },
  "limits": {
    "max_concurrent_jobs": 2,
    "max_tasks": null
  },
  "version": "0.1.0"
}
```

**Note:** `max_tasks: null` indicates that task count limits are not currently enforced. Future versions may introduce configurable quotas. All size values are in bytes for consistency with other API responses.

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
  "task_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
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
      "task_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
      "context": "...",
      "photo_count": 15,
      "storage_used": 47395430,
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
  "task_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "context": "vacation in SF",
  "created_at": "2024-01-15T10:30:00Z",
  "photo_count": 15,
  "storage_used": 47395430,
  "jobs": [
    {
      "job_id": "12345678-abcd-ef01-2345-6789abcdef01",
      "status": "completed",
      "model": "qwen2-vl:8b",
      "photo_count": 15,
      "created_at": "2024-01-15T10:35:00Z",
      "completed_at": "2024-01-15T10:45:00Z"
    },
    {
      "job_id": "abcdef01-2345-6789-abcd-ef0123456789",
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
  "task_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
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
- Field `client_ids`: JSON array of strings, one identifier per file (required)
- Field `files`: image files (can be repeated for multiple files)
- Limits: max_photos_per_request, max_photo_size (from config)

The `client_ids` array must have the same length as the number of `files` fields.
Each client_id is returned in the response, allowing the client to track which
local file corresponds to which photo_id.

**Behavior:**

The endpoint processes all photos and returns details about successes and failures.
Photos are uploaded as long as storage space is available and validation passes.
The response always includes both `uploaded` (successful) and `failed` arrays.

**Response:**

- `201 Created` - At least one photo was uploaded successfully
- `200 OK` - No photos were uploaded (all failed or empty request)

```json
{
  "uploaded": [
    {
      "client_id": "/Users/photos/IMG_001.jpg",
      "photo_id": "f0e1d2c3-b4a5-6789-0fed-cba987654321",
      "filename": "IMG_001.jpg",
      "size_bytes": 4200000
    },
    {
      "client_id": "/Users/photos/IMG_002.jpg",
      "photo_id": "a9b8c7d6-e5f4-3210-9876-543210fedcba",
      "filename": "IMG_002.jpg",
      "size_bytes": 3800000
    }
  ],
  "failed": [
    {
      "client_id": "/Users/photos/IMG_003.jpg",
      "filename": "IMG_003.jpg",
      "reason": "file_too_large"
    }
  ],
  "uploaded_size_bytes": 8000000
}
```

**Failure Reasons:**

- `file_too_large` - File exceeds max_photo_size
- `invalid_format` - Unsupported file format
- `too_many_files` - Uploaded files count exceeds max_photos_per_request
- `storage_full` - Insufficient storage space

**Errors:**

- `404` - Task not found

### GET /api/tasks/{task_id}/photos

Returns list of photo IDs in the task.

**Response:**

```json
{
  "photo_ids": [
    "f0e1d2c3-b4a5-6789-0fed-cba987654321",
    "a9b8c7d6-e5f4-3210-9876-543210fedcba",
    "11111111-2222-3333-4444-555555555555",
    "66666666-7777-8888-9999-aaaaaaaaaaaa",
    "bbbbbbbb-cccc-dddd-eeee-ffffffffffff"
  ],
  "count": 5
}
```

### GET /api/photos/{photo_id}

Returns detailed information about a specific photo.

**Response:**

```json
{
  "photo_id": "f0e1d2c3-b4a5-6789-0fed-cba987654321",
  "task_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "filename": "IMG_1234.jpg",
  "size_bytes": 4200000,
  "uploaded_at": "2024-01-15T10:32:00Z"
}
```

### DELETE /api/photos/{photo_id}

Deletes a specific photo.

**Response:**

- `204 No Content` - Photo successfully deleted

**Errors:**

- `404` - Photo not found
- `409` - Cannot delete photo referenced by active jobs (future implementation)

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
  "job_id": "12345678-abcd-ef01-2345-6789abcdef01",
  "task_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
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
  "job_id": "fedcba98-7654-3210-fedc-ba9876543210",
  "parent_job_id": "12345678-abcd-ef01-2345-6789abcdef01",
  "task_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
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
      "job_id": "12345678-abcd-ef01-2345-6789abcdef01",
      "task_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
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
  "job_id": "12345678-abcd-ef01-2345-6789abcdef01",
  "task_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "status": "processing",
  "model": "qwen2-vl:8b",
  "photo_count": 15,
  "progress": {
    "completed": 7,
    "failed": 1,
    "remaining": 7,
    "current_photo_id": "11111111-2222-3333-4444-555555555555"
  },
  "created_at": "2024-01-15T10:35:00Z"
}
```

### GET /api/jobs/{job_id}/results

Returns analysis results. Available even for jobs in "processing" or "cancelled" state (partial results).

**Response:**

```json
{
  "job_id": "12345678-abcd-ef01-2345-6789abcdef01",
  "task_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "status": "completed",
  "model": "qwen2-vl:8b",
  "results": [
    {
      "photo_id": "f0e1d2c3-b4a5-6789-0fed-cba987654321",
      "status": "completed",
      "tags": "golden gate bridge, sunset, long exposure, red suspension cables",
      "processed_at": "2024-01-15T10:36:00Z"
    },
    {
      "photo_id": "a9b8c7d6-e5f4-3210-9876-543210fedcba",
      "status": "failed",
      "error": "ollama timeout",
      "tags": null
    },
    {
      "photo_id": "11111111-2222-3333-4444-555555555555",
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
  "job_id": "12345678-abcd-ef01-2345-6789abcdef01",
  "total_photos": 15
}
```

**progress:**

```json
{
  "event": "progress",
  "photo_id": "f0e1d2c3-b4a5-6789-0fed-cba987654321",
  "status": "completed",
  "progress": "1/15"
}
```

**progress (failed photo):**

```json
{
  "event": "progress",
  "photo_id": "a9b8c7d6-e5f4-3210-9876-543210fedcba",
  "status": "failed",
  "error": "ollama timeout",
  "progress": "2/15"
}
```

**completed:**

```json
{
  "event": "completed",
  "job_id": "12345678-abcd-ef01-2345-6789abcdef01",
  "total": 15,
  "succeeded": 13,
  "failed": 2
}
```

**cancelled:**

```json
{
  "event": "cancelled",
  "job_id": "12345678-abcd-ef01-2345-6789abcdef01"
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
  "job_id": "12345678-abcd-ef01-2345-6789abcdef01",
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
  "size_bytes": "number",
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
- `file_too_large` (400) - Uploaded file exceeds max_photo_size
- `too_many_files` (400) - Upload exceeds max_photos_per_request
- `insufficient_storage` (507) - Storage quota exceeded
- `resource_in_use` (409) - Cannot delete resource referenced by active jobs
- `no_failed_photos` (400) - Retry requested but no failed photos
- `job_still_processing` (409) - Operation not allowed on active job

## See Also

- [Architecture](architecture.md) - System design and core concepts
- [Configuration](configuration.md) - Server configuration reference
- [Development Guide](development.md) - Testing and workflow
