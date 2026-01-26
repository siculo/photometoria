# Photometoria API Architecture

## Overview

The Photometoria API is built using a modern async-first architecture with Rust and Axum framework. The system is designed for scalability, maintainability, and future evolution through abstraction layers and modular structure.

**Key Design Principles:**

- Async-first design with Tokio runtime
- Worker pool pattern for GPU resource management
- In-memory storage with abstraction layers for future database integration
- Modular structure: routes, handlers, services, models, storage
- Trait-based abstractions for future-proofing

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

The initial version simplifies this to a single-level tagging system that produces one set of tags per photo, while still considering:

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

## Architecture Components

### Framework & Runtime

- **Axum** - Modern async web framework built on top of hyper and tower
- **Tokio** - Async runtime for handling concurrent requests
- **SSE (Server-Sent Events)** - Real-time updates to clients without WebSocket complexity

### Storage Strategy

- **In-memory data structures** for jobs and metadata (fast access)
- **Filesystem storage** for uploaded photos
- **Abstraction layer** designed for future evolution (database, object storage)

### Multi-Task Support

- Multiple tasks can coexist simultaneously in the system
- Each task maintains independent photo collections and job queues
- Task isolation ensures no cross-task interference
- Current limitation: In-memory storage is bound only by available RAM
- Future enhancement: Configurable limits on task count, storage per-task, and TTL-based cleanup

### Concurrency Model

- **Task-based async processing** using Tokio tasks
- **Worker pool** with GPU-based limits (one worker per GPU typically)
- **Multiple jobs** can run concurrently (up to GPU capacity)
- **Queue-based execution** for job scheduling

## Core Concepts

### Task

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

### Photo

A **Photo** is an image file uploaded for analysis.

**Characteristics:**

- Belongs to exactly one task
- Stored on filesystem (with configurable storage quota)
- Identified by unique photo_id (UUID)
- Contains metadata: original filename, size, upload timestamp

**Constraints:**

- Cannot be deleted if referenced by any active job
- Deleted automatically when parent task is deleted

### Job

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

### Worker Pool

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

## Module Organization

The codebase follows a clean modular structure:

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

## Implementation Strategy

### Abstraction Layers

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

### TaskStore Abstraction

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

All TaskStore implementations must be `Send + Sync` and support concurrent access from multiple Tokio tasks without data races. The trait design ensures this contract is enforced at compile time.

### Worker Pool Implementation

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

### Photo Deduplication (Future)

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

## See Also

- [API Reference](api-reference.md) - Complete endpoint documentation
- [Configuration](configuration.md) - Server configuration reference
- [Development Guide](development.md) - Development workflow and testing
