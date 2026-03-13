# Photometoria - AI Assistant Context

## Project Overview

Photometoria is an AI-based photographic metadata generation system integrated with Adobe Lightroom Classic. It uses a multi-level tagging approach: individual photo analysis, macro-categorization, contextual user suggestions, and EXIF data extraction.

The system is organized into two main components:

- **REST API server** (Rust/Axum) — analyzes photos and communicates with AI model providers
- **Lightroom Plugin** (Lua) — allows Lightroom Classic to communicate with the server

Supported providers include both local (Ollama) and remote providers.

**Version:** 0.2.0

---

## Architecture & Components

| Component | Technology | Status |
|-----------|------------|--------|
| REST API | Rust/Axum | In development |
| Lightroom Plugin | Lua 5.1 | In development |
| Testing Scripts | Python 3.11+ | Functional |

### Core Concepts

- **Provider**: External system that provides access to AI models (e.g., Ollama)
- **Model**: AI model with imaging/vision capabilities
- **Task**: Working session containing photos and context
- **Photo**: Uploaded image file with metadata
- **Job**: AI analysis process on a set of photos
- **Worker**: GPU-bound executor for job processing

---

## Code Style & Preferences

### General Principles

- **Clarity > micro-optimizations** — Prefer understandable code even if slightly less efficient
- **Separate execution paths** — Clearly divide different logical branches (early returns, well-structured match)
- **Short functions/methods** — Extract functional units into separate methods for readability
- **DRY for repeated patterns** — Constantly repeated patterns become reusable library functions

### Comments

- Doc comments on methods (`///` for public functions, `//!` for modules in Rust)
- **NO comments in method body** — Code should be self-explanatory
- Exception: Complex algorithms or necessary workarounds

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

---

## Git Workflow

- Imperative mood in commit messages
- Atomic commits (one logical change per commit)
- Run quality checks before committing

---

## GitHub CLI Best Practice

Always use `--json` to avoid Projects (classic) deprecation errors:

```bash
# CORRECT
gh issue view 3 --json title,body,state,labels

# WRONG (causes GraphQL error)
gh issue view 3
```

---

## Documentation Structure

- `README.md` — Project overview, quick start
- `CONTRIBUTING.md` — Development guidelines, coding standards
- `CLAUDE.md` (this file) — General project context for AI assistants
- `api/CLAUDE.md` — Rust API specific: endpoints, code structure, testing, tech stack
- `api/docs/` — API documentation (reference, architecture, configuration, development)
- `plugin/CLAUDE.md` — Lightroom plugin specific: structure, SDK constraints, testing
- `plugin/docs/` — Plugin documentation
- `docs/` — Cross-cutting documentation (provider abstraction, evolution plans)

---

## Roadmap

See the **GitHub issue tracker** for the current roadmap and progress.

---

## Key Design Decisions

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

## Security & Privacy

- All AI processing happens locally (Ollama)
- No external API calls for image analysis
- Temporary storage for uploaded photos (configurable retention)
- Suitable for confidential/sensitive photo collections

---

**End of AI Assistant Context**
