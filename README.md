# Photometoria

## Project Overview

Photometoria is an AI-powered metadata generation system for photography that integrates local AI models with image organization software, currently Adobe Lightroom Classic. The system is designed for manual batch processing of large photo collections, using a multi-level tagging approach that analyzes individual photos for fine details, groups photos for contextual macro categories, incorporates user-provided context hints, and extracts EXIF metadata.

The project aims to automate photo tagging workflows for photographers managing substantial image libraries.

The project consists of:

- **REST API** : Backend service that orchestrates image analysis using local AI models (Ollama)
- **Lightroom Plugin** : Lua extension for Adobe Lightroom Classic that allows photographers to send images to the API and receive generated metadata directly in their catalog
- **Testing Scripts** : Python tools for testing and validating AI models

The main goal is to automate the photographic keywording process, drastically reducing the time required to catalog images while maintaining high quality and precision in the generated metadata.

## Project Status

**Current Version:** 0.1.0 (Early Development)

- ✅ Python testing scripts functional
- ✅ Ollama service client implemented
- ✅ Worker pool for job processing implemented
- ✅ Job endpoints implemented (CRUD, retry, cancel)
- ✅ Models endpoint implemented (`GET /api/models`)
- 🚧 SSE streaming for job progress (#11)
- 🚧 Input validations (#13)
- 🚧 Configuration system (#5-6)
- 🚧 CLI subcommands (#15)
- 🚧 Lightroom Plugin (Lua) - planned (#12)
- 📋 Integration tests expansion (#14)
- 📋 Storage enhancements (#16-18)

The project is in active development. Core API functionality is implemented and functional.

## Getting Started

### Prerequisites

**Hardware:**

- NVIDIA GPU(s) recommended for AI model inference
- Sufficient storage for photo uploads (configurable quota)

  **Software:**

- [Ollama](https://ollama.ai/) - Local AI model inference engine
- Rust toolchain (for API development)
- Python 3.11+ (for testing scripts)

### Quick Start

1. **Install and start Ollama:**

```bash
# Install Ollama (see https://ollama.ai)
ollama serve

# Pull recommended model
ollama pull qwen3-vl:8b
```

2. **Test with Python scripts:**

```bash
cd scripts
pip install -r requirements.txt
python3 test_models.py
```

3. **Build and run API:**

```bash
cd api
cargo build --release
cargo run --release
```

## Documentation

- **[API Documentation](api/README.md)** - Comprehensive REST API specification and implementation guide
- **[Plugin Documentation](plugin/README.md)** - Lightroom Classic plugin integration
- **[Scripts Documentation](scripts/README.md)** - Python testing tools and usage
- **[Contributing Guidelines](CONTRIBUTING.md)** - Coding standards and guidelines for development

## Development

### Claude Code Skills

This project includes [Claude Code](https://claude.com/claude-code) skills for AI-assisted development. Skills provide on-demand development guidelines scoped to each component:

| Skill | Command | Description |
|-------|---------|-------------|
| API Development | `/api-dev` | Rust code style, testing guidelines, tech stack, dev commands |
| Plugin Development | `/plugin-dev` | LrView UI constraints, Lua testing, Lightroom workflow |

Invoke a skill at the start of a session when working on the corresponding component.

### Contributing

This project uses AI-assisted development with coding agents. Before contributing:

1. Read the [Contributing Guidelines](CONTRIBUTING.md) for code style and conventions
2. Review the [API Documentation](api/README.md) for architecture details
3. Run tests and formatting before committing:

```bash
# Rust
   cd api
   cargo fmt && cargo clippy && cargo test

   # Python
   cd scripts
   # Run your tests
```

### Project Structure

```
photometoria/
├── api/              # Rust REST API server (Axum)
├── plugin/           # Lightroom Classic plugin (Lua)
├── scripts/          # Python testing and validation tools
├── CONTRIBUTING.md   # Development and coding guidelines
└── README.md         # This file
```

### Git Workflow

- Use imperative mood in commit messages
- Keep commits atomic (one logical change per commit)
- Run quality checks before committing

## License

Copyright (c) 2026 Fabrizio Di Giuseppe

Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except in compliance with the License. You may obtain a copy of the License at

```
http://www.apache.org/licenses/LICENSE-2.0
```

Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the License for the specific language governing permissions and limitations under the License.

**Trademark Notice:** "Photometoria" is a trademark of Fabrizio Di Giuseppe.
