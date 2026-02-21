# Photometoria - AI Assistant Memory

## 🎯 Project Overview

Photometoria is an AI-powered metadata generation system for photography with Adobe Lightroom Classic integration. Multi-level tagging approach: individual photo analysis, photo grouping for macro-categories, user context hints, and EXIF extraction. Privacy-first with local AI (Ollama).

**Version:** 0.1.0 (Early Development)

---

## 🏗️ Architecture & API Workflow

### Components

| Component | Technology | Status |
|-----------|------------|--------|
| REST API | Rust/Axum | In development |
| Lightroom Plugin | Lua | Planned |
| Testing Scripts | Python 3.11+ | Functional |

### Core Concepts

- **Task**: Working session containing photos and context
- **Photo**: Uploaded image file with metadata
- **Job**: AI analysis process on a set of photos
- **Worker**: GPU-bound executor for job processing

### Complete API Workflow
```
1. POST /api/tasks → Create task (working session)
2. POST /api/tasks/{id}/photos → Upload photos
3. POST /api/tasks/{id}/jobs → Start job (choose AI model)
4. [TODO #11] SSE streaming → Monitor progress
5. POST /api/jobs/{id}/cancel → Cancel job (optional)
6. GET /api/jobs/{id}/results → Retrieve generated metadata
7. POST /api/jobs/{id}/retry → Retry failed/unprocessed photos
8. DELETE /api/tasks/{id} → Cleanup
```

### Implemented Guardrails

- Tasks/photos not deletable while any job is active
- Jobs deletable only if in terminal state
- Job transitions to `processing` BEFORE AI analysis starts
- Cancel removes pending photos from buffer, marks job `cancelled`

---

## 📂 Code Structure
```
api/src/
├── main.rs           # Entry point
├── routes/           # Endpoint definitions
├── handlers/         # Business logic (TEST LOGIC HERE)
├── services/         # External services (ollama, worker pool)
├── storage/          # Persistence layer
└── models/           # Domain structs
```

---

## ✨ Code Style & Preferences

### General Principles

- **Clarity > micro-optimizations** → Prefer understandable code even if slightly less efficient
- **Separate execution paths** → Clearly divide different logical branches (e.g., early returns, well-structured match)
- **Short functions/methods** → Extract functional units into separate methods for readability
- **DRY for repeated patterns** → Constantly repeated patterns → reusable library functions

### Rust-Specific
```rust
// ❌ AVOID: Excessive .clone()
fn process(data: MyStruct) { ... }
let result = process(data.clone());

// ✅ PREFER: References when possible
fn process(data: &MyStruct) { ... }
let result = process(&data);
```

**Evaluate whether to use references as parameters instead of cloning.**

### Comments

- ✅ **Doc comments on methods** (`///` for public functions, `//!` for modules)
- ❌ **NO comments in method body** → Code should be self-explanatory
- ✅ Exception: Complex algorithms or necessary workarounds

### File Headers (SPDX)

**REQUIRED:** All source files MUST include SPDX headers at the top.

- **License:** Apache-2.0
- **Copyright holder:** The Photometoria contributors
- **When to add:** Newly created files OR existing files missing headers

**Comment syntax by file type:**
```rust
// Rust (.rs)
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors
```

```python
# Python (.py), TOML (.toml), Shell scripts (.sh)
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: 2026 The Photometoria contributors
```

```lua
-- Lua (.lua)
-- SPDX-License-Identifier: Apache-2.0
-- SPDX-FileCopyrightText: 2026 The Photometoria contributors
```

**Special cases:**
- Python files with shebang: Place shebang first, then SPDX headers
- Always include blank line after SPDX headers before code

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

## 🧪 Testing Guidelines (CRITICAL)

### ✅ WHERE to Write Tests

- **Handler tests** (`handlers/{module}.rs`) → Business logic, error handling, custom validations
- **Storage tests** → CRUD, edge cases, persistence
- **Model tests** → Constructors, state transitions, conversions
- **Service tests** → Business logic with mocked dependencies

### ⚠️ Smoke Tests ONLY

- **Router tests** (`routes/mod.rs`) → NO logic duplication

### ❌ WHAT NOT to Test

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

📖 **Full details:** `api/docs/development.md` → Testing section

---

## 🤖 AI Models (Ollama)

- **qwen3-vl:8b** → PRODUCTION (best quality, slower)
- **llava** → DEVELOPMENT (faster, good for testing)

---

## 📋 Roadmap (Issue Tracker)

### ✅ Completed

- #7: Ollama service client
- #8: Worker Pool (`src/services/worker/`)
- #9: Job endpoints (CRUD + retry + cancel)
- #10: `GET /api/models`

### 🚧 In Development

- #11: SSE streaming for job progress
- #13: Input validations
- #5-6: Configuration system
- #15: CLI subcommands

### 📋 Planned

- #12: Lightroom Plugin (Lua)
- #14: Integration tests expansion
- #16-18: Storage enhancements

---

## ⚙️ Development Commands
```bash
# Build & run
cd api
cargo build --release
cargo run --release

# Quality checks (always before commit!)
cargo fmt && cargo clippy && cargo test

# Python tests
cd scripts
pip install -r requirements.txt
python3 test_models.py

# Quick API test
curl -X POST http://localhost:8080/api/tasks \
  -H "Content-Type: application/json" \
  -d '{"context":"test photos"}'
```

**Default server:** `http://localhost:8080`

---

## 🔧 GitHub CLI Best Practice

**⚠️ IMPORTANT:** Always use `--json` to avoid Projects (classic) deprecation errors:
```bash
# ✅ CORRECT
gh issue view 3 --json title,body,state,labels

# ❌ WRONG (causes GraphQL error)
gh issue view 3
```

---

## 📦 Technology Stack

### Rust Crates

- axum, tokio (full), tower
- reqwest (json), serde (derive), serde_json
- anyhow (app errors), thiserror (domain errors)
- tracing + tracing-subscriber (env-filter)

### Error Handling Pattern
```rust
Result<T, anyhow::Error>  // App level
.context("descriptive message")?  // Add context

// Domain-specific
#[derive(Error, Debug)]
pub enum OllamaError { ... }
```

### Concurrency

- `Arc<RwLock<T>>` → Shared mutable state
- `Arc<T>` → Shared immutable
- Future cancellation → `tokio::select!` / `timeout()`

### Edition

**Rust Edition:** `2024` (stable since Rust 1.85.0, currently using 1.92.0)

---

## 📝 Git Workflow

- Imperative mood in commit messages
- Atomic commits (one logical change per commit)
- Run quality checks before committing

---

## 📚 Documentation Structure

- `README.md` - Project overview, quick start
- `CONTRIBUTING.md` - Development guidelines, coding standards
- `api/docs/development.md` - Detailed API implementation guide
- `CLAUDE.md` (this file) - AI assistant context for coding agents

---

## 🎓 Key Design Decisions

### Why Rust?

- Zero-overhead performance for image processing
- Memory safety guarantees without GC
- Perfect for GPU-bound workloads (Ollama integration)
- Enterprise-ready for confidential data workflows

### Why Local AI (Ollama)?

- Privacy-first: no cloud dependencies
- Data sovereignty for corporate environments
- Cost-effective for large photo collections
- Full control over model selection and updates

### Why Manual Batch Processing?

- Photographers work in sessions, not real-time
- Quality over speed: AI analysis can take time
- Allows review and editing before applying to Lightroom
- Better resource management (GPU scheduling)

---

## 🔐 Security & Privacy

- All AI processing happens locally (Ollama)
- No external API calls for image analysis
- Temporary storage for uploaded photos (configurable retention)
- Suitable for confidential/sensitive photo collections

---

**End of AI Assistant Memory**