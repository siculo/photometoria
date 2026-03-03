# Photometoria API — AI Assistant Context

## API Workflow

```
1. GET  /api/info                          → Server info and capabilities
2. GET  /api/providers                     → List configured AI providers
3. GET  /api/providers/{provider_name}     → Provider details and models
4. GET  /api/models                        → List default provider models
5. POST /api/tasks                         → Create task (working session)
6. POST /api/tasks/{id}/photos             → Upload photos (multipart)
7. POST /api/tasks/{id}/jobs               → Start job (choose AI model)
8. [TODO #11] SSE streaming                → Monitor progress
9. POST /api/jobs/{id}/cancel              → Cancel job (optional)
10. GET /api/jobs/{id}/results             → Retrieve generated metadata
11. POST /api/jobs/{id}/retry              → Retry failed/unprocessed photos
12. DELETE /api/tasks/{id}                 → Cleanup
```

---

## Code Structure

```
api/src/
├── main.rs           # Entry point, CLI parsing
├── lib.rs            # Library crate root (re-exports)
├── cli.rs            # CLI argument definitions (clap)
├── startup.rs        # Server initialization and startup logic
├── app_state.rs      # Shared application state (AppState)
├── config/           # TOML configuration parsing
│   ├── mod.rs        #   Config struct, load_config()
│   ├── ai.rs         #   AIConfig, ProviderConfig, OllamaProviderConfig
│   ├── server.rs     #   ServerConfig (host, port)
│   ├── storage.rs    #   StorageConfig (paths, max_size)
│   ├── upload.rs     #   UploadConfig (max photo size, max per request)
│   ├── worker_pool.rs#   WorkerPoolConfig
│   └── byte_size.rs  #   ByteSize helper type
├── routes/           # Endpoint definitions (Router)
│   └── mod.rs        #   create_router() — all route mappings
├── handlers/         # Business logic (TEST LOGIC HERE)
│   ├── mod.rs
│   ├── tasks.rs      #   CRUD tasks
│   ├── photos.rs     #   get/delete/list photos
│   ├── upload_photos.rs # Multipart upload handling
│   ├── jobs.rs       #   CRUD jobs + cancel/retry
│   ├── providers.rs  #   Provider listing, model discovery
│   ├── info.rs       #   Server info endpoint
│   ├── app_error.rs  #   AppError → HTTP response mapping
│   └── test_utils.rs #   Test fixtures and helpers
├── models/           # Domain structs
│   ├── task.rs
│   ├── photo.rs
│   ├── job.rs
│   └── info.rs       #   ServerInfo response struct
├── services/         # External services
│   ├── ai/           #   AI provider abstraction
│   │   ├── mod.rs    #     Module root, re-exports
│   │   ├── provider.rs #   AIProvider trait + types
│   │   ├── registry.rs #   ProviderRegistry
│   │   ├── error.rs  #     AIProviderError
│   │   └── ollama/   #     Ollama implementation
│   │       ├── mod.rs
│   │       ├── provider.rs
│   │       └── types.rs
│   └── worker/       #   Job processing
│       ├── mod.rs
│       ├── pool.rs   #     WorkerPool
│       ├── processor.rs #  Photo analysis logic
│       ├── queue.rs  #     Job queue
│       └── worker.rs #     Individual worker
└── storage/          # Persistence layer (filesystem)
    ├── mod.rs        #   Store traits + re-exports
    ├── task_store.rs
    ├── photo_store.rs
    ├── job_store.rs
    ├── filesystem_task_store.rs
    ├── filesystem_photo_store.rs
    ├── filesystem_job_store.rs
    └── filesystem_layout.rs
```

---

## Implemented Guardrails

- Tasks/photos not deletable while any job is active
- Jobs deletable only if in terminal state
- Job transitions to `processing` BEFORE AI analysis starts
- Cancel removes pending photos from buffer, marks job `cancelled`

---

## AI Provider Architecture

The system uses a trait-based provider abstraction:

```
AIProvider trait  →  defines: name(), list_models(), analyze_image(), check_health()
       ↓
OllamaProvider   →  implements AIProvider for Ollama backend
       ↓
ProviderRegistry →  HashMap<String, Arc<dyn AIProvider>> + default_provider
       ↓
AppState         →  holds Arc<ProviderRegistry>
```

- Providers are configured via TOML (`[ai.providers.ollama]`)
- Each provider has named models with backend mappings (e.g., `qwen3-vl` → `qwen3-vl:8b`)
- `ProviderRegistry::from_config()` instantiates all providers at startup
- Auto-selects default if only one provider is configured
- `is_model_configured()` validates model IDs against static config (no network call)

---

## Rust Code Style

```rust
// AVOID: Excessive .clone()
fn process(data: MyStruct) { ... }
let result = process(data.clone());

// PREFER: References when possible
fn process(data: &MyStruct) { ... }
let result = process(&data);
```

**Evaluate whether to use references as parameters instead of cloning.**

### Preferred Example

```rust
/// Calculates the total size of photos in a task.
/// Returns None if the task contains no photos.
fn calculate_task_size(photos: &[Photo]) -> Option<u64> {
    if photos.is_empty() {
        return None;
    }

    let total = photos.iter()
        .map(|p| p.file_size)
        .sum();

    Some(total)
}
```

---

## Testing Guidelines (CRITICAL)

### WHERE to Write Tests

- **Handler tests** (`handlers/{module}.rs`) — Business logic, error handling, custom validations
- **Storage tests** — CRUD, edge cases, persistence
- **Model tests** — Constructors, state transitions, conversions
- **Service tests** — Business logic with mocked dependencies

### Smoke Tests ONLY

- **Router tests** (`routes/mod.rs`) — NO logic duplication

### WHAT NOT to Test

- Axum framework validations (UUID parsing, JSON deserialization)
- HTTP routing (Axum's responsibility)
- Standard serialization

### Test Utilities

- Use `handlers::test_utils::fixtures::create_test_state()` for consistent setup
- Avoid duplicating test infrastructure

### Naming Convention

```rust
test_<handler>_<scenario>
// Example: test_create_job_task_not_found
```

Full details: `api/docs/development.md` — Testing section

---

## Technology Stack

### Rust Crates

- axum (multipart), tokio (full), tower
- reqwest (json), serde (derive), serde_json
- anyhow (app errors), thiserror (domain errors)
- tracing + tracing-subscriber (env-filter)
- chrono (serde), uuid (v4, serde)
- async-trait, base64, infer, dashmap, toml
- Dev: tempfile, wiremock, http-body-util

### Error Handling Pattern

```rust
Result<T, anyhow::Error>     // App level
.context("descriptive message")?  // Add context

// Domain-specific
#[derive(Error, Debug)]
pub enum OllamaError { ... }
```

### Concurrency

- `Arc<RwLock<T>>` — Shared mutable state
- `Arc<T>` — Shared immutable
- Future cancellation — `tokio::select!` / `timeout()`

### Edition

**Rust Edition:** `2024` (stable since Rust 1.85.0, currently using 1.92.0)

---

## AI Models (Ollama)

- **qwen3-vl:8b** — PRODUCTION (best quality, slower)
- **llava** — DEVELOPMENT (faster, good for testing)

---

## Development Commands

```bash
# Build & run
cd api
cargo build --release
cargo run --release

# Quality checks (always before commit!)
cargo fmt && cargo clippy && cargo test

# Quick API test
curl -X POST http://localhost:3000/api/tasks \
  -H "Content-Type: application/json" \
  -d '{"context":"test photos"}'

# Server info
curl http://localhost:3000/api/info

# List providers
curl http://localhost:3000/api/providers
```

**Default server:** `http://localhost:3000`
