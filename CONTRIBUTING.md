# Contributing Guidelines for Photometoria

This document provides coding guidelines for contributing the Photometoria project.

## Project Overview

**Photometoria** is an AI-powered photographic metadata generation system consisting of:
- **Rust REST API** (Axum framework) - Backend server for AI processing
- **Python Scripts** - AI model testing and prototyping
- **Future Lightroom Plugin** (Lua) - Desktop integration

**Current Status:** Early development (v0.1.0) - API skeleton only, Python scripts functional

## Build, Test, and Lint Commands

### Rust API (`api/` directory)

```bash
# Build
cargo build                    # Debug build
cargo build --release          # Optimized release build
cargo check                    # Fast syntax/type check without codegen

# Run
cargo run                      # Start the API server (default: http://127.0.0.1:8080)

# Test
cargo test                     # Run all tests
cargo test <test_name>         # Run specific test
cargo test -- --nocapture      # Show println! output in tests

# Code Quality
cargo fmt                      # Auto-format code (rustfmt 1.8.0)
cargo clippy                   # Lint and catch common mistakes
cargo clippy -- -D warnings    # Treat warnings as errors
```

### Python Scripts (`scripts/` directory)

```bash
# Setup
pip install -r requirements.txt

# Run Tests
python3 test_models.py                    # Default: qwen3-vl:8b model
python3 test_models.py -m llava           # Specific model
python3 test_models.py --compare          # Test all available models
python3 test_models.py --list             # List Ollama models
```

**Note:** Python code follows PEP 8. Use `black` or `ruff` for formatting if needed.

## Code Style Guidelines

### Rust

#### Import Ordering
```rust
// 1. Standard library (implicit)
// 2. External crates (alphabetical)
use axum::{routing::get, Router};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
// 3. Internal modules (alphabetical)
use crate::models::Photo;
use super::*;
```

#### Naming Conventions
- **Functions/Variables:** `snake_case` (e.g., `create_router`, `photo_count`)
- **Types/Structs/Enums:** `PascalCase` (e.g., `PhotoMetadata`, `JobStatus`)
- **Constants:** `UPPER_SNAKE_CASE` (e.g., `MAX_WORKERS`, `DEFAULT_PORT`)
- **Modules:** `snake_case` (e.g., `mod.rs`, `handlers.rs`)

#### Type Annotations
- Always specify return types on public functions
- Use type inference for local variables when obvious
- Prefer explicit types in struct fields and function signatures

```rust
// Good
pub fn process_photo(path: &Path) -> Result<Photo, PhotoError> {
    let metadata = extract_metadata(path)?;
    Ok(Photo::new(metadata))
}

// Avoid in public APIs
pub fn process_photo(path: &Path) {  // Missing return type
```

#### Error Handling
- Use `anyhow::Result` for application-level errors
- Use `thiserror` for custom domain errors
- Never `unwrap()` or `expect()` in production code (tests are OK)
- Use `?` operator for error propagation

```rust
use anyhow::{Context, Result};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PhotoError {
    #[error("Invalid image format: {0}")]
    InvalidFormat(String),
    #[error("AI processing failed: {0}")]
    AiError(String),
}

pub fn load_image(path: &Path) -> Result<Image> {
    Image::open(path)
        .with_context(|| format!("Failed to load image: {}", path.display()))
}
```

#### Async Patterns
- Use `async fn` for I/O-bound operations
- Mark test functions with `#[tokio::test]`
- Prefer `tokio::spawn` for concurrent tasks

#### Formatting
- Use `rustfmt` with default settings (4-space indentation)
- Keep functions under 50 lines when possible
- Add blank lines between logical sections

#### Documentation
- Document public APIs with `///` doc comments
- Include examples in doc comments for complex functions
- Use `//` for implementation notes

### Python

#### Import Ordering (PEP 8)
```python
# 1. Standard library
import json
import sys
from datetime import datetime
from pathlib import Path
from typing import Dict, List, Any

# 2. Third-party packages
import requests
from PIL import Image

# 3. Local modules (when applicable)
from .utils import extract_exif
```

#### Naming Conventions
- **Functions/Variables:** `snake_case` (e.g., `extract_exif`, `image_path`)
- **Classes:** `PascalCase` (e.g., `PhotoAnalyzer`)
- **Constants:** `UPPER_SNAKE_CASE` (e.g., `MODEL_CONFIGS`, `TIMEOUT_SECONDS`)

#### Type Annotations
- Use type hints on all function signatures
- Import from `typing` module for complex types

```python
from pathlib import Path
from typing import Dict, Any, List

def call_ollama(image_path: Path, prompt_type: str) -> Dict[str, Any]:
    """Analyze image using Ollama API."""
    pass
```

#### Error Handling
- Catch specific exceptions, not bare `except:`
- Return error dictionaries in API-like functions
- Use `try`/`except`/`finally` appropriately

```python
try:
    response = requests.post(url, json=data, timeout=120)
    response.raise_for_status()
    return response.json()
except requests.Timeout:
    return {'success': False, 'error': 'Request timeout'}
except requests.RequestException as e:
    return {'success': False, 'error': str(e)}
```

## Git Commit Guidelines

Use **imperative mood** in commit messages:

```bash
# Good
git commit -m "Add photo metadata extraction endpoint"
git commit -m "Fix race condition in worker pool"
git commit -m "Update API documentation for /analyze endpoint"

# Avoid
git commit -m "Added photo metadata"  # Past tense
git commit -m "Fixes bug"             # Not descriptive
```

**Format:** Single-line summary (no prefixes like `feat:` or `fix:`)
- Capitalize first word
- No period at end
- Focus on what changed, not why
- Keep under 72 characters

## Architecture Notes

- **Modular Design:** Code is organized into modules: `config`, `handlers`, `models`, `routes`, `services`
- **Abstraction Layers:** Planned for future database/storage evolution (currently in-memory)
- **Async-First:** Use Tokio runtime for all I/O operations
- **Future SSE:** Server-Sent Events will be used for real-time job updates
- **Worker Pool:** GPU management system planned (not yet implemented)

## Important Context

- **Hardware:** Development on Linux with 2 NVIDIA GPUs (RTX 3060Ti + GTX 1080)
- **AI Backend:** Ollama for local model inference (qwen3-vl:8b is production model)
- **License:** Proprietary (Copyright 2026 Fabrizio Di Giuseppe)
- **Documentation First:** Maintain comprehensive README.md files in each directory

## When Making Changes

1. **Read existing README.md** in the relevant directory first
2. **Follow existing patterns** - consistency matters more than personal preference
3. **Write tests** for new functionality (use `#[tokio::test]` for async tests)
4. **Update documentation** if changing public APIs
5. **Run code quality checks** before committing:
   ```bash
   cargo fmt && cargo clippy && cargo test
   ```
6. **Keep commits atomic** - one logical change per commit
7. **Never commit tool-specific config** (CLAUDE.md, .cursor/, etc. - see .gitignore)

## Testing Philosophy

- **Integration Tests:** Test HTTP request/response cycles using Tower's `ServiceExt::oneshot`
- **Real Data:** Python scripts use actual photos from `scripts/test_images/`
- **Comprehensive Coverage:** Test both success and error paths
- **Async Testing:** All Rust tests must handle async properly with `#[tokio::test]`

## References

- Main docs: `README.md` (project overview)
- API docs: `api/README.md` (comprehensive API specification)
- Test images: `scripts/test_images/README.md` (copyright notice)
- License: `LICENSE` (proprietary terms)
