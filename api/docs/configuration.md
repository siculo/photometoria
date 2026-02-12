# Configuration Reference

## Overview

The Photometoria API server is configured via a TOML file read at startup. This document provides complete reference for all configuration options.

## Configuration File Location

By default, the server looks for `config.toml` in the current working directory. You can specify an alternative path using environment variables or command-line arguments.

## Example Configuration

```toml
[server]
host = "0.0.0.0"
port = 8080

[storage]
path = "/var/photometoria/storage"
max_size = "100GB"

[upload]
max_photos_per_request = 100
max_photo_size = "20MB"

[ai]
default_provider = "ollama"

[ai.providers.ollama]
type = "ollama"
base_url = "http://localhost:11434"
timeout_seconds = 120
devices = []

[ai.providers.ollama.models.qwen3-vl]
ollama_model = "qwen3-vl"
prompt_template = "Analyze this photo and provide comma-separated tags. Context: {context}"
description = "Best quality, slower processing"
supports_vision = true

[ai.providers.ollama.models.llava]
ollama_model = "llava:latest"
prompt_template = "List tags for this image, comma-separated. Context: {context}"
description = "Faster, good for testing"
supports_vision = true

[worker_pool]
min_photos_before_swap = 10
max_time_before_swap = "120s"
worker_idle_sleep = "500ms"
discovery_poll_interval = "5s"
```

## Configuration Sections

### [server]

Server binding and network configuration.

**Options:**

- **host** (string, required)
  - Bind address for the HTTP server
  - Default: `"0.0.0.0"` (all interfaces)
  - Examples: `"127.0.0.1"` (localhost only), `"0.0.0.0"` (all interfaces)

- **port** (integer, required)
  - TCP port to listen on
  - Default: `8080`
  - Range: 1024-65535 (recommended)

**Example:**

```toml
[server]
host = "0.0.0.0"
port = 8080
```

### [storage]

Photo storage configuration.

**Options:**

- **path** (string, required)
  - Filesystem path for storing uploaded photos
  - Must be writable by the server process
  - Should have sufficient disk space
  - Can be relative or absolute

- **max_size** (string or integer, required)
  - Total storage quota with unit suffix
  - Decimal units (base 1000): `KB`, `MB`, `GB`, `TB`
  - Binary units (base 1024): `KiB`, `MiB`, `GiB`, `TiB`
  - Can also be a plain integer (bytes)
  - Applies to all photos across all tasks
  - Upload requests fail with 507 error when quota exceeded
  - Set based on available disk space
  - Note: Filesystems typically report in binary units (GiB), hard drives in decimal (GB)

**Example:**

```toml
[storage]
path = "/var/photometoria/storage"
max_size = "100GB"
```

**Notes:**
- Directory is created automatically if it doesn't exist
- Quota is enforced before accepting uploads
- Deleting tasks frees up storage space

### [upload]

Photo upload limits.

**Options:**

- **max_photos_per_request** (integer, required)
  - Maximum number of photos in a single upload request
  - Prevents excessively large batch uploads
  - Recommended: 50-100
  - Must be ≥ 1

- **max_photo_size** (string or integer, required)
  - Maximum size per individual photo with unit suffix
  - Decimal units (base 1000): `KB`, `MB`, `GB`
  - Binary units (base 1024): `KiB`, `MiB`, `GiB`
  - Can also be a plain integer (bytes)
  - Photos larger than this are rejected with 400 error
  - Recommended: `"20MB"` - `"50MB"`

**Example:**

```toml
[upload]
max_photos_per_request = 50
max_photo_size = "20MB"
```

**Notes:**
- These limits help prevent resource exhaustion
- Clients should handle batch uploads appropriately
- Large photos may slow down AI processing

### [ai]

AI provider configuration. The system uses an abstraction layer that supports multiple AI providers.

**Options:**

- **default_provider** (string, optional)
  - Name of the default provider to use when none is specified
  - Must match a key in `[ai.providers.*]`
  - If only one provider is configured, it becomes the default automatically

**Example:**

```toml
[ai]
default_provider = "ollama"
```

### [ai.providers.{name}]

Provider-specific configuration. Each provider is identified by a unique name (e.g., `ollama`).

#### Ollama Provider

**Options:**

- **type** (string, required)
  - Must be `"ollama"` for Ollama providers

- **base_url** (string, optional)
  - Base URL for the Ollama HTTP API
  - Default: `"http://localhost:11434"`

- **timeout_seconds** (integer, optional)
  - Request timeout in seconds for a single AI analysis request
  - Default: `120`
  - This is a single value for the whole provider. Set it to accommodate the
    **slowest model on the slowest GPU** in your configuration — larger and more
    complex models (e.g., `qwen3-vl`) require significantly more time than
    lighter ones (e.g., `llava`), and older or lower-VRAM cards will be slower
    than high-end ones. If you observe timeout errors, increase this value rather
    than assuming the analysis failed permanently (use the retry endpoint to
    reprocess failed photos).

- **devices** (array of integers, optional)
  - GPU device indices to use (empty = auto-detect)
  - Default: `[]`

**Example:**

```toml
[ai.providers.ollama]
type = "ollama"
base_url = "http://localhost:11434"
timeout_seconds = 120
devices = []
```

### [ai.providers.{name}.models.{model_id}]

Model configuration within a provider. Each model is identified by a unique ID.

**Options:**

- **ollama_model** (string, required)
  - Actual Ollama model name
  - Examples: `"qwen3-vl"`, `"llava:latest"`

- **prompt_template** (string, optional)
  - Template for generating prompts
  - If not specified, uses the prompt from the API request

- **description** (string, optional)
  - Human-readable description

- **supports_vision** (boolean, optional)
  - Whether this model supports image analysis
  - Default: `true`

**Example:**

```toml
[ai.providers.ollama.models.qwen3-vl]
ollama_model = "qwen3-vl"
prompt_template = "Analyze this photo and provide comma-separated tags."
description = "Best quality, slower processing"
supports_vision = true

[ai.providers.ollama.models.llava]
ollama_model = "llava:latest"
prompt_template = "List tags for this image, comma-separated."
description = "Faster, good for testing"
supports_vision = true
```

### [worker_pool]

Controls how the worker pool schedules photo processing across jobs and manages model switching.

The number of workers is determined by `[ai.providers.ollama] devices`: one worker per configured GPU. If `devices` is empty, one worker runs on device 0.

**Options:**

- **min_photos_before_swap** (integer, optional)
  - Minimum photos to process with the current model before allowing a switch to another model
  - Higher values improve efficiency (fewer model reloads); lower values improve fairness between jobs
  - Must be ≥ 1
  - Default: `10`

- **max_time_before_swap** (string, optional)
  - Maximum time a model can stay loaded before forcing a swap
  - Ensures jobs with different models are not starved indefinitely
  - Supported suffixes: `ms` (milliseconds), `s` (seconds), `m` (minutes)
  - Default: `"120s"`

A model swap is allowed only when **both** thresholds are exceeded. This balances loading efficiency with temporal fairness.

- **worker_idle_sleep** (string, optional)
  - How long a worker sleeps when the photo buffer is empty before checking again
  - Lower values reduce processing latency at the cost of more CPU usage when idle
  - Supported suffixes: `ms` (milliseconds), `s` (seconds), `m` (minutes)
  - Default: `"500ms"`

- **discovery_poll_interval** (string, optional)
  - How often the worker pool polls the job store for newly queued jobs
  - Lower values reduce the delay between job creation and processing start
  - Supported suffixes: `ms` (milliseconds), `s` (seconds), `m` (minutes)
  - Default: `"5s"`

**Example:**

```toml
[worker_pool]
min_photos_before_swap = 10
max_time_before_swap = "120s"
worker_idle_sleep = "500ms"
discovery_poll_interval = "5s"
```

## AI Provider System

### Architecture

The AI provider system uses a registry pattern with trait-based abstraction:

```
┌─────────────────────────────────────────┐
│            Configuration                │
│  [ai.providers.ollama] → OllamaConfig   │
├─────────────────────────────────────────┤
│               Trait                     │
│  AIProvider (list_models, analyze_image,│
│              check_health)              │
├─────────────────────────────────────────┤
│           Implementation                │
│  OllamaProvider → calls Ollama API      │
├─────────────────────────────────────────┤
│              Registry                   │
│  HashMap<String, Arc<dyn AIProvider>>   │
│  + default_provider                     │
└─────────────────────────────────────────┘
```

### How It Works

1. **Server reads provider configurations** from `[ai.providers.*]` at startup
2. **Creates provider instances** and registers them in the ProviderRegistry
3. **Sets the default provider** based on `[ai.default_provider]`
4. **Handlers access providers** through the registry by name or via the default

### Adding a New Provider Instance

To add a new Ollama instance (e.g., remote server):

```toml
[ai.providers.ollama-remote]
type = "ollama"
base_url = "http://gpu-server:11434"
timeout_seconds = 180

[ai.providers.ollama-remote.models.qwen3-vl]
ollama_model = "qwen3-vl"
supports_vision = true
```

### Adding a Model to an Existing Provider

1. **Install the model in Ollama:**
   ```bash
   ollama pull model-name:tag
   ```

2. **Add a model configuration:**
   ```toml
   [ai.providers.ollama.models.my-model]
   ollama_model = "model-name:tag"
   prompt_template = "Your optimized prompt here."
   description = "Model description"
   supports_vision = true
   ```

3. **Restart the server**

### Future Providers

The abstraction layer is designed to support additional providers:

- **OpenAI** - Cloud-based, fast inference
- **Anthropic** - Claude models with vision
- **LocalAI** - OpenAI-compatible self-hosted

Each provider will have its own configuration section under `[ai.providers.*]`.

## Environment Variables

Currently, environment variables are not supported for configuration. All configuration must be in the TOML file.

**Future enhancement**: Support for environment variable overrides (e.g., `PHOTOMETORIA_SERVER_PORT=8080`).

## Configuration Validation

The server validates configuration at startup:

- Required fields must be present
- Values must be within valid ranges
- Ollama connection must be reachable
- Storage paths must be valid and writable

**Startup will fail** if configuration is invalid, with error messages indicating the problem.

## See Also

- [Architecture](architecture.md) - System design and implementation details
- [API Reference](api-reference.md) - REST API endpoints
- [Development Guide](development.md) - Setup and testing
