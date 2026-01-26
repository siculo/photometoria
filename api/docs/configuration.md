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

[gpu]
devices = [0, 1]      # GPU indices to use
max_workers = 2        # Maximum concurrent jobs

[storage]
path = "/var/photometoria/storage"
max_size_gb = 100

[upload]
max_photos_per_request = 50
max_photo_size_mb = 20

[ollama]
base_url = "http://localhost:11434"

[[models]]
name = "qwen2-vl:8b"
ollama_model = "qwen2-vl:8b"
prompt_template = "Analyze this photo and provide comma-separated tags. Context: {context}"
description = "Best quality, slower processing"

[[models]]
name = "llava"
ollama_model = "llava:latest"
prompt_template = "List tags for this image, comma-separated. Context: {context}"
description = "Faster, good for testing"
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

### [gpu]

GPU resource configuration for worker pool.

**Options:**

- **devices** (array of integers, required)
  - GPU device indices to use for AI processing
  - Each device typically runs one worker
  - Use `nvidia-smi` or similar to identify available GPUs
  - Examples: `[0]` (single GPU), `[0, 1]` (two GPUs)

- **max_workers** (integer, required)
  - Maximum number of concurrent job executions
  - Typically set to match number of GPUs (one worker per GPU)
  - Must be ≥ 1
  - Higher values may cause GPU memory issues

**Example:**

```toml
[gpu]
devices = [0, 1]
max_workers = 2
```

**Notes:**
- Each worker processes one job at a time
- Jobs are queued when all workers are busy
- More workers = more concurrent processing but higher GPU memory usage

### [storage]

Photo storage configuration.

**Options:**

- **path** (string, required)
  - Filesystem path for storing uploaded photos
  - Must be writable by the server process
  - Should have sufficient disk space
  - Can be relative or absolute

- **max_size_gb** (integer, required)
  - Total storage quota in gigabytes
  - Applies to all photos across all tasks
  - Upload requests fail with 507 error when quota exceeded
  - Set based on available disk space

**Example:**

```toml
[storage]
path = "/var/photometoria/storage"
max_size_gb = 100
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

- **max_photo_size_mb** (integer, required)
  - Maximum size per individual photo in megabytes
  - Photos larger than this are rejected with 400 error
  - Recommended: 20-50 MB
  - Must be > 0

**Example:**

```toml
[upload]
max_photos_per_request = 50
max_photo_size_mb = 20
```

**Notes:**
- These limits help prevent resource exhaustion
- Clients should handle batch uploads appropriately
- Large photos may slow down AI processing

### [ollama]

Ollama service configuration.

**Options:**

- **base_url** (string, required)
  - Base URL for the Ollama HTTP API
  - Must include protocol (http/https)
  - Default: `"http://localhost:11434"`
  - Examples:
    - Local: `"http://localhost:11434"`
    - Remote: `"http://ollama-server:11434"`

**Example:**

```toml
[ollama]
base_url = "http://localhost:11434"
```

**Notes:**
- Ollama must be running and accessible at this URL
- Server will fail to start if Ollama is not reachable
- Health check performed at startup

### [[models]]

AI model definitions (array of tables).

Each `[[models]]` block defines one supported model.

**Options:**

- **name** (string, required)
  - Model identifier used in API requests
  - Must be unique across all model definitions
  - Used in `POST /api/tasks/{task_id}/jobs` requests

- **ollama_model** (string, required)
  - Actual Ollama model name
  - Must match a model available in Ollama
  - Examples: `"qwen2-vl:8b"`, `"llava:latest"`

- **prompt_template** (string, required)
  - Template for generating prompts sent to the model
  - Supports `{context}` placeholder for user-provided context
  - Should request comma-separated tag output
  - Model-specific optimization recommended

- **description** (string, required)
  - Human-readable description of the model
  - Returned by `GET /api/models`
  - Examples: "Best quality, slower", "Fast, good for testing"

**Example:**

```toml
[[models]]
name = "qwen2-vl:8b"
ollama_model = "qwen2-vl:8b"
prompt_template = "Analyze this photo and provide comma-separated tags. Context: {context}"
description = "Best quality, slower processing"

[[models]]
name = "llava"
ollama_model = "llava:latest"
prompt_template = "List tags for this image, comma-separated. Context: {context}"
description = "Faster, good for testing"
```

**Notes:**
- Multiple models can be defined
- Only models installed in Ollama appear as "available"
- Different models may require different prompt templates for optimal results

## Supported Models System

### How It Works

The server maintains a list of supported models with their corresponding prompt templates:

1. **Server reads model definitions** from configuration at startup
2. **Queries Ollama** to check which models are actually installed
3. **Only models that are both configured and installed** appear in `GET /api/models`
4. **Job creation validates** that the requested model is available (returns 400 if not)

### Benefits

- **Centralized prompt optimization** - Each model has its own optimized prompt template
- **Validation at job creation** - Prevents jobs from failing due to unavailable models
- **Easy to add new models** - No code changes required, just configuration
- **Clear separation** - Distinguishes between supported vs. available models

### Adding New Models

To add a new model:

1. **Install the model in Ollama:**
   ```bash
   ollama pull model-name:tag
   ```

2. **Add a `[[models]]` block** to `config.toml`:
   ```toml
   [[models]]
   name = "my-new-model"
   ollama_model = "model-name:tag"
   prompt_template = "Your optimized prompt here. Context: {context}"
   description = "Model description for API clients"
   ```

3. **Restart the server** - The new model will appear in `GET /api/models`

4. **Test the model** - Create a job using the new model name

### Prompt Template Guidelines

When creating prompt templates:

- **Request comma-separated output** - The API expects this format
- **Include context placeholder** - Use `{context}` where user context should be inserted
- **Be specific** - Clear instructions produce better results
- **Test with real photos** - Optimize based on actual output quality
- **Model-specific tuning** - Different models respond differently to prompts

**Example templates:**

```toml
# Detailed analysis
prompt_template = "Analyze this photograph in detail and provide comma-separated descriptive tags. Include subjects, setting, mood, and technical aspects. Context: {context}"

# Concise tagging
prompt_template = "List short, comma-separated tags for this image. Context: {context}"

# Specific focus
prompt_template = "Identify landmarks, locations, and activities in this photo. Output as comma-separated tags. Context: {context}"
```

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
