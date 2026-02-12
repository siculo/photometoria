# AI Provider Abstraction Layer - Design Document

## Status

**Implemented** - Ollama provider complete (Issue #7)

## Overview

This document describes the abstraction layer that allows Photometoria to work with multiple AI vision providers. The implementation uses a trait-based design with a registry pattern.

## Current Implementation

### Architecture

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
│  ProviderRegistry                       │
│  HashMap<String, Arc<dyn AIProvider>>   │
│  + default_provider                     │
└─────────────────────────────────────────┘
```

### File Structure

```
src/services/ai/
├── mod.rs           # Module exports
├── error.rs         # AIProviderError enum
├── provider.rs      # AIProvider trait + types
├── registry.rs      # ProviderRegistry
└── ollama/
    ├── mod.rs
    ├── provider.rs  # OllamaProvider implementation
    └── types.rs     # Ollama API types
```

## Core Abstraction

### AIProvider Trait

```rust
// src/services/ai/provider.rs

#[async_trait]
pub trait AIProvider: Send + Sync {
    /// Returns the unique name of this provider.
    fn name(&self) -> &str;

    /// Checks the health/availability of the provider.
    async fn check_health(&self) -> AIProviderResult<HealthStatus>;

    /// Lists available models from this provider.
    async fn list_models(&self, vision_only: bool) -> AIProviderResult<Vec<ModelInfo>>;

    /// Analyzes an image using the specified model.
    async fn analyze_image(&self, request: AnalyzeImageRequest) -> AIProviderResult<AnalyzeImageResponse>;
}
```

### Common Types

```rust
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub supports_vision: bool,
    pub provider: String,
}

pub struct AnalyzeImageRequest {
    pub model: String,
    pub image_base64: String,
    pub prompt: String,
}

pub struct AnalyzeImageResponse {
    pub text: String,
    pub model: String,
    pub tokens_used: Option<TokenUsage>,
}

pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

pub struct HealthStatus {
    pub healthy: bool,
    pub message: Option<String>,
    pub available_models: Option<usize>,
}
```

### Error Types

```rust
pub enum AIProviderError {
    Unavailable { provider: String, message: String },
    RequestFailed { provider: String, message: String },
    InvalidResponse { provider: String, message: String },
    ModelNotFound { provider: String, model: String },
    VisionNotSupported { provider: String, model: String },
    ConfigurationError { provider: String, message: String },
    Timeout { provider: String, timeout_secs: u64 },
    ImageError { message: String },
}
```

## Design Principles

1. **Single Responsibility**: Photometoria handles workflow, providers handle AI inference
2. **Open/Closed**: Easy to add new providers without modifying core logic
3. **Dependency Inversion**: Depend on abstractions (trait), not concrete implementations
4. **Configuration over Code**: Provider selection via configuration file

## Provider Implementations

### 1. Ollama Provider (Local) - Implemented

**Implementation:**

```rust
// src/services/ai/ollama/provider.rs

pub struct OllamaProvider {
    name: String,
    base_url: String,
    timeout: Duration,
    models: HashMap<String, OllamaModelConfig>,
    client: reqwest::Client,
}

#[async_trait]
impl AIProvider for OllamaProvider {
    fn name(&self) -> &str {
        &self.name
    }

    async fn check_health(&self) -> AIProviderResult<HealthStatus> {
        // GET {base_url}/api/tags
        // Returns HealthStatus with available model count
    }

    async fn list_models(&self, vision_only: bool) -> AIProviderResult<Vec<ModelInfo>> {
        // Returns configured models, optionally filtered by vision support
    }

    async fn analyze_image(&self, request: AnalyzeImageRequest) -> AIProviderResult<AnalyzeImageResponse> {
        // POST {base_url}/api/generate
        // Sends image as base64, returns generated text
    }
}
```

**Configuration:**

```toml
[ai]
default_provider = "ollama"

[ai.providers.ollama]
type = "ollama"
base_url = "http://localhost:11434"
timeout_seconds = 120
devices = []
max_workers = 2

[ai.providers.ollama.models.qwen3-vl]
ollama_model = "qwen3-vl:8b"
description = "Best quality, slower"
supports_vision = true

[ai.providers.ollama.models.llava]
ollama_model = "llava:latest"
description = "Faster, good for testing"
supports_vision = true
```

**Characteristics:**
- ✅ Free and local (privacy)
- ✅ Easy setup
- ✅ Good vision model support
- ✅ No usage costs
- ⚠️ Requires local GPU
- ⚠️ Moderate performance (1-2 min/photo)

### 2. OpenAI Provider (Cloud) - Planned

**Implementation:**

```rust
// src/services/ai/openai/provider.rs

pub struct OpenAIProvider {
    name: String,
    api_key: String,
    base_url: String,      // Allows LocalAI compatibility
    timeout: Duration,
    models: HashMap<String, OpenAIModelConfig>,
    client: reqwest::Client,
}

#[async_trait]
impl AIProvider for OpenAIProvider {
    fn name(&self) -> &str {
        &self.name
    }

    async fn check_health(&self) -> AIProviderResult<HealthStatus> {
        // GET {base_url}/v1/models
        // Verify API key and connectivity
    }

    async fn list_models(&self, vision_only: bool) -> AIProviderResult<Vec<ModelInfo>> {
        // Return configured models with vision support info
    }

    async fn analyze_image(&self, request: AnalyzeImageRequest) -> AIProviderResult<AnalyzeImageResponse> {
        // POST {base_url}/v1/chat/completions
        // Format: OpenAI's chat completions API with vision
        // Include base64-encoded image in messages
    }
}
```

**Configuration:**

```toml
[ai.providers.openai]
type = "openai"
api_key = "${OPENAI_API_KEY}"  # From environment variable
base_url = "https://api.openai.com/v1"
timeout_seconds = 60
max_cost_per_photo = 0.05      # Safety limit

[ai.providers.openai.models.gpt-4o]
openai_model = "gpt-4o"
description = "Best quality, fast"
supports_vision = true

[ai.providers.openai.models.gpt-4-vision]
openai_model = "gpt-4-vision-preview"
description = "Previous generation vision model"
supports_vision = true
```

**Characteristics:**
- ✅ Fast inference (cloud GPUs)
- ✅ Excellent vision quality
- ✅ No local GPU needed
- ✅ Scalable
- ❌ Costs money (~$0.01-0.03 per photo)
- ❌ Privacy concerns (data sent to cloud)
- ⚠️ Requires internet connection

### 3. LocalAI Provider (Self-hosted OpenAI-compatible) - Planned

**Implementation:**

```rust
// src/services/ai/localai/provider.rs

// LocalAI uses OpenAI-compatible API, so we can reuse OpenAIProvider!
// Just configure with a different base_url and no API key
pub type LocalAIProvider = OpenAIProvider;
```

**Configuration:**

```toml
[ai.providers.localai]
type = "openai"  # Reuses OpenAI provider type
base_url = "http://localhost:8080/v1"
api_key = ""  # No API key needed for LocalAI
timeout_seconds = 120

[ai.providers.localai.models.llava]
openai_model = "llava-1.5-7b"
description = "Local LLaVA model"
supports_vision = true
```

**Characteristics:**
- ✅ OpenAI-compatible API
- ✅ Self-hosted (privacy)
- ✅ Free to use
- ⚠️ Requires Docker setup
- ⚠️ Moderate performance
- ⚠️ Vision support depends on models available

### 4. Anthropic Provider (Cloud) - Planned

**Implementation:**

```rust
// src/services/ai/anthropic/provider.rs

pub struct AnthropicProvider {
    name: String,
    api_key: String,
    timeout: Duration,
    models: HashMap<String, AnthropicModelConfig>,
    client: reqwest::Client,
}

#[async_trait]
impl AIProvider for AnthropicProvider {
    fn name(&self) -> &str {
        &self.name
    }

    async fn check_health(&self) -> AIProviderResult<HealthStatus> {
        // Verify API key with a simple request
    }

    async fn list_models(&self, vision_only: bool) -> AIProviderResult<Vec<ModelInfo>> {
        // Return configured models
    }

    async fn analyze_image(&self, request: AnalyzeImageRequest) -> AIProviderResult<AnalyzeImageResponse> {
        // POST https://api.anthropic.com/v1/messages
        // Format: Anthropic's messages API with vision
        // Include base64-encoded image in content blocks
    }
}
```

**Configuration:**

```toml
[ai.providers.anthropic]
type = "anthropic"
api_key = "${ANTHROPIC_API_KEY}"
timeout_seconds = 60
max_cost_per_photo = 0.05

[ai.providers.anthropic.models.claude-sonnet]
anthropic_model = "claude-3-5-sonnet-20241022"
description = "Best balance of speed and quality"
supports_vision = true

[ai.providers.anthropic.models.claude-opus]
anthropic_model = "claude-3-opus-20240229"
description = "Highest quality, slower"
supports_vision = true
```

**Characteristics:**
- ✅ Excellent vision quality
- ✅ Fast inference
- ✅ Good context understanding
- ❌ Costs money (~$0.01-0.02 per photo)
- ❌ Privacy concerns (cloud)
- ⚠️ Requires internet connection

### 5. Google Vertex AI Provider (Cloud) - Planned

**Implementation sketch:**

```rust
// src/services/ai/google/provider.rs

pub struct GoogleProvider {
    name: String,
    project_id: String,
    location: String,
    credentials: GoogleCredentials,
    timeout: Duration,
    models: HashMap<String, GoogleModelConfig>,
    client: reqwest::Client,
}

#[async_trait]
impl AIProvider for GoogleProvider {
    fn name(&self) -> &str {
        &self.name
    }

    async fn check_health(&self) -> AIProviderResult<HealthStatus> {
        // Verify credentials and connectivity
    }

    async fn list_models(&self, vision_only: bool) -> AIProviderResult<Vec<ModelInfo>> {
        // Return configured Gemini models
    }

    async fn analyze_image(&self, request: AnalyzeImageRequest) -> AIProviderResult<AnalyzeImageResponse> {
        // POST to Vertex AI Gemini endpoint
    }
}
```

**Configuration:**

```toml
[ai.providers.google]
type = "google"
project_id = "my-project"
location = "us-central1"
credentials_path = "/path/to/service-account.json"
timeout_seconds = 60

[ai.providers.google.models.gemini-pro]
google_model = "gemini-pro-vision"
description = "Google's Gemini Pro Vision model"
supports_vision = true
```

## Provider Registry

The `ProviderRegistry` manages multiple AI providers and provides a unified interface for accessing them.

```rust
// src/services/ai/registry.rs

pub struct ProviderRegistry {
    /// Map of provider name to provider instance.
    providers: HashMap<String, Arc<dyn AIProvider>>,

    /// The name of the default provider (if configured).
    default_provider_name: Option<String>,
}

impl ProviderRegistry {
    /// Creates a registry from configuration.
    pub fn from_config(config: &AIConfig) -> AIProviderResult<Self> {
        let mut registry = Self::new();

        // Register each configured provider
        for (name, provider_config) in &config.providers {
            let provider = match provider_config {
                ProviderConfig::Ollama(ollama_config) => {
                    create_ollama_provider(name.clone(), ollama_config)
                }
                // Future: ProviderConfig::OpenAI, ProviderConfig::Anthropic, etc.
            };

            registry.register(name.clone(), provider);
        }

        // Set default provider
        if let Some(default_name) = &config.default_provider {
            registry.set_default(default_name)?;
        } else if registry.len() == 1 {
            // Auto-select if only one provider configured
            let name = registry.provider_names()[0].to_string();
            registry.set_default(&name)?;
        }

        Ok(registry)
    }

    /// Gets a provider by name.
    pub fn get(&self, name: &str) -> AIProviderResult<Arc<dyn AIProvider>>;

    /// Gets the default provider.
    pub fn default_provider(&self) -> AIProviderResult<Arc<dyn AIProvider>>;

    /// Returns all registered provider names.
    pub fn provider_names(&self) -> Vec<&str>;
}
```

## Integration with Worker Pool

The Worker Pool (Issue #8) will use the `ProviderRegistry` to access AI providers.

```rust
// src/services/worker.rs (planned)

pub struct Worker {
    provider: Arc<dyn AIProvider>,
    // ... other fields
}

impl Worker {
    pub async fn process_photo(
        &self,
        photo: &Photo,
        context: &str,
    ) -> Result<PhotoResult, WorkerError> {
        // Load and encode image
        let image_data = tokio::fs::read(&photo.path).await?;
        let image_base64 = base64::engine::general_purpose::STANDARD.encode(&image_data);

        // Prepare request
        let request = AnalyzeImageRequest {
            model: self.model_id.clone(),
            image_base64,
            prompt: format!("Context: {}\n\nGenerate comma-separated tags for this image.", context),
        };

        // Call provider
        let response = self.provider
            .analyze_image(request)
            .await
            .map_err(WorkerError::ProviderError)?;

        Ok(PhotoResult {
            photo_id: photo.photo_id.clone(),
            status: ResultStatus::Completed,
            tags: Some(parse_tags(&response.text)),
            tokens_used: response.tokens_used,
            error: None,
            processed_at: Some(Utc::now()),
        })
    }
}
```

## Configuration Structure

### Complete config.toml Example

```toml
[server]
host = "0.0.0.0"
port = 8080

[storage]
path = "/var/photometoria/storage"
max_size = "100GiB"

[upload]
max_photos_per_request = 50
max_photo_size = "20MB"

# AI Provider Configuration
[ai]
default_provider = "ollama"  # Which provider to use by default

# Ollama provider (local)
[ai.providers.ollama]
type = "ollama"
base_url = "http://localhost:11434"
timeout_seconds = 120
devices = []        # GPU device IDs (empty = auto-detect)
max_workers = 2     # Concurrent workers for job processing

[ai.providers.ollama.models.qwen3-vl]
ollama_model = "qwen3-vl:8b"
description = "Best quality, slower"
supports_vision = true
prompt_template = "Analyze this image and generate descriptive keywords..."

[ai.providers.ollama.models.llava]
ollama_model = "llava:latest"
description = "Faster, good for testing"
supports_vision = true

# OpenAI provider (cloud) - example for future use
# [ai.providers.openai]
# type = "openai"
# api_key = "${OPENAI_API_KEY}"
# base_url = "https://api.openai.com/v1"
# timeout_seconds = 60
# max_cost_per_photo = 0.05
#
# [ai.providers.openai.models.gpt-4o]
# openai_model = "gpt-4o"
# description = "Best quality, fast"
# supports_vision = true

# Anthropic provider (cloud) - example for future use
# [ai.providers.anthropic]
# type = "anthropic"
# api_key = "${ANTHROPIC_API_KEY}"
# timeout_seconds = 60
# max_cost_per_photo = 0.05
#
# [ai.providers.anthropic.models.claude-sonnet]
# anthropic_model = "claude-3-5-sonnet-20241022"
# description = "Best balance of speed and quality"
# supports_vision = true
```

### Environment Variables

```bash
# For cloud providers, use environment variables for API keys
export OPENAI_API_KEY="sk-..."
export ANTHROPIC_API_KEY="sk-ant-..."
export GOOGLE_APPLICATION_CREDENTIALS="/path/to/credentials.json"
```

## Provider Comparison Matrix

| Provider | Type | Privacy | Cost | Performance | Setup | Vision Quality | Best For |
|----------|------|---------|------|-------------|-------|----------------|----------|
| **Ollama** | Local | ✅ Excellent | ✅ Free | ✅ Good | ✅ Easy | ✅ Excellent | Privacy-conscious users, hobby projects |
| **LocalAI** | Self-hosted | ✅ Excellent | ✅ Free | ⚠️ Moderate | ⚠️ Docker | ⚠️ Good | Self-hosted with OpenAI compatibility |
| **vLLM** | Self-hosted | ✅ Excellent | ✅ Free | ✅ Excellent | ❌ Complex | ⚠️ Experimental | High-performance text (vision limited) |
| **OpenAI** | Cloud | ❌ Poor | ❌ ~$0.01-0.03 | 🚀 Excellent | ✅ Easy | ✅ Excellent | Production, speed critical |
| **Anthropic** | Cloud | ❌ Poor | ❌ ~$0.01-0.02 | 🚀 Excellent | ✅ Easy | 🚀 Best | Best quality, understanding context |
| **Google** | Cloud | ❌ Poor | ❌ ~$0.005-0.02 | 🚀 Excellent | ⚠️ GCP setup | ✅ Excellent | GCP users, cost-conscious |

## Advanced Features (Future)

### 1. Fallback Provider Chain

```rust
pub struct FallbackProvider {
    providers: Vec<Arc<dyn AIProvider>>,
    max_retries: usize,
}

#[async_trait]
impl AIProvider for FallbackProvider {
    fn name(&self) -> &str {
        "fallback"
    }

    async fn check_health(&self) -> AIProviderResult<HealthStatus> {
        // Check if at least one provider is healthy
        for provider in &self.providers {
            if let Ok(status) = provider.check_health().await {
                if status.healthy {
                    return Ok(status);
                }
            }
        }
        Ok(HealthStatus { healthy: false, message: Some("All providers unhealthy".into()), available_models: None })
    }

    async fn list_models(&self, vision_only: bool) -> AIProviderResult<Vec<ModelInfo>> {
        // Aggregate models from all providers
        let mut all_models = Vec::new();
        for provider in &self.providers {
            if let Ok(models) = provider.list_models(vision_only).await {
                all_models.extend(models);
            }
        }
        Ok(all_models)
    }

    async fn analyze_image(&self, request: AnalyzeImageRequest) -> AIProviderResult<AnalyzeImageResponse> {
        let mut last_error = None;

        for provider in &self.providers {
            match provider.analyze_image(request.clone()).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    tracing::warn!(
                        "Provider {} failed: {}. Trying next...",
                        provider.name(),
                        e
                    );
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(||
            AIProviderError::Unavailable {
                provider: "fallback".to_string(),
                message: "All providers failed".to_string(),
            }
        ))
    }
}
```

Configuration:

```toml
[ai]
default_provider = "fallback"

[ai.providers.fallback]
type = "fallback"
providers = ["ollama", "openai"]  # Try ollama first, fallback to openai
```

### 2. Cost Tracking and Budgets

```rust
pub struct CostTracker {
    total_spent: Arc<Mutex<f64>>,
    daily_budget: f64,
    monthly_budget: f64,
}

impl CostTracker {
    pub async fn check_budget(&self, estimated_cost: f64) -> Result<(), BudgetError> {
        let spent = self.total_spent.lock().await;
        if *spent + estimated_cost > self.daily_budget {
            return Err(BudgetError::DailyBudgetExceeded);
        }
        Ok(())
    }
    
    pub async fn record_usage(&self, actual_cost: f64) {
        let mut spent = self.total_spent.lock().await;
        *spent += actual_cost;
    }
}
```

### 3. A/B Testing and Quality Comparison

```rust
pub async fn compare_providers(
    providers: Vec<Box<dyn VisionProvider>>,
    test_images: Vec<Photo>,
) -> ComparisonReport {
    let mut results = HashMap::new();
    
    for provider in providers {
        let mut provider_results = vec![];
        
        for photo in &test_images {
            let start = Instant::now();
            let response = provider.analyze_image(/* ... */).await;
            let duration = start.elapsed();
            
            provider_results.push(TestResult {
                photo_id: photo.id.clone(),
                tags: response.tags,
                duration,
                cost: provider.estimate_cost(/* ... */),
            });
        }
        
        results.insert(provider.name(), provider_results);
    }
    
    ComparisonReport { results }
}
```

### 4. Mock Provider for Testing

```rust
// src/services/ai/mock/provider.rs (for testing)

pub struct MockProvider {
    name: String,
    responses: HashMap<String, AnalyzeImageResponse>,
    delay: Option<Duration>,
    health_status: HealthStatus,
}

#[async_trait]
impl AIProvider for MockProvider {
    fn name(&self) -> &str {
        &self.name
    }

    async fn check_health(&self) -> AIProviderResult<HealthStatus> {
        Ok(self.health_status.clone())
    }

    async fn list_models(&self, _vision_only: bool) -> AIProviderResult<Vec<ModelInfo>> {
        Ok(vec![ModelInfo {
            id: "mock-model".to_string(),
            name: "Mock Model".to_string(),
            description: Some("For testing".to_string()),
            supports_vision: true,
            provider: self.name.clone(),
        }])
    }

    async fn analyze_image(&self, request: AnalyzeImageRequest) -> AIProviderResult<AnalyzeImageResponse> {
        // Simulate delay
        if let Some(delay) = self.delay {
            tokio::time::sleep(delay).await;
        }

        // Return pre-configured response or default
        self.responses
            .get(&request.model)
            .cloned()
            .ok_or_else(|| AIProviderError::ModelNotFound {
                provider: self.name.clone(),
                model: request.model,
            })
    }
}
```

## API Impact

### GET /api/models

Returns available models from the configured provider:

```json
{
  "provider": "ollama",
  "models": [
    {
      "id": "qwen3-vl:8b",
      "name": "Qwen2-VL 8B",
      "description": "Best quality, slower processing",
      "supports_vision": true,
      "context_window": 8192
    },
    {
      "id": "llava",
      "name": "LLaVA",
      "description": "Faster, good for testing",
      "supports_vision": true,
      "context_window": 4096
    }
  ]
}
```

### POST /api/tasks/{task_id}/jobs

Job creation now includes provider info:

```json
{
  "model": "qwen3-vl:8b",
  "photo_ids": null,
  "estimated_cost": 0.0  // For cloud providers
}
```

Response includes provider metadata:

```json
{
  "job_id": "job_xyz",
  "provider": "ollama",
  "model": "qwen3-vl:8b",
  "estimated_cost": 0.0,
  "status": "queued"
}
```

### GET /api/jobs/{job_id}/results

Results include usage information:

```json
{
  "job_id": "job_xyz",
  "provider": "openai",
  "model": "gpt-4o",
  "results": [
    {
      "photo_id": "p1",
      "status": "completed",
      "tags": "sunset, beach, ocean",
      "usage": {
        "prompt_tokens": 1250,
        "completion_tokens": 45,
        "total_tokens": 1295
      },
      "cost": 0.0194
    }
  ],
  "total_cost": 0.584  // For all photos in job
}
```

## Implementation Roadmap

### Phase 1: Core Abstraction ✅ Complete
**Goal:** Establish the provider abstraction layer

**Completed:**
1. ✅ Define `AIProvider` trait with `check_health`, `list_models`, `analyze_image`
2. ✅ Implement `OllamaProvider` with full Ollama API integration
3. ✅ Update configuration system with `[ai]` section
4. ✅ Implement `ProviderRegistry` for managing multiple providers
5. ✅ Add unit tests and integration tests with WireMock
6. ✅ Integrate registry into `AppState`

**Outcome:** Ollama works through abstraction layer, ready for additional providers

### Phase 2: OpenAI-Compatible Providers (Planned)
**Goal:** Support cloud and self-hosted OpenAI-compatible providers

**Tasks:**
1. Implement `OpenAIProvider` using chat completions API
2. Add LocalAI support (reuses OpenAI provider with different base_url)
3. Add API key management via environment variables
4. Implement cost estimation (optional trait method)
5. Add safety limits (max_cost_per_photo config)
6. Update documentation with setup instructions

**Outcome:** Users can choose Ollama (local) or OpenAI/LocalAI

### Phase 3: Additional Cloud Providers (Planned)
**Goal:** Support major cloud vision APIs

**Tasks:**
1. Implement `AnthropicProvider` using Messages API
2. Implement `GoogleProvider` for Gemini (optional)
3. Add provider comparison tooling
4. Implement cost tracking
5. Add budget controls (daily/monthly limits)

**Outcome:** Full choice of providers for different use cases

### Phase 4: Advanced Features (Future)
**Goal:** Production-ready features

**Tasks:**
1. Implement fallback provider chain
2. Add retry with exponential backoff for cloud providers
3. Add caching layer (avoid re-analyzing same photos)
4. Metrics and monitoring (success rate, latency, cost)
5. Provider health monitoring and auto-failover

**Outcome:** Robust, production-ready system

## Migration Guide

### From Design Document to Implementation

The original design used a `ProviderFactory` pattern. The actual implementation uses a `ProviderRegistry` which provides more flexibility for managing multiple providers simultaneously.

**Design vs Implementation:**

| Design Document | Actual Implementation |
|----------------|----------------------|
| `VisionProvider` trait | `AIProvider` trait |
| `VisionRequest` | `AnalyzeImageRequest` |
| `VisionResponse` | `AnalyzeImageResponse` |
| `ProviderFactory::create()` | `ProviderRegistry::from_config()` |
| `Box<dyn VisionProvider>` | `Arc<dyn AIProvider>` |
| `src/providers/` | `src/services/ai/` |
| `[provider]` config | `[ai]` config |

**Configuration Migration:**

```toml
# Old design (not implemented)
[provider]
type = "ollama"

[provider.ollama]
base_url = "http://localhost:11434"
default_model = "qwen3-vl:8b"

# Actual implementation
[ai]
default_provider = "ollama"

[ai.providers.ollama]
type = "ollama"
base_url = "http://localhost:11434"
timeout_seconds = 120

[ai.providers.ollama.models.qwen3-vl]
ollama_model = "qwen3-vl:8b"
supports_vision = true
```

## Testing Strategy

### Unit Tests

Unit tests are included in each module and use mock HTTP responses via WireMock.

```rust
// src/services/ai/registry.rs - tests
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_from_config() {
        let config = create_test_config();
        let registry = ProviderRegistry::from_config(&config).unwrap();

        assert_eq!(registry.len(), 1);
        assert_eq!(registry.default_provider_name(), Some("ollama"));
        assert!(registry.get("ollama").is_ok());
    }

    #[test]
    fn test_registry_get_nonexistent() {
        let registry = ProviderRegistry::new();
        let result = registry.get("nonexistent");
        assert!(result.is_err());
    }
}
```

### Integration Tests

Integration tests use WireMock to simulate Ollama API responses.

```rust
// tests/ai_provider_tests.rs
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path};

#[tokio::test]
async fn test_ollama_health_check() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "models": [{"name": "llava:latest"}]
        })))
        .mount(&mock_server)
        .await;

    let provider = create_test_provider(&mock_server.uri());
    let health = provider.check_health().await.unwrap();

    assert!(health.healthy);
    assert_eq!(health.available_models, Some(1));
}

#[tokio::test]
async fn test_ollama_analyze_image() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/generate"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "response": "sunset, beach, ocean, waves",
            "done": true
        })))
        .mount(&mock_server)
        .await;

    let provider = create_test_provider(&mock_server.uri());
    let request = AnalyzeImageRequest {
        model: "test-model".to_string(),
        image_base64: "dGVzdA==".to_string(), // "test" in base64
        prompt: "Generate tags".to_string(),
    };

    let response = provider.analyze_image(request).await.unwrap();
    assert_eq!(response.text, "sunset, beach, ocean, waves");
}
```

## Documentation Requirements

### User Documentation

1. **Provider Comparison Guide**: Help users choose the right provider
2. **Setup Instructions**: Step-by-step for each provider
3. **Cost Calculator**: Estimate costs for batch operations
4. **Troubleshooting**: Common issues with each provider

### Developer Documentation

1. **Provider Implementation Guide**: How to add new providers
2. **Testing Guide**: How to test providers
3. **API Reference**: Full trait documentation
4. **Architecture Diagrams**: Show data flow

## Security Considerations

### API Key Management

- ✅ Use environment variables, never commit keys
- ✅ Support key rotation
- ✅ Log key usage (redacted)
- ✅ Validate keys at startup

### Data Privacy

- ✅ Clearly document which providers send data to cloud
- ✅ Allow users to explicitly opt-in to cloud providers
- ✅ Provide privacy comparison matrix
- ✅ Consider data retention policies of cloud providers

### Cost Controls

- ✅ Hard limits on cost per photo
- ✅ Daily/monthly budget enforcement
- ✅ Alerts when approaching budget
- ✅ Graceful degradation to local provider if budget exceeded

## Conclusion

The provider abstraction layer enables Photometoria to:

1. **Support multiple AI providers** without coupling to any single implementation
2. **Give users choice** between privacy (local), cost (free), and performance (cloud)
3. **Stay future-proof** as new providers and models emerge
4. **Maintain simplicity** while supporting advanced use cases
5. **Enable testing** with mock providers and WireMock
6. **Track costs** and enforce budgets for paid providers (planned)

The abstraction follows software engineering best practices (SOLID principles) while remaining pragmatic and easy to implement incrementally.

**Current status:** Phase 1 complete with Ollama provider. The system is ready for additional providers to be added in Phase 2.

---

*Last updated: Issue #7 implementation complete.*
