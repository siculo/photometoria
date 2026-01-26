# Photometoria API

## Overview

This is the **REST API server** component of Photometoria, implemented in Rust using the Axum framework. It provides HTTP endpoints for managing photo analysis tasks, uploading images, and orchestrating AI-powered metadata generation through local Ollama models.

**Key Responsibilities:**

- **Task Management**: Create and manage working sessions for photo collections
- **Photo Storage**: Handle image uploads with configurable storage quotas
- **AI Processing**: Coordinate concurrent job execution across multiple GPUs
- **Real-time Updates**: Stream job progress via Server-Sent Events (SSE)
- **Model Abstraction**: Support multiple AI models with optimized prompts

**Architecture Highlights:**

- Async-first design with Tokio runtime
- Worker pool pattern for GPU resource management
- In-memory storage with abstraction layers for future database integration
- Modular structure: routes, handlers, services, models, storage

## Development Environment

### Requirements by Component

#### Ollama & AI Models

**Hardware:**

- GPU with sufficient memory for the AI models you intend to use
- Currently tested models: **qwen2-vl:8b** (higher quality, requires more VRAM) and **llava** (faster, lower requirements)
- Multi-GPU setup supported for concurrent processing

**Supported Systems:**

- **Linux** - GPU support via appropriate drivers (CUDA for NVIDIA, ROCm for AMD)
- **Windows** - Native GPU support or via WSL2
- **Mac** - Metal support (Apple Silicon recommended for better inference performance)

#### API Server Development

**Requirements:**

- System capable of compiling and running Rust applications
- Modern development environment with sufficient resources for async runtime

**Supported Systems:**

- **Linux** - Primary development and production platform
- **Windows** - Full support
- **Mac** - Full support

#### Lightroom Plugin Development

**Requirements:**

- Adobe Lightroom Classic installed
- Lua development environment

**Supported Systems:**

- **Mac** - Fully supported
- **Windows** - Fully supported

### Software Stack

**Core Technologies:**

- **Rust** - REST API server implementation
- **Ollama** - Local AI model inference engine
- **Python** - Initial testing and prototyping
- **Git/GitHub** - Version control

**Development Tools:**

- Node.js - JavaScript runtime
- Rust toolchain - Compiler and package manager
- Adobe Lightroom software development kit (SDK) - Lightroom plugin development

### Recommended Development Tools

**Code Editors:**

- **VS Code** - Versatile editor with extensive plugin ecosystem
- **RustRover** - JetBrains IDE optimized for Rust development

**AI-Assisted Development:**

- **OpenCode** - AI-powered development assistant

**Lua Development:**

- **Lua Language Server** - Language support and IntelliSense
- **Lua Debug** - Debugging support for Lua scripts

**Rust Development:**

- **rust-analyzer** - IDE support for Rust with intelligent code completion
- **Cargo** - Build system and package manager (included in Rust toolchain)

**API Testing:**

- **curl** - Command-line HTTP client
- **Postman** - GUI-based API testing tool
- **Bruno** - Open-source API client

**Git Clients:**

- **Command-line git** - Standard git CLI
- **GUI options** - GitKraken, SourceTree, or other clients based on preference

## Core System Design

### Multi-Level Tagging Approach

The original design concept involves multiple analysis levels:

1. **Individual Analysis (Micro)**: Each photo analyzed separately for specific details
    - Example: "golden gate bridge, sunset, long exposure, red suspension cables"

2. **Group Analysis (Macro)**: Photos analyzed together for broader context
    - Example: 20 photos → "san francisco vacation, summer 2024, california coast"

3. **User Context Hints**: Manually provided information
    - Example: "trip to northern california"

4. **EXIF Metadata**: Technical information extracted from camera data

**Current Implementation:**
The initial version simplifies this to a single-level tagging system that produces one set of tags per photo, while
still considering:

- Individual photo content
- User-provided context hints
- Other photos in the same processing batch

### Model Selection & Testing

**Key Findings:**

- **qwen2-vl:8b**: Superior quality for landmark identification and detailed tagging, but slower
- **llava**: Faster iteration for development work, acceptable quality

**Technical Approach:**

- Use Ollama's HTTP API directly (more reliable than subprocess calls)
- Different models require optimized prompts for clean comma-separated tag output
- Always test on real photo collections before production use

## REST API Server (Rust)

### Architecture Overview

**Framework & Runtime:**

- **Axum** - Modern async web framework
- **Tokio** - Async runtime
- **SSE (Server-Sent Events)** - Real-time updates to clients

**Storage Strategy:**

- In-memory data structures for jobs and metadata
- Filesystem storage for uploaded photos
- Abstraction layer designed for future evolution (database, object storage)

**Multi-Task Support:**

- Multiple tasks can coexist simultaneously in the system
- Each task maintains independent photo collections and job queues
- Task isolation ensures no cross-task interference
- Current limitation: In-memory storage is bound only by available RAM
- Future enhancement: Configurable limits on task count, storage per-task, and TTL-based cleanup

**Concurrency Model:**

- Task-based async processing (Tokio tasks)
- Worker pool with GPU-based limits
- Multiple jobs can run concurrently (up to GPU capacity)

### Core Concepts

#### Task

A **Task** represents a working session for a photographer.

**Characteristics:**

- Container for uploaded photos and shared context hints
- Short-lived (one working session), but no automatic timeout initially
- Photos remain available until task is explicitly deleted
- Context can be modified after creation

**Task Limits:**

- The current implementation does not enforce limits on the number of tasks
- Future versions may introduce configurable limits based on:
  - Total task count (system-wide or per-user)
  - Storage quota enforcement
  - Time-based cleanup policies (TTL)
- The architecture supports adding these constraints without major refactoring

**Lifecycle:**

```
Created → Photos Uploaded → Jobs Created/Executed → Explicitly Deleted
```

#### Photo

A **Photo** is an image file uploaded for analysis.

**Characteristics:**

- Belongs to exactly one task
- Stored on filesystem (with configurable storage quota)
- Identified by unique photo_id
- Contains metadata: original filename, size, upload timestamp

**Constraints:**

- Cannot be deleted if referenced by any active job
- Deleted automatically when parent task is deleted

#### Job

A **Job** is an AI analysis process that runs on photos within a task.

**Characteristics:**

- References a specific task
- Specifies which AI model to use
- Can process all photos in the task or a specific subset
- Works on a snapshot of available photos at creation time
- Multiple jobs can be active concurrently (GPU limit permitting)

**States:**

- `queued` - Waiting for available worker
- `processing` - Currently being executed
- `completed` - Finished successfully
- `failed` - Encountered fatal error
- `cancelled` - Manually stopped by user

**Results:**

- Available incrementally during processing (partial results)
- Remain available after completion until job is deleted
- Each photo in the job has individual status (completed/failed)

**Lifecycle:**

```
Created → Queued → Processing → Completed/Failed/Cancelled → Deleted
```

#### Worker Pool

The **Worker Pool** manages concurrent job execution based on available GPU resources.

**Design:**

- One worker per GPU (configured in settings)
- Workers pull jobs from a queue
- Jobs wait in queue until a worker becomes available
- Each worker processes photos sequentially within a job

**Configuration:**

```toml
[gpu]
devices = [0, 1]  # GPU indices to use
max_workers = 2   # Maximum concurrent jobs
```

### Data Flow

**Complete Workflow:**

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

### API Endpoints

#### System Endpoints

**GET /api/config**

Returns server configuration and limits relevant to the client.

Response:

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

**GET /api/models**

Returns list of supported AI models that are currently available (both configured and installed in Ollama).

Response:

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

#### Task Endpoints

**POST /api/tasks**

Creates a new task. Multiple tasks can be active simultaneously.

**Note:** The current implementation does not enforce task count limits. Future versions may introduce configurable quotas.

Request:

```json
{
  "context": "vacation in San Francisco, summer 2024"
}
```

Response:

```json
{
  "task_id": "task_abc",
  "context": "vacation in San Francisco, summer 2024",
  "created_at": "2024-01-15T10:30:00Z"
}
```

**GET /api/tasks**

Returns list of all tasks.

Response:

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

**GET /api/tasks/{task_id}**

Returns detailed information about a specific task, including all associated jobs.

Response:

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

**PATCH /api/tasks/{task_id}**

Updates the task context.

Request:

```json
{
  "context": "updated context information"
}
```

Response:

```json
{
  "task_id": "task_abc",
  "context": "updated context information"
}
```

**DELETE /api/tasks/{task_id}**

Deletes a task and all associated resources (photos, jobs).

Errors:

- `409` - Cannot delete task with active jobs

#### Photo Endpoints

**POST /api/tasks/{task_id}/photos**

Uploads one or more photos to a task using multipart/form-data.

Request:

- Content-Type: multipart/form-data
- Field name: "files" (can be repeated for multiple files)
- Limits: max_photos_per_request, max_photo_size_mb (from config)

Response:

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

Errors:

- `400` - File too large or too many files
- `404` - Task not found
- `507` - Insufficient storage space

**GET /api/tasks/{task_id}/photos**

Returns list of photo IDs in the task.

Response:

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

**GET /api/photos/{photo_id}**

Returns detailed information about a specific photo.

Response:

```json
{
  "photo_id": "p1",
  "task_id": "task_abc",
  "filename": "IMG_1234.jpg",
  "size_mb": 4.2,
  "uploaded_at": "2024-01-15T10:32:00Z"
}
```

**DELETE /api/tasks/{task_id}/photos/{photo_id}**

Deletes a specific photo from the task.

Errors:

- `409` - Cannot delete photo referenced by active jobs

#### Job Endpoints

**POST /api/tasks/{task_id}/jobs**

Creates and starts a new analysis job.

Request:

```json
{
  "model": "qwen2-vl:8b",
  "photo_ids": null
  // null = all photos, or array of specific IDs
}
```

Response:

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

Errors:

- `400` - Invalid model or photo_ids
- `404` - Task not found

**POST /api/jobs/{job_id}/retry**

Retries only the failed photos from a completed job, using the same model and enriched context from successfully
processed photos.

Response:

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

Errors:

- `400` - No failed photos to retry
- `409` - Original job still processing

**GET /api/jobs**

Returns list of all jobs across all tasks.

Response:

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

**GET /api/jobs/{job_id}**

Returns current state of a specific job.

Response:

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

**GET /api/jobs/{job_id}/results**

Returns analysis results. Available even for jobs in "processing" or "cancelled" state (partial results).

Response:

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

**GET /api/jobs/{job_id}/stream**

Opens a Server-Sent Events (SSE) stream for real-time job updates.

Event Types:

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
When a client disconnects from the SSE stream, the server detects this and marks the job as "abandoned". Currently, no
automatic timeout/cleanup is implemented for abandoned jobs (future enhancement).

**DELETE /api/jobs/{job_id}**

Cancels and deletes a job.

Behavior:

- If job is running: completes current photo processing, then stops
- Partial results are preserved and retrievable
- Associated photos remain in the task

Response:

```json
{
  "job_id": "job_xyz",
  "status": "cancelled"
}
```

### Data Models

#### Task

```json
{
  "task_id": "string (UUID)",
  "context": "string",
  "created_at": "ISO 8601 timestamp"
}
```

#### Photo

```json
{
  "photo_id": "string (UUID)",
  "task_id": "string (UUID)",
  "filename": "string",
  "size_mb": "number",
  "uploaded_at": "ISO 8601 timestamp"
}
```

#### Job

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

#### Result

```json
{
  "photo_id": "string (UUID)",
  "status": "completed|failed",
  "tags": "string (comma-separated) | null",
  "error": "string | null",
  "processed_at": "ISO 8601 timestamp | null"
}
```

### Error Handling

All errors follow a consistent JSON format:

```json
{
  "error": "error_code",
  "message": "Human-readable error description"
}
```

**Common Error Codes:**

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

### Configuration

The server reads configuration from a TOML file at startup.

**Example Configuration:**

```toml
[server]
host = "0.0.0.0"
port = 8080

[gpu]
devices = [0, 1]      # GPU indices to use
max_workers = 2        # Maximum concurrent jobs

[storage]
path = "/var/photometoria/storage"
max_size_gb = 100

[upload]
max_photos_per_request = 50
max_photo_size_mb = 20

[ollama]
base_url = "http://localhost:11434"

[[models]]
name = "qwen2-vl:8b"
ollama_model = "qwen2-vl:8b"
prompt_template = "Analyze this photo and provide comma-separated tags. Context: {context}"
description = "Best quality, slower processing"

[[models]]
name = "llava"
ollama_model = "llava:latest"
prompt_template = "List tags for this image, comma-separated. Context: {context}"
description = "Faster, good for testing"
```

**Configuration Sections:**

**[server]**

- `host` - Bind address
- `port` - Server port

**[gpu]**

- `devices` - Array of GPU device indices to use
- `max_workers` - Maximum concurrent job executions (typically 1 per GPU)

**[storage]**

- `path` - Filesystem path for photo storage
- `max_size_gb` - Total storage quota

**[upload]**

- `max_photos_per_request` - Limit for batch uploads
- `max_photo_size_mb` - Maximum size per individual photo

**[ollama]**

- `base_url` - Ollama API endpoint

**[[models]]** (array of supported models)

- `name` - Model identifier used in API requests
- `ollama_model` - Actual Ollama model name
- `prompt_template` - Prompt template for this model (supports {context} placeholder)
- `description` - Human-readable description

### Supported Models System

The server maintains a list of supported models with their corresponding prompt templates. At startup:

1. Server reads model definitions from configuration
2. Queries Ollama to check which models are actually installed
3. Only models that are both configured and installed appear in `GET /api/models`

**Benefits:**

- Centralized prompt optimization per model
- Validation at job creation (400 error if model unavailable)
- Easy to add new models without code changes
- Clear separation between supported vs. available models

### Implementation Strategy

#### Abstraction Layers

The implementation uses abstraction to allow future evolution without major refactoring:

**TaskStore**

- Interface: Trait-based abstraction (`TaskStore` trait)
- Current: `InMemoryTaskStore` using `DashMap` for concurrent access
- Future: Database-backed (PostgreSQL, SQLite), Redis cache, or hybrid approaches

**JobStore**

- Current: In-memory `HashMap` with `RwLock`
- Future: PostgreSQL, SQLite, or other database

**PhotoStore**

- Current: Filesystem with metadata in memory
- Future: Object storage (S3, MinIO), database-backed metadata

**TaskQueue**

- Current: In-memory `VecDeque` with `Mutex`
- Future: Redis, RabbitMQ, or other message queue

**NotificationManager**

- Current: SSE with in-memory connection tracking
- Future: WebSocket, or external pub/sub system

#### TaskStore Abstraction

The task storage layer uses a trait-based abstraction pattern to enable future evolution.

**Design Pattern:**

- `TaskStore` trait defines the storage interface (create, get, list, update, delete, exists, count)
- Trait-based design allows multiple implementations without changing business logic
- All methods are async and return `Result<T, TaskStoreError>` for proper error handling
- Thread-safe operations (`Send + Sync` bounds) for concurrent access from multiple Tokio tasks

**Current Implementation: InMemoryTaskStore**

- Uses `DashMap` for thread-safe concurrent access (lock-free reads)
- O(1) lookup performance by task_id
- Data persists only for the lifetime of the server process
- No built-in limits on task count (memory-bound only)
- Suitable for development and single-user scenarios

**Future Implementations:**

- **Database-backed**: PostgreSQL or SQLite for persistence across restarts
- **Redis**: Distributed cache with TTL support for automatic cleanup
- **Hybrid**: In-memory cache + database for read performance + persistence
- **Custom limits**: Quota enforcement, LRU eviction, per-user isolation

**Thread Safety:**

All TaskStore implementations must be `Send + Sync` and support concurrent access
from multiple Tokio tasks without data races. The trait design ensures this contract
is enforced at compile time.

#### Worker Pool Implementation

**Design:**

- Tokio task per worker
- Shared job queue (`Arc<Mutex<VecDeque<JobId>>>`)
- Workers pull jobs and process photos sequentially
- Semaphore pattern to limit concurrency

**Pseudo-flow:**

```
Worker loop:
  1. Acquire semaphore permit (enforces max_workers limit)
  2. Pop job from queue
  3. For each photo in job:
     - Call Ollama API
     - Save result incrementally
     - Send SSE update
  4. Mark job as completed
  5. Release permit
```

#### Photo Deduplication (Future)

**Design for future implementation:**

- Calculate SHA256 hash on upload
- Store photos by content hash (content-addressable storage)
- Task maintains references to photo hashes, not file copies
- Reference counting: delete photo file when no task references it

**API Impact:**

- Transparent to client (no API changes needed)
- Upload response still returns photo_id
- Internally: photo_id maps to content hash

**Benefits:**

- Significant storage savings for repeated uploads
- Faster uploads (skip if already stored)

**Implementation Note:**
Not implemented initially to keep first version simple.

## REST API Structure (Rust)

### Module Organization

```
src/
├── main.rs              # Application entry point, server initialization
├── config.rs            # Configuration loading and types
├── routes/              # REST endpoint definitions (routing)
│   ├── mod.rs
│   ├── tasks.rs         # Task-related routes
│   ├── photos.rs        # Photo upload/management routes
│   ├── jobs.rs          # Job execution routes
│   └── system.rs        # System info routes (/config, /models)
├── handlers/            # Business logic for each endpoint
│   ├── mod.rs
│   ├── tasks.rs
│   ├── photos.rs
│   ├── jobs.rs
│   └── system.rs
├── services/            # External integrations
│   ├── mod.rs
│   ├── ollama.rs        # Ollama HTTP API client
│   └── worker.rs        # Worker pool and job processing
├── storage/             # Abstraction layer for persistence
│   ├── mod.rs
│   ├── task_store.rs    # Task storage abstraction
│   ├── photo_store.rs   # Photo storage abstraction
│   └── job_store.rs     # Job storage abstraction
├── models/              # Data structures
│   ├── mod.rs
│   ├── task.rs          # Task entity and DTOs
│   ├── photo.rs         # Photo entity and DTOs
│   ├── job.rs           # Job entity and DTOs
│   └── error.rs         # Error types
└── sse/                 # Server-Sent Events implementation
    ├── mod.rs
    └── manager.rs       # SSE connection management
```

### Main Dependencies

- `axum` - Async web framework
- `tokio` - Async runtime
- `reqwest` - HTTP client for Ollama API calls
- `serde` / `serde_json` - JSON serialization
- `uuid` - Unique identifier generation
- `anyhow` / `thiserror` - Error handling
- `tracing` - Logging and tracing
- `tower` - Middleware
- `tower-http` - HTTP middleware (CORS, tracing, etc.)

## Development Workflow

### Prerequisites

**On Linux Development Machine:**

- Rust toolchain installed
- Ollama running with desired models pulled
- NVIDIA drivers and CUDA configured for GPUs

**On Mac (for future plugin development):**

- Adobe Lightroom Classic installed
- Lua development environment (planned)

### Initial Setup

1. Clone repository
2. Configure `config.toml` with appropriate paths and GPU settings
3. Pull required Ollama models: `ollama pull qwen2-vl:8b`
4. Build server: `cargo build --release`
5. Run server: `cargo run --release`

### Testing Strategy

**Unit Tests:**

- Storage abstractions (mock implementations)
- Model selection and validation logic
- Configuration parsing

**Integration Tests:**

- Full API endpoint tests with real server instance
- Mock Ollama responses for predictable testing

**Manual Testing:**

- Use curl, Postman, or custom client
- Test with real photos and Ollama models
- Validate SSE streaming behavior

### Testing the API

**Manual Testing with curl:**

All endpoints can be tested using curl. Below are examples for common workflows.

**1. Create a task:**
```bash
curl -X POST http://localhost:3000/api/tasks \
  -H "Content-Type: application/json" \
  -d '{"context":"vacation in San Francisco, summer 2024"}'
```

Response:
```json
{
  "task_id": "550e8400-e29b-41d4-a716-446655440000",
  "context": "vacation in San Francisco, summer 2024",
  "created_at": "2024-01-15T10:30:00Z"
}
```

**2. List all tasks:**
```bash
curl http://localhost:3000/api/tasks
```

**3. Get task details:**
```bash
curl http://localhost:3000/api/tasks/550e8400-e29b-41d4-a716-446655440000
```

**4. Update task context:**
```bash
curl -X PATCH http://localhost:3000/api/tasks/550e8400-e29b-41d4-a716-446655440000 \
  -H "Content-Type: application/json" \
  -d '{"context":"updated context information"}'
```

**5. Delete a task:**
```bash
curl -X DELETE http://localhost:3000/api/tasks/550e8400-e29b-41d4-a716-446655440000
```

**6. Create multiple tasks (to test multi-task support):**
```bash
# Create first task
curl -X POST http://localhost:3000/api/tasks \
  -H "Content-Type: application/json" \
  -d '{"context":"San Francisco trip"}'

# Create second task (should succeed, not return 409)
curl -X POST http://localhost:3000/api/tasks \
  -H "Content-Type: application/json" \
  -d '{"context":"New York vacation"}'

# List both tasks
curl http://localhost:3000/api/tasks
```

**Expected behavior:**
- Multiple tasks should be created without conflicts
- Each task gets a unique UUID
- All tasks appear in the list endpoint

### Development with Claude Code

Claude Code is used for AI-assisted development, particularly for:

- Code structure and architecture decisions
- Implementation of complex async patterns
- Error handling strategies
- Documentation generation

## Key Learnings

### Model Performance

- **qwen2-vl:8b** produces superior results but slower (use for production)
- **llava** good for rapid iteration during development
- Different models require different prompt engineering for optimal output

### API Design

- Direct HTTP API calls to Ollama more reliable than subprocess management
- SSE provides simple, effective real-time updates without WebSocket complexity
- Separating Task and Job concepts enables flexible workflows and retry logic

### Rust Development

- Axum + Tokio provides excellent async web framework
- Abstraction layers essential for future-proofing
- In-memory implementations good starting point before adding complexity

### Version Control

- Git SSH authentication works well across machines
- GitKraken simplifies complex repository operations
- Comprehensive CLAUDE.md documentation captures full project state

## Future Roadmap

### Short-term (Current Focus)

- Complete REST API server implementation
- Thorough testing with real photo collections
- Performance optimization for batch processing

### Medium-term

- **Lightroom Lua Plugin**: Direct integration with Adobe Lightroom Classic
    - Photo upload from Lightroom catalog
    - Metadata write-back to Lightroom
    - UI for job monitoring and retry

- **Multi-user Support**: Add authentication and resource isolation
    - User authentication and authorization
    - Per-user task limits and storage quotas
    - Isolated task/job workspaces
    - Role-based access control (RBAC)

### Long-term

- **Enhanced Storage**:
    - Photo deduplication (content-addressable storage)
    - Database-backed metadata
    - Object storage integration (S3, MinIO)

- **Advanced Features**:
    - Job timeout and automatic cleanup
    - Task templates and presets
    - Batch operations on multiple tasks
    - Export/import of results

- **Performance**:
    - Rust-based image preprocessing (resize, format conversion)
    - GPU pooling optimization
    - Caching layer for repeated analyses

- **Alternative Implementations**:
    - Potential Python version for easier community contributions
    - Desktop application (non-server mode) for single-user scenarios

## Version History

- **v0.1.0** (Planned) - Initial REST API server implementation with core functionality

---

*Last Updated: January 2026*
