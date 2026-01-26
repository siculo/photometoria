# Development Guide

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

## Getting Started

### Prerequisites

**For API Server Development:**

- Rust toolchain installed (1.70+)
- Ollama running with desired models pulled
- GPU with appropriate drivers (CUDA/ROCm/Metal)

**For Lightroom Plugin Development:**

- Adobe Lightroom Classic installed
- Lua development environment

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
ollama pull qwen2-vl:8b
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

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# Run with output
cargo test -- --nocapture

# Run integration tests
cargo test --test '*'
```

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
- Comprehensive documentation captures full project state
- Feature branches for independent development

## Future Roadmap

### Short-term (Current Focus)

- Complete REST API server implementation
- Thorough testing with real photo collections
- Performance optimization for batch processing

### Medium-term

**Lightroom Lua Plugin**: Direct integration with Adobe Lightroom Classic
- Photo upload from Lightroom catalog
- Metadata write-back to Lightroom
- UI for job monitoring and retry

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
