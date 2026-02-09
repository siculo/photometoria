# Photometoria API Architecture

## Overview

The Photometoria API is built using a modern async-first architecture with Rust and Axum framework. The system is designed for scalability, maintainability, and future evolution through abstraction layers and modular structure.

**Key Design Principles:**

- Async-first design with Tokio runtime
- Worker pool pattern for GPU resource management (planned - Issue #8)
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

- **Filesystem-based storage** for all data (tasks, photos, jobs)
- **JSON files** for metadata persistence
- **Binary files** for photo storage
- **FileSystemLayout** for centralized directory structure management
- **Abstraction layer** designed for future evolution (database, object storage)

### Multi-Task Support

- Multiple tasks can coexist simultaneously in the system
- Each task maintains independent photo collections and job queues
- Task isolation ensures no cross-task interference through separate filesystem directories
- Current limitation: Filesystem storage is bound by available disk space
- Future enhancement: Configurable limits on task count, storage per-task, and TTL-based cleanup

### Concurrency Model

- **Task-based async processing** using Tokio tasks
- **Worker pool** with GPU-based limits (planned - one worker per GPU typically)
- **Queue-based execution** for job scheduling (planned)
- Current: Jobs are processed sequentially (concurrent execution not yet implemented)

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
- Multiple jobs can be queued (concurrent execution planned - Issue #8)

**States:**

- `queued` - Waiting for processing (worker pool not yet implemented)
- `processing` - Currently being executed
- `completed` - Finished successfully
- `failed` - Encountered fatal error
- `cancelled` - Manually stopped by user (not yet implemented)

**Results:**

- Available incrementally during processing (partial results)
- Remain available after completion until job is deleted
- Each photo in the job has individual status (completed/failed)

**Lifecycle:**

```
Created → Queued → Processing → Completed/Failed/Cancelled → Deleted
```

### Worker Pool (Planned - Issue #8)

The **Worker Pool** will manage concurrent job execution based on available GPU resources.

**Status:** Not yet implemented. This section describes the planned design.

**Planned Design:**

- One worker per GPU (configured in settings)
- Workers will pull jobs from a queue
- Jobs will wait in queue until a worker becomes available
- Each worker will process photos sequentially within a job

**Configuration (Ready):**

The configuration structure is already in place in `OllamaProviderConfig`:

```toml
[ai.providers.ollama]
base_url = "http://localhost:11434"
devices = [0, 1]     # GPU indices to use (ready for worker pool)
max_workers = 2      # Maximum concurrent jobs (ready for worker pool)
```

**Current Behavior:**

Jobs are processed sequentially without a worker pool. The `max_workers` and `devices` configuration is parsed but not yet used.

---

### Photo Selection Strategy (Planned)

When implementing the worker pool, a key architectural decision is how workers select the next photo to process. This is especially important when multiple jobs use different AI models.

#### The Challenge: Efficiency vs. Fairness

With multiple jobs using different models and limited GPUs, there's a fundamental trade-off:

**Sequential job execution:**
- ✅ Minimal model swaps (one load per job)
- ✅ Maximum efficiency (~6% overhead)
- ❌ Poor fairness (jobs wait in queue until others complete)

**Round-robin photo selection:**
- ✅ Perfect fairness (all jobs progress simultaneously)
- ❌ Excessive model swaps (one per photo if models differ)
- ❌ Severe overhead (~77% in worst case)

#### Proposed: Smart Priority-Based Selection

A hybrid strategy that balances efficiency and fairness:

**Algorithm:**

1. Worker tracks current loaded model and counter of photos processed with it
2. When selecting next photo:
   - If `counter < threshold`: Prioritize photos requiring the current model
   - If `counter >= threshold`: Accept any photo (allow model swap)
3. Reset counter when model changes

**Pseudo-code:**

```rust
fn select_next_photo(&mut self, threshold: usize) -> Option<Photo> {
    let current_model = self.loaded_model;
    let counter = self.model_counter;

    // Below threshold: prefer same model (avoid swap)
    if counter < threshold {
        if let Some(photo) = find_photo_with_model(current_model) {
            return Some(photo);
        }
    }

    // Above threshold: accept any photo (allow model swap for fairness)
    find_any_available_photo()
}
```

#### Performance Analysis

**Scenario:** 1 GPU, 2 jobs (50 photos each), different models (qwen2-vl, llava)

| Strategy | Time | Overhead | First Result Job B | Fairness |
|----------|------|----------|-------------------|----------|
| Sequential (threshold=∞) | 320s | 20s (6%) | t=170s | Poor |
| Smart (threshold=25) | 340s | 40s (12%) | t=95s | Good |
| Smart (threshold=10) | 420s | 100s (24%) | t=50s | Excellent |
| Round-robin (threshold=1) | 1300s | 1000s (77%) | t=23s | Perfect |

**Key Insights:**

- **Threshold=20-25**: Best balance for production use (~6-12% overhead, good fairness)
- **Threshold=50+**: Equivalent to sequential job execution (maximum efficiency, poor fairness)
- **Threshold=1-5**: Near round-robin behavior (poor efficiency, maximum fairness)

#### Advantages

**1. Configurable Trade-off:**
- Tune threshold based on workload characteristics
- High threshold for efficiency-critical workloads
- Low threshold for user-facing interactive scenarios

**2. Adaptive Behavior:**
- If all jobs use same model: zero overhead (never swaps)
- If one job finishes early: continues with remaining job without unnecessary swaps
- Automatically optimal for homogeneous workloads

**3. Better User Experience:**

```
Sequential execution:
  Job A: ████████████████ (completes, then Job B starts)
  Job B: ................ ████████████████

Smart priority (threshold=10):
  Job A: ███...███...███...███...███
  Job B: ...███...███...███...███...███

Both jobs show progress simultaneously via SSE updates!
```

**4. Model Locality:**
- Exploits Ollama's keep-alive feature (models stay in VRAM for 5 minutes by default)
- Processes multiple photos with same model before swapping
- Minimizes expensive model load operations

#### Implementation Considerations

**Queue Structure:**

```rust
struct PhotoQueue {
    // Photos organized by required model
    photos_by_model: HashMap<ModelId, VecDeque<PhotoId>>,

    // All pending photos (for threshold overflow)
    all_photos: VecDeque<PhotoId>,

    // Current model statistics
    current_model: ModelId,
    model_counter: usize,
    threshold: usize,
}
```

**Configuration:**

```toml
[worker_pool]
photo_selection_threshold = 20  # Photos before accepting model swap
```

**Metrics to Track:**
- Model swaps per job
- Time spent on model loading vs. processing
- Fairness metric (time to first result per job)

#### Recommendation

For initial implementation, use **threshold=20-25 photos**:
- Overhead remains acceptable (6-12%)
- All jobs begin processing within 1-2 minutes
- Good balance for typical photography workloads (batch sizes 20-200 photos)

The threshold can be exposed as a configuration option for users to tune based on their specific needs.

## Module Organization

The codebase follows a clean modular structure:

```
src/
├── main.rs              # Application entry point, server initialization
├── lib.rs               # Library exports for integration tests
├── config/              # Configuration loading and types
│   ├── mod.rs
│   └── byte_size.rs     # ByteSize parsing
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
│   ├── ai/              # AI provider abstraction layer
│   │   ├── mod.rs       # Module exports
│   │   ├── error.rs     # AIProviderError types
│   │   ├── provider.rs  # AIProvider trait and common types
│   │   ├── registry.rs  # ProviderRegistry for managing providers
│   │   └── ollama/      # Ollama provider implementation
│   │       ├── mod.rs
│   │       ├── provider.rs  # OllamaProvider
│   │       └── types.rs     # Ollama API types
│   └── (worker.rs)      # Worker pool (planned - Issue #8)
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
  - Supports vision models (llava, qwen2-vl, etc.)
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
- Directory structure: `{storage_path}/tasks/{task_id}/` with subdirectories for photos (`imgs/`) and jobs

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

### Worker Pool Implementation (Planned)

**Status:** Not yet implemented (Issue #8). This section describes the planned architecture.

**Planned Design:**

- Tokio task per worker
- Photo queue with priority-based selection (see "Photo Selection Strategy" in Worker Pool section)
- Semaphore pattern to limit concurrency based on available GPUs
- Model-aware scheduling to minimize VRAM swaps

**Two Implementation Approaches:**

**Approach 1: Job-Level (Simple)**

Workers pull complete jobs from queue:

```
Worker loop:
  1. Acquire semaphore permit (enforces max_workers limit)
  2. Pop job from queue
  3. For each photo in job:
     - Call AI provider API (via AIProvider trait)
     - Save result incrementally
     - Send SSE update
  4. Mark job as completed
  5. Release permit
```

*Pros:* Simple, minimal model swaps
*Cons:* Poor fairness with multiple jobs

**Approach 2: Photo-Level with Smart Selection (Recommended)**

Workers pull individual photos using priority-based selection:

```
Worker loop:
  1. Acquire semaphore permit (enforces max_workers limit)
  2. Load AI model if needed (check current_model)
  3. Loop:
     a. Select next photo (smart priority: prefer same model, threshold-based)
     b. If model changed: unload old, load new model
     c. Call AI provider API
     d. Save result incrementally
     e. Send SSE update
     f. Update model counter
     g. If no more photos: break
  4. Release permit
```

*Pros:* Excellent fairness, configurable efficiency
*Cons:* More complex, some model swap overhead (controllable via threshold)

See the **Photo Selection Strategy** section in Worker Pool for detailed analysis and threshold recommendations.

**Implementation Notes:**

When implementing the worker pool:
- Use the existing `AIProvider` trait for provider abstraction
- Leverage the `max_workers` and `devices` configuration from `OllamaProviderConfig`
- Implement the smart photo selection strategy with configurable threshold (recommended: 20-25)
- Track model swap metrics for monitoring and optimization
- Consider GPU device assignment strategies (round-robin, load-based, etc.)
- Implement graceful shutdown for worker tasks
- Handle model loading failures gracefully (retry, skip, fail job)

**Recommended:** Start with Approach 2 (photo-level with smart selection) as it provides better user experience and is more flexible for future enhancements.

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
