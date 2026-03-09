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
