# REST API Structure (Rust)

The API uses Axum as the web framework.

## Modules

- `routes/` - REST endpoint definitions (paths, HTTP methods, routing to handlers)
- `handlers/` - Business logic for each endpoint
- `services/` - External integrations (primarily Ollama API calls)
- `models/` - Data structures (requests, responses, entities)
- `config/` - Application configuration

## Main Dependencies

- `axum` - Async web framework
- `tokio` - Async runtime
- `reqwest` - HTTP client for Ollama calls
- `serde` / `serde_json` - JSON serialization
- `anyhow` / `thiserror` - Error handling
- `tracing` - Logging and tracing
