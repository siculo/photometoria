# Photometoria API — AI Assistant Context

## API Workflow

```
1. GET  /api/info                          → Server info and capabilities
2. GET  /api/providers                     → List configured AI providers
3. GET  /api/providers/{provider_name}     → Provider details and models
4. GET  /api/models                        → List default provider models
5. POST /api/catalogs/{catalog_id}/tasks   → Create task (working session)
   GET  /api/catalogs/{catalog_id}/tasks   → List tasks by catalog
6. POST /api/tasks/{id}/photos             → Upload photos (multipart)
7. POST /api/tasks/{id}/jobs               → Start job (choose AI model, optional language)
8. GET  /api/tasks/{id}/jobs               → List jobs for a task
9. [TODO #11] SSE streaming                → Monitor progress
10. POST /api/jobs/{id}/cancel             → Cancel job (optional)
11. GET /api/jobs/{id}/results             → Retrieve generated metadata
12. POST /api/jobs/{id}/retry              → Retry failed/unprocessed photos
13. DELETE /api/tasks/{id}                 → Cleanup
```

---

## Code Structure

```
api/src/
├── main.rs           # Entry point, CLI parsing
├── lib.rs            # Library crate root (re-exports)
├── cli.rs            # CLI argument definitions (clap)
├── startup.rs        # Server initialization and startup logic
├── app_state.rs      # Shared application state (AppState)
├── config/           # TOML configuration parsing
│   ├── mod.rs        #   Config struct, load_config()
│   ├── ai.rs         #   AIConfig, ProviderConfig, OllamaProviderConfig
│   ├── server.rs     #   ServerConfig (host, port)
│   ├── storage.rs    #   StorageConfig (paths, max_size)
│   ├── upload.rs     #   UploadConfig (max photo size, max per request)
│   ├── worker_pool.rs#   WorkerPoolConfig
│   └── byte_size.rs  #   ByteSize helper type
├── routes/           # Endpoint definitions (Router)
│   └── mod.rs        #   create_router() — all route mappings
├── handlers/         # Business logic (TEST LOGIC HERE)
│   ├── mod.rs
│   ├── tasks.rs      #   CRUD tasks
│   ├── photos.rs     #   get/delete/list photos
│   ├── upload_photos.rs # Multipart upload handling
│   ├── jobs.rs       #   CRUD jobs + cancel/retry
│   ├── providers.rs  #   Provider listing, model discovery
│   ├── info.rs       #   Server info endpoint
│   ├── app_error.rs  #   AppError → HTTP response mapping
│   └── test_utils.rs #   Test fixtures and helpers
├── models/           # Domain structs
│   ├── task.rs
│   ├── photo.rs
│   ├── job.rs
│   └── info.rs       #   ServerInfo response struct
├── services/         # External services
│   ├── ai/           #   AI provider abstraction
│   │   ├── mod.rs    #     Module root, re-exports
│   │   ├── provider.rs #   AIProvider trait + types
│   │   ├── registry.rs #   ProviderRegistry
│   │   ├── error.rs  #     AIProviderError
│   │   └── ollama/   #     Ollama implementation
│   │       ├── mod.rs
│   │       ├── provider.rs
│   │       └── types.rs
│   └── worker/       #   Job processing
│       ├── mod.rs
│       ├── pool.rs   #     WorkerPool
│       ├── processor.rs #  Photo analysis logic
│       ├── queue.rs  #     Job queue
│       └── worker.rs #     Individual worker
└── storage/          # Persistence layer (filesystem)
    ├── mod.rs        #   Store traits + re-exports
    ├── task_store.rs
    ├── photo_store.rs
    ├── job_store.rs
    ├── filesystem_task_store.rs
    ├── filesystem_photo_store.rs
    ├── filesystem_job_store.rs
    └── filesystem_layout.rs
```

---

## Implemented Guardrails

- Tasks/photos not deletable while any job is active
- Jobs deletable only if in terminal state
- Job transitions to `processing` BEFORE AI analysis starts
- Cancel removes pending photos from buffer, marks job `cancelled`

---

## AI Provider Architecture

The system uses a trait-based provider abstraction:

```
AIProvider trait  →  defines: name(), list_models(), analyze_image(), check_health()
       ↓
OllamaProvider   →  implements AIProvider for Ollama backend
       ↓
ProviderRegistry →  HashMap<String, Arc<dyn AIProvider>> + default_provider
       ↓
AppState         →  holds Arc<ProviderRegistry>
```

- Providers are configured via TOML (`[ai.providers.ollama]`)
- Each provider has named models with backend mappings (e.g., `qwen3-vl` → `qwen3-vl:8b`)
- `ProviderRegistry::from_config()` instantiates all providers at startup
- Auto-selects default if only one provider is configured
- `is_model_configured()` validates model IDs against static config (no network call)
- Language support: jobs accept an optional `language` field; falls back to `default_language` in `[ai]` config, then to English
- Provider details expose `default_language` and per-model `supported_languages` from config

---

## AI Models (Ollama)

- **qwen3.5:latest** — PRODUCTION (recommended, high quality)
- **qwen3-vl:8b** — PRODUCTION (best vision quality, slower)
- **ministral-3:latest** — Mistral vision model
- **llava:latest** — DEVELOPMENT (faster, good for testing)
- **gemma3n:e4b** — Google Gemma 3n vision model

---

> **Development guidelines** (code style, testing, commands): use `/api-dev` skill
