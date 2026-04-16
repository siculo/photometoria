---
name: api-dev
description: Use this skill when working on the Rust API (`api/` directory).
---

# Photometoria API — Development Guidelines

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

## Defensive Programming

### When to add a sanity check

A check is required when a wrong value produces a **silent error** — one that generates neither a `Result::Err` nor a panic, but a wrong result that propagates through the system. The at-risk categories in Rust are:

| Situation | Risk | Solution |
|-----------|------|----------|
| `as` cast on non-constant value | Silent truncation | `TryFrom`/`TryInto` |
| `usize` arithmetic in `--release` | Silent underflow/overflow (wrapping) | `checked_sub`, `checked_mul` |
| Unvalidated state transition | Invariant violated without error | `debug_assert!` or explicit guard |
| `unwrap()` outside `#[cfg(test)]` | Panic in production | `?` with `.context()` |
| Multiple `Uuid` as positional parameters | Accidental swap not detected | Newtype wrapper |

> In `--debug`, arithmetic overflow panics; in `--release` it wraps silently.
> The server runs in `--release`: all arithmetic on non-constant values is at risk.

---

### Rule 1 — `TryFrom`/`TryInto` instead of `as` on non-constant values

```rust
// AVOID: as in routes/mod.rs:24
// u64 → usize followed by multiplication: silent overflow in release
let max_upload_size = state.config.upload.max_photo_size.0 as usize
    * state.config.upload.max_photos_per_request
    + 1024 * 1024;

// PREFER
let max_photo_size = usize::try_from(state.config.upload.max_photo_size.0)
    .context("max_photo_size exceeds usize")?;
let max_upload_size = max_photo_size
    .checked_mul(state.config.upload.max_photos_per_request)
    .context("max upload size overflow")?
    + 1024 * 1024;
```

**Exception:** `usize → u64` is always safe (on 64-bit, `usize == u64`).
The pattern `data.len() as u64` in `upload_photos.rs:218` is acceptable.

---

### Rule 2 — `checked_sub` / `checked_mul` for arithmetic on derived values

```rust
// AVOID: as in models/job.rs:264
// if a bug causes completed + failed > photo_ids.len(),
// usize subtraction wraps to usize::MAX in release
let remaining = self.photo_ids.len() - (completed + failed);

// PREFER
let processed = completed + failed;
let remaining = self.photo_ids.len()
    .checked_sub(processed)
    .unwrap_or(0); // or: .context("processed count exceeds total")?
```

---

### Rule 3 — `debug_assert!` for state invariants

The transition methods in `models/job.rs` (`start`, `complete`, `fail`, `cancel`)
do not validate the current state — any transition is accepted silently.

```rust
// PREFER: debug_assert! for invariants known to be true at call sites
// but that should be verified during development and in tests

/// Marks the job as started (Processing).
pub fn start(&mut self) {
    debug_assert_eq!(
        self.status, JobStatus::Queued,
        "start() called on job with status {:?}", self.status
    );
    self.status = JobStatus::Processing;
    self.started_at = Some(Utc::now());
}

/// Marks the job as completed.
pub fn complete(&mut self) {
    debug_assert_eq!(
        self.status, JobStatus::Processing,
        "complete() called on job with status {:?}", self.status
    );
    self.status = JobStatus::Completed;
    self.completed_at = Some(Utc::now());
}
```

`debug_assert!` is zero-cost in `--release` and panics in `--debug` and in tests,
making state machine bugs visible during development without penalising production.

---

### Rule 4 — No `unwrap()` outside `#[cfg(test)]`

```rust
// AVOID: as in config/storage.rs:27 and config/upload.rs:30
// unwrap() in a Default impl is not test code —
// if the hardcoded string becomes invalid, the server fails to start
impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            max_size: "10GiB".parse().unwrap(), // ← panics in production
        }
    }
}

// PREFER: a typed, infallible constructor
impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            max_size: ByteSize::gibibytes(10),
        }
    }
}
```

---

### Rule 5 — Newtype for domain IDs (prevent positional mixing)

Functions in `storage/filesystem_layout.rs` accept multiple positional `Uuid` arguments:

```rust
// FRAGILE: the compiler cannot detect argument swaps
pub fn job_file_path(&self, catalog_id: Uuid, task_id: Uuid, job_id: Uuid) -> PathBuf

// ROBUST: compile-time error if arguments are swapped
pub fn job_file_path(&self, catalog_id: CatalogId, task_id: TaskId, job_id: JobId) -> PathBuf

// Newtype definition — zero runtime overhead
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JobId(pub Uuid);
```

---

## Testing Guidelines (CRITICAL)

### Test-First Strategy

When fixing bugs or adding behaviour, **write a failing test first** that
captures the expected behaviour, then implement the fix or feature to make
it pass. This ensures the change is verifiable and prevents regressions.

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

### New Fields Must Be Tested

When adding a field to a struct (model, response, DTO), always add or update
test assertions to verify the field is correctly populated — both at the unit
level (e.g., `From` conversion) and at the handler level (e.g., API response).
An untested field could silently contain a default/zero value.

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

## Documentation (CRITICAL)

When adding, modifying, or removing endpoints, models, or response structures,
**always update all affected documentation in the same change**:

- `api/docs/api-reference.md` — Endpoint reference and response examples
- `api/docs/Photometoria.postman_collection.json` — Postman collection
- `api/docs/development.md` — curl examples in the manual testing section
- `api/CLAUDE.md` — API workflow overview

Do not consider the work complete until documentation is in sync with code.

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
