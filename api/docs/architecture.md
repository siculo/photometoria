# Photometoria API Architecture

## Overview

The Photometoria API is built using a modern async-first architecture with Rust and Axum framework. The system is designed for scalability, maintainability, and future evolution through abstraction layers and modular structure.

**Key Design Principles:**

- Async-first design with Tokio runtime
- Worker pool pattern for GPU resource management
- Filesystem-based storage with abstraction layers for future database integration
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

### Model Selection & Testing

**Key Findings:**

- **qwen3.5**: Recommended model for production use — high quality, good balance of speed and accuracy
- **qwen3-vl**: Superior quality for landmark identification and detailed tagging, but slower
- **gemma3n:e4b**: Google Gemma 3n vision model, tested as alternative
- **ministral-3:latest**: Mistral vision model, tested as alternative
- **llava**: Faster iteration for development work, acceptable quality

**Technical Approach:**

- Use Ollama's HTTP API directly (more reliable than subprocess calls)
- Models return structured JSON (`{"tags": [{"tag": "..."}]}`); the processor validates and converts to comma-separated text
- Always test on real photo collections before production use

## Architecture Components

### Framework & Runtime

- **Axum** - Modern async web framework built on top of hyper and tower
- **Tokio** - Async runtime for handling concurrent requests
- **Polling-based monitoring** - Clients poll job results endpoint for progress updates

### Storage Strategy

- **Filesystem-based storage** for all data (tasks, photos, jobs)
- **JSON files** for metadata persistence
- **Binary files** for photo storage
- **FileSystemLayout** for centralized directory structure management
- **Abstraction layer** designed for future evolution (database, object storage)

### Catalog-Based Organization

- A **Catalog** corresponds to a Lightroom Classic catalog and serves as the top-level organizational unit
- Tasks and their photos are scoped to a specific catalog
- Catalog isolation ensures complete separation of data between different Lightroom catalogs

### Multi-Task Support

- Multiple tasks can coexist simultaneously within a catalog
- Each task maintains independent photo collections and job queues
- Task isolation ensures no cross-task interference through separate filesystem directories
- Current limitation: Filesystem storage is bound by available disk space
- Future enhancement: Configurable limits on task count, storage per-task, and TTL-based cleanup

### Concurrency Model

- **Task-based async processing** using Tokio tasks
- **Worker pool** with GPU-based limits (one worker per GPU)
- **Queue-based execution** for job scheduling via shared `PhotoBuffer`

## Core Concepts

### Catalog

A **Catalog** represents a Lightroom Classic catalog. It is the top-level container in the system.

**Characteristics:**

- Maps 1:1 to a Lightroom Classic catalog
- Identified by a unique catalog_id (UUID)
- Contains all tasks (and their photos/jobs) for that catalog
- Created implicitly when the first task is created for a catalog_id

**Purpose:**

- Ensures data isolation between different Lightroom catalogs
- Allows the same Photometoria server to serve multiple catalogs

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
- Multiple jobs can be queued (processed concurrently up to one per GPU)

**States:**

- `queued` - Waiting to be picked up by a worker
- `processing` - Currently being executed by a worker
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
- Workers pull individual photos from a shared `PhotoBuffer`
- Jobs transition from `queued` to `processing` as soon as a worker picks up the first photo, *before* the AI analysis call begins — so the status reflects work in progress during the entire analysis, not just after it completes
- Each worker processes photos sequentially

**Configuration:**

Configured via `OllamaProviderConfig`:

```toml
[ai.providers.ollama]
base_url = "http://localhost:11434"
devices = [0, 1]     # GPU indices — one worker is spawned per device
```

One worker is spawned per GPU listed in `devices`. If `devices` is empty, a single worker on device 0 is used.

---

## Module Organization

The codebase follows a clean modular structure:

```
src/
├── main.rs              # Application entry point
├── lib.rs               # Library exports for integration tests
├── cli.rs               # CLI argument definitions (clap)
├── startup.rs           # Server initialization and startup logic
├── app_state.rs         # Shared application state (AppState)
├── config/              # Configuration loading and types
│   ├── mod.rs           # Config struct, load_config()
│   ├── ai.rs            # AIConfig, ProviderConfig, OllamaProviderConfig
│   ├── server.rs        # ServerConfig (host, port)
│   ├── storage.rs       # StorageConfig (paths, max_size)
│   ├── upload.rs        # UploadConfig (max photo size, max per request)
│   ├── worker_pool.rs   # WorkerPoolConfig
│   └── byte_size.rs     # ByteSize helper type
├── routes/              # REST endpoint definitions (routing)
│   └── mod.rs           # create_router() — all route mappings
├── handlers/            # Business logic for each endpoint
│   ├── mod.rs
│   ├── tasks.rs         # CRUD tasks
│   ├── photos.rs        # Get/delete/list photos
│   ├── upload_photos.rs # Multipart upload handling
│   ├── jobs.rs          # CRUD jobs + cancel/retry/results
│   ├── providers.rs     # Provider listing, model discovery
│   ├── info.rs          # Server info endpoint
│   ├── app_error.rs     # AppError → HTTP response mapping
│   └── test_utils.rs    # Test fixtures and helpers
├── models/              # Data structures
│   ├── mod.rs
│   ├── catalog.rs       # Catalog entity
│   ├── task.rs          # Task entity and DTOs
│   ├── photo.rs         # Photo entity and DTOs
│   ├── job.rs           # Job entity and DTOs
│   └── info.rs          # ServerInfo response struct
├── services/            # External integrations
│   ├── mod.rs
│   ├── ai/              # AI provider abstraction layer
│   │   ├── mod.rs       # Module exports
│   │   ├── error.rs     # AIProviderError types
│   │   ├── provider.rs  # AIProvider trait and common types
│   │   ├── registry.rs  # ProviderRegistry for managing providers
│   │   └── ollama/      # Ollama provider implementation
│   │       ├── mod.rs
│   │       ├── provider.rs  # OllamaProvider
│   │       └── types.rs     # Ollama API types
│   └── worker/          # Worker pool implementation
│       ├── mod.rs
│       ├── pool.rs      # WorkerPool: discovery loop, startup recovery
│       ├── worker.rs    # Worker: hybrid threshold scheduling
│       ├── processor.rs # PhotoProcessor: AI call, job state update
│       └── queue.rs     # PhotoBuffer: shared photo queue
└── storage/             # Abstraction layer for persistence
    ├── mod.rs           # Store traits + re-exports
    ├── task_store.rs    # TaskStore trait
    ├── photo_store.rs   # PhotoStore trait
    ├── job_store.rs     # JobStore trait
    ├── filesystem_task_store.rs
    ├── filesystem_photo_store.rs
    ├── filesystem_job_store.rs
    └── filesystem_layout.rs
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
- `base64` - Image encoding for AI providers
- `async-trait` - Async trait support

### AI Provider Abstraction

The system uses a provider abstraction layer (`services/ai/`) to support multiple AI backends:

**Core Components:**

- **`AIProvider` trait** - Common interface for all AI providers
  - `check_health()` - Verify provider availability
  - `list_models()` - Get available models
  - `analyze_image()` - Perform image analysis

- **`ProviderRegistry`** - Manages provider instances
  - Stores providers by name
  - Provides default provider access
  - Created from configuration at startup

- **`OllamaProvider`** - Ollama implementation
  - Calls Ollama REST API (`/api/tags`, `/api/generate`)
  - Supports vision models (llava, qwen3-vl, etc.)
  - Configurable timeout and model mappings

**Design Benefits:**

- **Extensibility** - Add new providers without changing handlers
- **Testability** - Mock providers for unit tests (WireMock for integration)
- **Configuration-driven** - Provider selection via TOML
- **Future-proof** - Ready for OpenAI, Anthropic, and other providers

## Implementation Strategy

### Abstraction Layers

The implementation uses abstraction to allow future evolution without major refactoring:

**TaskStore**

- Interface: Trait-based abstraction (`TaskStore` trait)
- Current: `FileSystemTaskStore` with JSON-based persistence
- Future: Database-backed (PostgreSQL, SQLite), Redis cache, or hybrid approaches

**JobStore**

- Current: `FileSystemJobStore` with JSON files per job
- Future: PostgreSQL, SQLite, or other database

**PhotoStore**

- Current: `FileSystemPhotoStore` with binary photo data and JSON metadata
- Future: Object storage (S3, MinIO), database-backed metadata

**FileSystemLayout**

- Centralized directory structure management
- Consistent path generation across all storage implementations
- Directory structure: `{storage_path}/catalogs/{catalog_id}/tasks/{task_id}/` with subdirectories for photos (`imgs/`) and jobs

**TaskQueue**

- Current: In-memory `VecDeque` with `Mutex`
- Future: Redis, RabbitMQ, or other message queue

**NotificationManager**

- Current: SSE with in-memory connection tracking
- Future: WebSocket, or external pub/sub system

### Storage Abstraction

The storage layer uses trait-based abstraction patterns to enable future evolution.

**Design Pattern:**

- `TaskStore`, `PhotoStore`, and `JobStore` traits define storage interfaces
- Trait-based design allows multiple implementations without changing business logic
- All methods are async and return `Result<T, StoreError>` for proper error handling
- Thread-safe operations (`Send + Sync` bounds) for concurrent access from multiple Tokio tasks

**Current Implementation: Filesystem-based**

- **FileSystemTaskStore**: JSON file per task (`task.json`)
- **FileSystemPhotoStore**: Binary photo data in `imgs/` subdirectory, metadata in `photos.json`
- **FileSystemJobStore**: JSON file per job in `jobs/` subdirectory
- **FileSystemLayout**: Centralized path generation and directory structure management
- Data persists across server restarts
- Uses Tokio's async file I/O for non-blocking operations
- Concurrent access handled through async file locks
- Suitable for single-server deployments

**Directory Structure:**

```text
{storage_path}/
└── catalogs/
    └── {catalog_id}/
        ├── catalog.json           # Catalog metadata
        └── tasks/
            └── {task_id}/
                ├── task.json          # Task metadata
                ├── photos.json        # Photos metadata
                ├── imgs/              # Photo binary data
                │   ├── {photo_id_1}
                │   └── {photo_id_2}
                └── jobs/              # Job metadata
                    ├── {job_id_1}.json
                    └── {job_id_2}.json
```

**Future Implementations:**

- **Database-backed**: PostgreSQL or SQLite for better querying and indexing
- **Object Storage**: S3, MinIO, or similar for photo storage
- **Redis**: Distributed cache with TTL support for automatic cleanup
- **Hybrid**: Database metadata + object storage for photos
- **Custom limits**: Quota enforcement, LRU eviction, per-user isolation

**Thread Safety:**

All storage implementations must be `Send + Sync` and support concurrent access from multiple Tokio tasks without data races. The trait design ensures this contract is enforced at compile time.

### Worker Pool Implementation

**Design:**

- Tokio task per worker (one per GPU)
- Shared `PhotoBuffer` with priority-based photo selection
- Polling-based job discovery: the pool periodically polls the JobStore for new queued jobs
- Model-aware scheduling to minimize VRAM swaps
- Stale job recovery on startup (jobs in `processing` state are reset to `queued`)

**Worker Loop (Photo-Level with Hybrid Threshold):**

Workers pull individual photos from the shared `PhotoBuffer` using a smart hybrid selection strategy that balances efficiency (minimizing model swaps) with fairness (ensuring all jobs progress):

```
Worker loop:
  1. Load AI model if needed
  2. Initialize: photos_processed = 0, model_load_time = now()
  3. Loop:
     a. Select next photo from buffer:
        - Below thresholds (count OR time): prioritize photos with current model
        - Above both thresholds: accept any photo (allow model swap for fairness)
     b. If photo requires different model: load new model, reset counters
     c. Call AI provider API (via AIProvider trait)
     d. Save result incrementally
     e. Increment photos_processed
     f. If no more photos: break
```

For the detailed analysis of the hybrid threshold algorithm and its trade-offs, see [Photo Selection Strategy](photo-selection-strategy.md).

### Photo Deduplication (Future)

**Design for future implementation:**

- Use the photo's UUID to detect and prevent duplicate uploads within the same task
- On upload, check if the same photo (by original filename and size) already exists in the task
- If a duplicate is detected, return the existing photo_id instead of creating a new entry

**Benefits:**

- Prevents accidental duplicate uploads within a task
- Saves storage space without adding complexity

**Implementation Note:**

Not implemented initially to keep first version simple.

## See Also

- [API Reference](api-reference.md) - Complete endpoint documentation
- [Configuration](configuration.md) - Server configuration reference
- [Development Guide](development.md) - Development workflow and testing
- [Photo Selection Strategy](photo-selection-strategy.md) - Hybrid threshold algorithm for worker photo scheduling
