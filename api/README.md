# Photometoria API

## Overview

This is the **REST API server** component of Photometoria, implemented in Rust using the Axum framework. It provides HTTP endpoints for managing photo analysis tasks, uploading images, and orchestrating AI-powered metadata generation through local Ollama models.

## Key Features

- **Task Management** - Create and manage working sessions for photo collections
- **Photo Storage** - Handle image uploads with configurable storage quotas
- **AI Processing** - Coordinate concurrent job execution across multiple GPUs
- **Real-time Updates** - Stream job progress via Server-Sent Events (SSE)
- **Model Abstraction** - Support multiple AI models with optimized prompts

## Quick Start

### Prerequisites

- Rust toolchain (1.70+)
- Ollama running with vision models installed
- GPU with CUDA/ROCm/Metal support (recommended)

### Installation

1. **Clone the repository:**
   ```bash
   git clone https://github.com/yourusername/photometoria.git
   cd photometoria/api
   ```

2. **Configure the server:**
   ```bash
   cp config.toml.example config.toml
   # Edit config.toml with your settings
   ```

3. **Pull required AI models:**
   ```bash
   ollama pull qwen3-vl:8b
   ollama pull llava
   ```

4. **Build and run:**
   ```bash
   cargo build --release
   cargo run --release
   ```

The API server will start on `http://localhost:8080` (configurable).

### Quick Test

Create a task and verify the server is running:

```bash
# Create a task
curl -X POST http://localhost:8080/api/tasks \
  -H "Content-Type: application/json" \
  -d '{"context":"test photos"}'

# List tasks
curl http://localhost:8080/api/tasks
```

## Basic Workflow

1. **Create a task** - Start a working session
2. **Upload photos** - Add images to analyze
3. **Start a job** - Begin AI analysis with chosen model
4. **Monitor progress** - Stream real-time updates via SSE
5. **Retrieve results** - Get generated metadata tags
6. **Cleanup** - Delete task when finished

See [API Reference](docs/api-reference.md) for detailed endpoint documentation.

## Architecture

The API server uses:

- **Axum** - Async web framework built on Tokio
- **Worker Pool** - GPU-aware concurrent job execution
- **SSE Streaming** - Real-time progress updates
- **Abstraction Layers** - Future-proof storage and provider interfaces

**Core concepts:**

- **Task**: A working session containing photos and context
- **Photo**: An uploaded image file with metadata
- **Job**: An AI analysis process on a set of photos
- **Worker**: GPU-bound executor for job processing

See [Architecture Documentation](docs/architecture.md) for detailed design.

## Documentation

- **[Architecture](docs/architecture.md)** - System design and core concepts
- **[API Reference](docs/api-reference.md)** - Complete endpoint documentation
- **[Development Guide](docs/development.md)** - Setup, testing, and workflow
- **[Configuration](docs/configuration.md)** - Configuration file reference

## Model Support

Currently tested models:

- **qwen3-vl:8b** - Best quality, slower (recommended for production)
- **llava** - Faster, good for development iteration

See [Configuration](docs/configuration.md) for model setup details.

## Development

### Running Tests

```bash
# Unit tests
cargo test

# Integration tests
cargo test --test '*'

# With output
cargo test -- --nocapture
```

### Code Quality

```bash
# Linting
cargo clippy --all-targets

# Formatting
cargo fmt
```

### API Testing

Use curl, Postman, or Bruno for manual API testing. See [Development Guide](docs/development.md) for detailed examples.

## Project Structure

```
api/
├── README.md              # This file
├── docs/                  # Documentation
│   ├── architecture.md    # System design
│   ├── api-reference.md   # API endpoints
│   ├── development.md     # Development guide
│   └── configuration.md   # Configuration reference
├── src/                   # Source code
│   ├── main.rs           # Entry point
│   ├── routes/           # API routes
│   ├── handlers/         # Request handlers
│   ├── services/         # Business logic
│   ├── storage/          # Data persistence
│   ├── models/           # Data structures
│   └── sse/              # Server-Sent Events
├── Cargo.toml            # Dependencies
└── config.toml           # Configuration (create from example)
```

## Contributing

See [CONTRIBUTING.md](../CONTRIBUTING.md) for development guidelines.

## License

Apache 2.0 - See [LICENSE](../LICENSE) for details.

## Version

Current: v0.1.0 (Planned)

---

*For the main Photometoria project documentation, see the [root README](../README.md).*
