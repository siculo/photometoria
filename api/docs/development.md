# Development Guide

## Development Environment

### Requirements by Component

#### Ollama & AI Models

**Hardware:**

- GPU with sufficient memory for the AI models you intend to use
- Currently tested models: **qwen3-vl:8b** (higher quality, requires more VRAM) and **llava** (faster, lower requirements)
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

### Software Stack

**Core Technologies:**

- **Rust** - REST API server implementation
- **Ollama** - Local AI model inference engine
- **Python** - Initial testing and prototyping
- **Git/GitHub** - Version control

**Development Tools:**

- Node.js - JavaScript runtime
- Rust toolchain - Compiler and package manager

### Recommended Development Tools

**Code Editors:**

- **VS Code** - Versatile editor with extensive plugin ecosystem
- **RustRover** - JetBrains IDE optimized for Rust development

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

## Getting Started

### Prerequisites

**For API Server Development:**

- Rust toolchain installed (1.70+)
- Ollama running with desired models pulled
- GPU with appropriate drivers (CUDA/ROCm/Metal)

### Initial Setup

1. **Clone repository**

```bash
git clone https://github.com/yourusername/photometoria.git
cd photometoria/api
```

2. **Configure the server**

```bash
cp config.toml.example config.toml
# Edit config.toml with your settings
```

3. **Pull required Ollama models**

```bash
ollama pull qwen3-vl:8b
ollama pull llava
```

4. **Build and run the server**

```bash
# Development build
cargo build
cargo run

# Release build (optimized)
cargo build --release
cargo run --release
```

The server will start on `http://localhost:8080` (or the configured port).

## Testing

### Testing Strategy

The project follows a layered testing approach that emphasizes testing business logic in handlers while avoiding duplication with framework-level concerns.

#### 1. Handler Tests (Primary Test Layer)

**Location**: `src/handlers/{module}.rs` in `#[cfg(test)]` modules

**What to Test**:
- ✅ Business logic of handler functions
- ✅ Error handling (not found, validation failures, conflicts)
- ✅ Custom validations (e.g., photo_id belongs to task)
- ✅ State transitions and data transformations

**What NOT to Test**:
- ❌ Axum framework validations (UUID parsing, JSON deserialization)
- ❌ HTTP routing (tested by router smoke tests)
- ❌ Serialization of standard types

**Example Pattern**:
```rust
#[tokio::test]
async fn test_create_job_invalid_photo_id() {
    let ts = create_test_state().await;
    // Setup: create task with photos
    // Action: try to create job with invalid photo_id
    // Assert: error with "invalid_parameter"
}
```

**Naming Convention**: `test_<handler>_<scenario>`
- `test_create_job_all_photos` - happy path
- `test_create_job_task_not_found` - error case
- `test_create_job_invalid_photo_id` - validation error

#### 2. Storage Layer Tests

**Location**: `src/storage/{implementation}.rs`

**Coverage**:
- ✅ All CRUD operations
- ✅ Edge cases (duplicates, not found, empty results)
- ✅ Persistence (filesystem stores)
- ✅ Concurrent access (in-memory stores)

**Example**: `FileSystemTaskStore`, `FileSystemJobStore`

#### 3. Model Tests

**Location**: `src/models/{model}.rs`

**Coverage**:
- ✅ Constructors and factory methods
- ✅ State transitions (e.g., Job lifecycle: Queued → Processing → Completed)
- ✅ Conversions between entity and DTOs
- ✅ Custom serialization/deserialization

**Example**: Testing Job state machine, Photo metadata generation

#### 4. Service Tests

**Location**: `src/services/{module}.rs` or `tests/{module}_tests.rs`

**Coverage**:
- ✅ Business logic in services (AI provider selection, configuration)
- ✅ Integration with external dependencies (use mocking)
- ✅ Error handling and fallbacks

**Example**: `ProviderRegistry`, `OllamaProvider` (with WireMock)

#### 5. Router Tests (Minimal)

**Location**: `src/routes/mod.rs`

**Coverage**:
- ✅ Smoke tests only (e.g., `/version` endpoint works)
- ✅ Router configuration is valid
- ❌ Do NOT duplicate handler logic tests here

**Rationale**: Handler tests cover business logic more cleanly without HTTP overhead.

#### 6. Test Utilities

**Location**: `src/handlers/test_utils.rs`

**Purpose**:
- Provides `create_test_state()` for consistent AppState setup
- Defines `TestState` struct to keep temp directories alive
- Shared fixtures and helpers

**Usage**:
```rust
use crate::handlers::test_utils::fixtures::create_test_state;

#[tokio::test]
async fn my_test() {
    let ts = create_test_state().await;
    // Use ts.state for testing
}
```

**Guidelines**:
- Always use `create_test_state()` instead of duplicating setup
- Add new fixtures to `test_utils.rs` if needed by multiple test files
- Document new utilities with examples

### Running Tests

```bash
# Run all tests
cargo test

# Run tests for a specific module
cargo test handlers::tasks::tests

# Run with output visible
cargo test -- --nocapture

# Run integration tests only
cargo test --test '*'

# Run with coverage (requires cargo-tarpaulin)
cargo tarpaulin --out Html
```

### Test Organization

```
api/src/
├── handlers/
│   ├── tasks.rs          # Handler + #[cfg(test)] mod tests
│   ├── photos.rs         # Handler + tests
│   ├── jobs.rs           # Handler + tests
│   └── test_utils.rs     # Shared test fixtures
├── storage/
│   ├── filesystem_task_store.rs   # Storage + tests
│   └── in_memory_job_store.rs     # Storage + tests
├── models/
│   ├── task.rs           # Model + tests
│   └── job.rs            # Model + lifecycle tests
└── services/
    └── ai/
        └── registry.rs   # Service + tests

api/tests/
└── ai_provider_tests.rs  # Integration tests with WireMock
```

### Future: Integration Tests

**Status**: 🚧 To be defined

**Planned Coverage**:
- End-to-end API workflows (task → upload → job → results)
- Multi-user scenarios
- Performance/load testing
- Real Ollama integration tests (manual only for now)

**Current Approach**:
- Use `tests/ai_provider_tests.rs` pattern with WireMock
- Avoid duplicating handler unit tests
- Focus on cross-component integration

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

### Code Quality

**Linting:**

```bash
# Run clippy for lint warnings
cargo clippy --all-targets

# Auto-fix some issues
cargo clippy --all-targets --fix
```

**Formatting:**

```bash
# Check formatting
cargo fmt --check

# Format code
cargo fmt
```

## Key Learnings

### Model Performance

- **qwen3-vl:8b** produces superior results but slower (use for production)
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
- Comprehensive documentation captures full project state
- Feature branches for independent development

## Future Roadmap

### Short-term (Current Focus)

- Complete REST API server implementation
- Thorough testing with real photo collections
- Performance optimization for batch processing

### Medium-term

**Multi-user Support**: Add authentication and resource isolation
- User authentication and authorization
- Per-user task limits and storage quotas
- Isolated task/job workspaces
- Role-based access control (RBAC)

### Long-term

**Enhanced Storage:**
- Photo deduplication (content-addressable storage)
- Database-backed metadata
- Object storage integration (S3, MinIO)

**Advanced Features:**
- Job timeout and automatic cleanup
- Task templates and presets
- Batch operations on multiple tasks
- Export/import of results

**Performance:**
- Rust-based image preprocessing (resize, format conversion)
- GPU pooling optimization
- Caching layer for repeated analyses

**Alternative Implementations:**
- Potential Python version for easier community contributions
- Desktop application (non-server mode) for single-user scenarios

## Version History

- **v0.1.0** (Planned) - Initial REST API server implementation with core functionality

## See Also

- [Architecture](architecture.md) - System design and core concepts
- [API Reference](api-reference.md) - Complete endpoint documentation
- [Configuration](configuration.md) - Server configuration reference

