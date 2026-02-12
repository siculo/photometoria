# Photometoria - Project Context

## Overview

Photometoria is an AI-powered metadata generation system for photography. It uses local AI models (Ollama) to automatically generate keywords and metadata for photo collections, integrating with Adobe Lightroom Classic.

**Version:** 0.1.0 (Early Development)

## Architecture

### Components

| Component | Technology | Status |
|-----------|------------|--------|
| REST API | Rust / Axum | In development |
| Lightroom Plugin | Lua | Planned |
| Testing Scripts | Python 3.11+ | Functional |

### Core Concepts

* **Task**: A working session containing photos and context
* **Photo**: An uploaded image file with metadata
* **Job**: An AI analysis process on a set of photos
* **Worker**: GPU-bound executor for job processing

### API Workflow

1. Create a task (working session)
2. Upload photos to the task
3. Start a job with chosen AI model
4. Monitor progress via SSE streaming (Issue #11, not yet implemented)
5. Optionally cancel the job (`POST /api/jobs/{id}/cancel`)
6. Retrieve generated metadata tags (`GET /api/jobs/{id}/results`)
7. Retry failed or unprocessed photos (`POST /api/jobs/{id}/retry`)
8. Cleanup (delete task)

## Project Structure

```
photometoria/
├── api/              # Rust REST API server (Axum)
│   ├── src/          # Source code
│   │   ├── main.rs
│   │   ├── routes/
│   │   ├── handlers/
│   │   ├── services/
│   │   ├── storage/
│   │   └── models/
│   └── docs/         # API documentation
├── plugin/           # Lightroom Classic plugin (Lua)
├── scripts/          # Python testing tools
└── CONTRIBUTING.md   # Development guidelines
```

## AI Models

Tested models via Ollama:
* **qwen3-vl:8b** - Best quality, slower (recommended for production)
* **llava** - Faster, good for development

## Open Issues (Roadmap)

### Configuration (Issues #5-#6)

* #5: Complete configuration with all sections
* #6: CLI configuration overrides

### AI Integration (Issues #7-#8)

* ✅ #7: Implement Ollama service client (COMPLETED)
* ✅ #8: Implement Worker Pool for job processing (COMPLETED - `src/services/worker/`)

### Job Management (Issues #9-#11)

* ✅ #9: Implement Job endpoints (COMPLETED)
  - All CRUD endpoints: `POST /api/tasks/{id}/jobs`, `GET /api/jobs`, `GET /api/jobs/{id}`, `GET /api/jobs/{id}/results`, `DELETE /api/jobs/{id}`
  - `POST /api/jobs/{id}/retry` — retries failed + unprocessed photos (e.g. after cancellation)
  - `POST /api/jobs/{id}/cancel` — removes pending photos from buffer, marks job `cancelled`
  - Delete guards: tasks and photos cannot be deleted while any job is active; jobs must be in terminal state to be deleted
  - Job transitions to `processing` before AI analysis begins (not after first photo completes)
* ✅ #10: Implement GET /api/models endpoint (COMPLETED)
* #11: Implement SSE streaming for job progress

### Plugin & Quality (Issues #12-#14)

* #12: Lightroom Plugin - Basic structure
* #13: Add input validations
* #14: Expand test suite with integration tests

### CLI & Storage (Issues #15-#18)

* #15: Implement CLI subcommands (serve, config check, version)
* #16: Configurable accepted image formats for upload
* #17: Implement filesystem storage consistency checks on startup
* #18: Add image type information to Photo model

## Development Commands

```bash
# Build and run API
cd api
cargo build --release
cargo run --release

# Quality checks
cargo fmt && cargo clippy && cargo test

# Test with Python scripts
cd scripts
pip install -r requirements.txt
python3 test_models.py
```

## Testing Guidelines

**Where to write tests:**
- ✅ **Handler tests** (`src/handlers/{module}.rs`) - Test business logic, error handling, custom validations
- ✅ **Storage tests** - CRUD operations, edge cases, persistence
- ✅ **Model tests** - Constructors, state transitions, conversions
- ✅ **Service tests** - Business logic with mocked dependencies
- ⚠️ **Router tests** (`src/routes/mod.rs`) - Smoke tests only, no logic duplication

**What NOT to test:**
- ❌ Axum framework validations (UUID parsing, JSON deserialization)
- ❌ HTTP routing (Axum's responsibility)
- ❌ Standard serialization

**Test utilities:**
- Use `handlers::test_utils::fixtures::create_test_state()` for consistent setup
- Avoid duplicating test infrastructure

**Naming:** `test_<handler>_<scenario>` (e.g., `test_create_job_task_not_found`)

📖 **Full details:** `api/docs/development.md` → Testing section

## GitHub CLI Notes

When using `gh` to interact with issues and PRs, always use JSON output format to avoid errors with deprecated Projects (classic):

```bash
# Correct - use --json flag
gh issue view 3 --json title,body,state,labels

# Incorrect - causes GraphQL error about Projects (classic) deprecation
gh issue view 3
```

## API Server

Default: `http://localhost:8080`

Quick test:

```bash
# Create a task
curl -X POST http://localhost:8080/api/tasks \
  -H "Content-Type: application/json" \
  -d '{"context":"test photos"}'

# List tasks
curl http://localhost:8080/api/tasks
```

