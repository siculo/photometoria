# AI Provider Abstraction Layer - Design Document

## Overview

This document describes the design for an abstraction layer that allows Photometoria to work with multiple AI vision providers, both local and cloud-based. The abstraction enables users to choose the provider that best fits their needs (privacy, cost, performance) without changing the core application logic.

## Motivation

**Problems with tight coupling to a specific provider:**
- Locked into one implementation (Ollama)
- Cannot easily test alternatives
- Difficult to support different use cases (hobbyist vs. enterprise)
- Must manage AI infrastructure concerns within Photometoria

**Benefits of abstraction:**
- **Separation of concerns**: Photometoria handles workflow, providers handle AI inference
- **Flexibility**: Users choose their preferred provider
- **Future-proof**: Easy to add new providers as they emerge
- **Testability**: Mock providers for unit tests
- **Graceful degradation**: Fallback to alternative providers

## Design Principles

1. **Single Responsibility**: Photometoria manages photo workflows, providers manage AI inference
2. **Open/Closed**: Easy to add new providers without modifying core logic
3. **Dependency Inversion**: Depend on abstractions (trait), not concrete implementations
4. **Configuration over Code**: Provider selection via configuration file

## Core Abstraction

### VisionProvider Trait

```rust
// src/providers/mod.rs

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionRequest {
    pub image_data: Vec<u8>,          // Raw image bytes
    pub prompt: String,               // Base prompt for tag generation
    pub context: Option<String>,      // User-provided context hints
    pub temperature: f32,             // Sampling temperature
    pub max_tokens: u32,              // Maximum response length
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionResponse {
    pub tags: String,                 // Comma-separated tags
    pub raw_response: Option<String>, // Full response for debugging
    pub usage: Option<TokenUsage>,    // Token usage (for billing tracking)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub context_window: Option<u32>,
    pub supports_vision: bool,
}

/// Main trait for AI vision providers
#[async_trait]
pub trait VisionProvider: Send + Sync {
    /// Name of the provider (for logging/config)
    fn name(&self) -> &str;
    
    /// Check if provider is available and healthy
    async fn health_check(&self) -> Result<bool, ProviderError>;
    
    /// List available models from this provider
    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError>;
    
    /// Analyze image and generate tags
    async fn analyze_image(
        &self,
        request: VisionRequest
    ) -> Result<VisionResponse, ProviderError>;
    
    /// Estimate cost for a request (optional, for paid providers)
    fn estimate_cost(&self, request: &VisionRequest) -> Option<f64> {
        None // Default: no cost for local providers
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("Connection error: {0}")]
    Connection(String),
    
    #[error("Authentication error: {0}")]
    Auth(String),
    
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
    
    #[error("Model not found: {0}")]
    ModelNotFound(String),
    
    #[error("Rate limit exceeded")]
    RateLimit,
    
    #[error("Timeout: {0}")]
    Timeout(String),
    
    #[error("Provider error: {0}")]
    Other(String),
}
```

## Provider Implementations

### 1. Ollama Provider (Local)

**Implementation:**

```rust
// src/providers/ollama.rs

pub struct OllamaProvider {
    base_url: String,
    default_model: String,
    client: reqwest::Client,
}

impl OllamaProvider {
    pub fn new(base_url: String, default_model: String) -> Self {
        Self {
            base_url,
            default_model,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl VisionProvider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }
    
    async fn health_check(&self) -> Result<bool, ProviderError> {
        // GET {base_url}/api/tags
        let response = self.client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await
            .map_err(|e| ProviderError::Connection(e.to_string()))?;
        
        Ok(response.status().is_success())
    }
    
    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        // Parse Ollama's /api/tags response
        // Filter for vision-capable models
        // ...
    }
    
    async fn analyze_image(
        &self,
        request: VisionRequest
    ) -> Result<VisionResponse, ProviderError> {
        // POST {base_url}/api/generate
        // Format: Ollama's generate API with image
        // Parse response and extract tags
        // ...
    }
}
```

**Configuration:**

```toml
[provider]
type = "ollama"

[provider.ollama]
base_url = "http://localhost:11434"
default_model = "qwen2-vl:8b"
models = [
    { name = "qwen2-vl:8b", description = "Best quality, slower" },
    { name = "llava", description = "Faster, good for testing" }
]
```

**Characteristics:**
- ✅ Free and local (privacy)
- ✅ Easy setup
- ✅ Good vision model support
- ✅ No usage costs
- ⚠️ Requires local GPU
- ⚠️ Moderate performance (1-2 min/photo)

### 2. OpenAI Provider (Cloud)

**Implementation:**

```rust
// src/providers/openai.rs

pub struct OpenAIProvider {
    api_key: String,
    base_url: String,      // Allows LocalAI compatibility
    default_model: String,
    client: reqwest::Client,
}

#[async_trait]
impl VisionProvider for OpenAIProvider {
    fn name(&self) -> &str {
        "openai"
    }
    
    async fn analyze_image(
        &self,
        request: VisionRequest
    ) -> Result<VisionResponse, ProviderError> {
        // POST {base_url}/v1/chat/completions
        // Format: OpenAI's chat completions API with vision
        // Include base64-encoded image in messages
        // ...
    }
    
    fn estimate_cost(&self, request: &VisionRequest) -> Option<f64> {
        // GPT-4 Vision pricing (approximate)
        // Input: ~$0.01 per 1K tokens
        // Output: ~$0.03 per 1K tokens
        let estimated_tokens = (request.image_data.len() / 1024) as f64 * 0.5;
        Some((estimated_tokens / 1000.0) * 0.01 + 
             (request.max_tokens as f64 / 1000.0) * 0.03)
    }
}
```

**Configuration:**

```toml
[provider]
type = "openai"

[provider.openai]
api_key = "${OPENAI_API_KEY}"  # From environment variable
base_url = "https://api.openai.com/v1"
default_model = "gpt-4o"
models = ["gpt-4o", "gpt-4-vision-preview"]
max_cost_per_photo = 0.05      # Safety limit
```

**Characteristics:**
- ✅ Fast inference (cloud GPUs)
- ✅ Excellent vision quality
- ✅ No local GPU needed
- ✅ Scalable
- ❌ Costs money (~$0.01-0.03 per photo)
- ❌ Privacy concerns (data sent to cloud)
- ⚠️ Requires internet connection

### 3. LocalAI Provider (Self-hosted OpenAI-compatible)

**Implementation:**

```rust
// src/providers/localai.rs

// LocalAI uses OpenAI-compatible API, so we can reuse OpenAIProvider!
pub type LocalAIProvider = OpenAIProvider;

impl LocalAIProvider {
    pub fn new_localai(base_url: String, model: String) -> Self {
        Self {
            api_key: String::new(), // No API key needed
            base_url,
            default_model: model,
            client: reqwest::Client::new(),
        }
    }
}
```

**Configuration:**

```toml
[provider]
type = "localai"

[provider.localai]
base_url = "http://localhost:8080/v1"
default_model = "llava-1.5-7b"
models = ["llava-1.5-7b", "llava-1.6-mistral-7b"]
```

**Characteristics:**
- ✅ OpenAI-compatible API
- ✅ Self-hosted (privacy)
- ✅ Free to use
- ⚠️ Requires Docker setup
- ⚠️ Moderate performance
- ⚠️ Vision support depends on models available

### 4. Anthropic Provider (Cloud)

**Implementation:**

```rust
// src/providers/anthropic.rs

pub struct AnthropicProvider {
    api_key: String,
    default_model: String,
    client: reqwest::Client,
}

#[async_trait]
impl VisionProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }
    
    async fn analyze_image(
        &self,
        request: VisionRequest
    ) -> Result<VisionResponse, ProviderError> {
        // POST https://api.anthropic.com/v1/messages
        // Format: Anthropic's messages API with vision
        // Include base64-encoded image in content blocks
        // ...
    }
    
    fn estimate_cost(&self, request: &VisionRequest) -> Option<f64> {
        // Claude pricing
        // Input: ~$0.003-0.015 per 1K tokens (model dependent)
        // Output: ~$0.015-0.075 per 1K tokens
        // ...
    }
}
```

**Configuration:**

```toml
[provider]
type = "anthropic"

[provider.anthropic]
api_key = "${ANTHROPIC_API_KEY}"
default_model = "claude-3-5-sonnet-20241022"
models = [
    "claude-3-5-sonnet-20241022",
    "claude-3-opus-20240229"
]
max_cost_per_photo = 0.05
```

**Characteristics:**
- ✅ Excellent vision quality
- ✅ Fast inference
- ✅ Good context understanding
- ❌ Costs money (~$0.01-0.02 per photo)
- ❌ Privacy concerns (cloud)
- ⚠️ Requires internet connection

### 5. Google Vertex AI Provider (Cloud)

**Implementation sketch:**

```rust
// src/providers/google.rs

pub struct GoogleProvider {
    project_id: String,
    location: String,
    credentials: GoogleCredentials,
    default_model: String,
}

// Implementation details for Google Cloud Vision API or Gemini
```

**Configuration:**

```toml
[provider]
type = "google"

[provider.google]
project_id = "my-project"
location = "us-central1"
credentials_path = "/path/to/service-account.json"
default_model = "gemini-pro-vision"
```

## Provider Factory

```rust
// src/providers/factory.rs

pub struct ProviderFactory;

impl ProviderFactory {
    /// Create a provider based on configuration
    pub async fn create(config: &Config) -> Result<Box<dyn VisionProvider>, ProviderError> {
        match config.provider.provider_type.as_str() {
            "ollama" => {
                let provider = OllamaProvider::new(
                    config.provider.ollama.base_url.clone(),
                    config.provider.ollama.default_model.clone(),
                );
                
                // Verify provider is available
                provider.health_check().await?;
                
                Ok(Box::new(provider))
            }
            
            "openai" => {
                let provider = OpenAIProvider::new(
                    config.provider.openai.api_key.clone(),
                    config.provider.openai.base_url.clone(),
                    config.provider.openai.default_model.clone(),
                );
                Ok(Box::new(provider))
            }
            
            "localai" => {
                let provider = OpenAIProvider::new_localai(
                    config.provider.localai.base_url.clone(),
                    config.provider.localai.default_model.clone(),
                );
                Ok(Box::new(provider))
            }
            
            "anthropic" => {
                let provider = AnthropicProvider::new(
                    config.provider.anthropic.api_key.clone(),
                    config.provider.anthropic.default_model.clone(),
                );
                Ok(Box::new(provider))
            }
            
            "google" => {
                let provider = GoogleProvider::new(
                    config.provider.google.project_id.clone(),
                    config.provider.google.location.clone(),
                    config.provider.google.credentials_path.clone(),
                    config.provider.google.default_model.clone(),
                );
                Ok(Box::new(provider))
            }
            
            _ => Err(ProviderError::InvalidRequest(
                format!("Unknown provider type: {}", config.provider.provider_type)
            ))
        }
    }
    
    /// Create provider with fallback chain
    pub async fn create_with_fallback(
        config: &Config
    ) -> Result<Box<dyn VisionProvider>, ProviderError> {
        let primary = Self::create(config).await?;
        
        // TODO: Implement fallback wrapper that tries multiple providers
        Ok(primary)
    }
}
```

## Integration with Worker Pool

```rust
// src/services/worker.rs

pub struct Worker {
    provider: Arc<dyn VisionProvider>,
    // ... other fields
}

impl Worker {
    pub async fn process_photo(
        &self,
        photo: &Photo,
        context: &str,
    ) -> Result<PhotoResult, WorkerError> {
        // Load image
        let image_data = tokio::fs::read(&photo.path).await?;
        
        // Prepare request
        let request = VisionRequest {
            image_data,
            prompt: "Generate comma-separated tags for this image.".to_string(),
            context: Some(context.to_string()),
            temperature: 0.3,
            max_tokens: 200,
        };
        
        // Cost check for paid providers
        if let Some(cost) = self.provider.estimate_cost(&request) {
            if cost > self.max_cost_per_photo {
                return Err(WorkerError::CostLimitExceeded(cost));
            }
            tracing::info!("Estimated cost: ${:.4}", cost);
        }
        
        // Call provider
        let response = self.provider
            .analyze_image(request)
            .await
            .map_err(WorkerError::ProviderError)?;
        
        Ok(PhotoResult {
            photo_id: photo.photo_id.clone(),
            status: ResultStatus::Completed,
            tags: Some(response.tags),
            usage: response.usage,
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
max_size_gb = 100

[upload]
max_photos_per_request = 50
max_photo_size_mb = 20

# Provider selection
[provider]
type = "ollama"  # Options: "ollama", "openai", "anthropic", "localai", "google"

# Ollama configuration (local)
[provider.ollama]
base_url = "http://localhost:11434"
default_model = "qwen2-vl:8b"
models = [
    { name = "qwen2-vl:8b", description = "Best quality, slower" },
    { name = "llava", description = "Faster iteration" }
]

# OpenAI configuration (cloud)
[provider.openai]
api_key = "${OPENAI_API_KEY}"
base_url = "https://api.openai.com/v1"
default_model = "gpt-4o"
models = ["gpt-4o", "gpt-4-vision-preview"]
max_cost_per_photo = 0.05

# Anthropic configuration (cloud)
[provider.anthropic]
api_key = "${ANTHROPIC_API_KEY}"
default_model = "claude-3-5-sonnet-20241022"
models = ["claude-3-5-sonnet-20241022", "claude-3-opus-20240229"]
max_cost_per_photo = 0.05

# LocalAI configuration (self-hosted)
[provider.localai]
base_url = "http://localhost:8080/v1"
default_model = "llava-1.5-7b"
models = ["llava-1.5-7b"]

# Google Vertex AI configuration (cloud)
[provider.google]
project_id = "my-gcp-project"
location = "us-central1"
credentials_path = "/path/to/service-account.json"
default_model = "gemini-pro-vision"
max_cost_per_photo = 0.05

# Worker pool configuration
[gpu]
max_workers = 2  # Based on available GPUs or desired concurrency
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

## Advanced Features

### 1. Fallback Provider Chain

```rust
pub struct FallbackProvider {
    providers: Vec<Box<dyn VisionProvider>>,
    max_retries: usize,
}

#[async_trait]
impl VisionProvider for FallbackProvider {
    async fn analyze_image(
        &self,
        request: VisionRequest
    ) -> Result<VisionResponse, ProviderError> {
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
            ProviderError::Other("All providers failed".to_string())
        ))
    }
}
```

Configuration:

```toml
[provider]
type = "fallback"

[provider.fallback]
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
// src/providers/mock.rs

pub struct MockProvider {
    responses: HashMap<String, VisionResponse>,
    delay: Option<Duration>,
}

#[async_trait]
impl VisionProvider for MockProvider {
    fn name(&self) -> &str {
        "mock"
    }
    
    async fn analyze_image(
        &self,
        request: VisionRequest
    ) -> Result<VisionResponse, ProviderError> {
        // Simulate delay
        if let Some(delay) = self.delay {
            tokio::time::sleep(delay).await;
        }
        
        // Return pre-configured response
        let photo_hash = format!("{:x}", md5::compute(&request.image_data));
        self.responses
            .get(&photo_hash)
            .cloned()
            .ok_or_else(|| ProviderError::Other("Photo not found in mock".to_string()))
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
      "id": "qwen2-vl:8b",
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
  "model": "qwen2-vl:8b",
  "photo_ids": null,
  "estimated_cost": 0.0  // For cloud providers
}
```

Response includes provider metadata:

```json
{
  "job_id": "job_xyz",
  "provider": "ollama",
  "model": "qwen2-vl:8b",
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

### Phase 1: Core Abstraction (Current)
**Goal:** Establish the provider abstraction layer

**Tasks:**
1. Define `VisionProvider` trait
2. Implement `OllamaProvider` (migrate existing code)
3. Update configuration system to support provider selection
4. Implement `ProviderFactory`
5. Update worker pool to use provider abstraction
6. Add unit tests with mock provider

**Outcome:** Ollama works through abstraction layer, no functionality lost

### Phase 2: OpenAI-Compatible Providers (Next)
**Goal:** Support cloud and self-hosted OpenAI-compatible providers

**Tasks:**
1. Implement `OpenAIProvider`
2. Add LocalAI configuration support (reuses OpenAIProvider)
3. Add API key management (environment variables)
4. Implement cost estimation and tracking
5. Add safety limits (max cost per photo)
6. Update documentation with setup instructions

**Outcome:** Users can choose Ollama (local) or OpenAI/LocalAI

### Phase 3: Additional Cloud Providers
**Goal:** Support major cloud vision APIs

**Tasks:**
1. Implement `AnthropicProvider`
2. Implement `GoogleProvider` (optional)
3. Add provider comparison tooling
4. Implement cost tracking dashboard
5. Add budget controls (daily/monthly limits)

**Outcome:** Full choice of providers for different use cases

### Phase 4: Advanced Features
**Goal:** Production-ready features

**Tasks:**
1. Implement fallback provider chain
2. Add A/B testing framework
3. Implement retry with backoff for cloud providers
4. Add caching layer (avoid re-analyzing same photos)
5. Metrics and monitoring (success rate, latency, cost)
6. Provider health monitoring

**Outcome:** Robust, production-ready system

## Migration Guide

### From Current Ollama-only to Abstraction

**Before:**
```rust
// Direct Ollama calls
let response = ollama_client
    .generate(photo, prompt)
    .await?;
```

**After:**
```rust
// Provider abstraction
let request = VisionRequest {
    image_data: photo.data,
    prompt,
    context: Some(task.context),
    temperature: 0.3,
    max_tokens: 200,
};

let response = provider
    .analyze_image(request)
    .await?;
```

**Configuration:**
```toml
# Old: implicit Ollama
[ollama]
base_url = "http://localhost:11434"

# New: explicit provider selection
[provider]
type = "ollama"

[provider.ollama]
base_url = "http://localhost:11434"
default_model = "qwen2-vl:8b"
```

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_mock_provider() {
        let mut mock = MockProvider::new();
        mock.add_response(
            "test_photo.jpg",
            VisionResponse {
                tags: "sunset, beach".to_string(),
                raw_response: None,
                usage: None,
            }
        );
        
        let request = VisionRequest {
            image_data: load_test_image("test_photo.jpg"),
            prompt: "Generate tags".to_string(),
            context: None,
            temperature: 0.3,
            max_tokens: 100,
        };
        
        let response = mock.analyze_image(request).await.unwrap();
        assert_eq!(response.tags, "sunset, beach");
    }
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_ollama_provider_integration() {
    // Requires Ollama running locally
    let provider = OllamaProvider::new(
        "http://localhost:11434".to_string(),
        "llava".to_string(),
    );
    
    // Health check
    assert!(provider.health_check().await.unwrap());
    
    // List models
    let models = provider.list_models().await.unwrap();
    assert!(!models.is_empty());
    
    // Analyze image
    let request = VisionRequest {
        image_data: load_test_image("sample.jpg"),
        prompt: "Generate tags for this image".to_string(),
        context: Some("vacation photo".to_string()),
        temperature: 0.3,
        max_tokens: 200,
    };
    
    let response = provider.analyze_image(request).await.unwrap();
    assert!(!response.tags.is_empty());
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
5. **Enable testing** with mock providers
6. **Track costs** and enforce budgets for paid providers

The abstraction follows software engineering best practices (SOLID principles) while remaining pragmatic and easy to implement incrementally.

**Recommended starting point:** Phase 1 with Ollama, then add OpenAI support in Phase 2 based on user demand.

---

*This design document should be reviewed and updated as implementation progresses and new providers emerge.*
