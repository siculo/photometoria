# Piano: Ollama Service Client con Provider Abstraction (Issue #7)

## Obiettivo
Implementare un client per Ollama con architettura a provider astratti, pronta per futuri provider AI.

## Architettura

```
┌─────────────────────────────────────────┐
│            Configurazione               │
│  [ai.providers.ollama] → OllamaConfig   │
├─────────────────────────────────────────┤
│               Trait                     │
│  AIProvider (list_models, analyze_image,│
│              check_health)              │
├─────────────────────────────────────────┤
│           Implementazione               │
│  OllamaProvider → chiama Ollama API     │
├─────────────────────────────────────────┤
│              Registry                   │
│  HashMap<String, Arc<dyn AIProvider>>   │
│  + default_provider                     │
└─────────────────────────────────────────┘
```

## File da creare

| File | Descrizione | Stato |
|------|-------------|-------|
| `src/services/ai/mod.rs` | Modulo principale e re-export | ✅ Completato |
| `src/services/ai/error.rs` | `AIProviderError` enum | ✅ Completato |
| `src/services/ai/provider.rs` | `AIProvider` trait + tipi | ✅ Completato |
| `src/services/ai/registry.rs` | `ProviderRegistry` per gestire i provider | ⏳ In corso |
| `src/services/ai/ollama/mod.rs` | Modulo Ollama | ⏳ In corso |
| `src/services/ai/ollama/provider.rs` | `OllamaProvider` implementazione | ⏳ In corso |
| `src/services/ai/ollama/types.rs` | Tipi API Ollama | ✅ Completato |

## File da modificare

### Codice

| File | Modifiche | Stato |
|------|-----------|-------|
| `src/services/mod.rs` | Aggiungere `pub mod ai;` | ✅ Completato |
| `src/config/mod.rs` | Aggiungere `AIConfig`, `ProviderConfig`, `OllamaProviderConfig` | ✅ Completato |
| `src/app_state.rs` | Aggiungere `ai_providers: Arc<ProviderRegistry>` | ⏳ Pendente |
| `src/startup.rs` | Inizializzare il registry dei provider | ⏳ Pendente |
| `Cargo.toml` | Aggiungere `base64 = "0.22"`, `wiremock = "0.6"` (dev) | ⏳ Pendente |
| `config.toml.example` | Aggiungere esempio configurazione AI | ✅ Completato |

### Documentazione

| File | Modifiche | Stato |
|------|-----------|-------|
| `docs/PROVIDER_ABSTRACTION.md` | Aggiornare con la nuova architettura | ✅ Completato |
| `api/docs/configuration.md` | Sostituire sezioni `[gpu]`, `[ollama]`, `[[models]]` con nuova sezione `[ai]` | ✅ Completato |
| `api/docs/architecture.md` | Aggiungere modulo `services/ai/` nella struttura | ✅ Completato |

## Configurazione TOML

```toml
[ai]
default_provider = "ollama"

[ai.providers.ollama]
type = "ollama"
base_url = "http://localhost:11434"
timeout_seconds = 120
devices = []
max_workers = 2

[ai.providers.ollama.models.qwen2-vl]
ollama_model = "qwen2-vl:8b"
prompt_template = "Analyze this image..."
description = "Best quality, slower"
supports_vision = true
```

## Trait AIProvider

```rust
#[async_trait]
pub trait AIProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn check_health(&self) -> AIProviderResult<HealthStatus>;
    async fn list_models(&self, vision_only: bool) -> AIProviderResult<Vec<ModelInfo>>;
    async fn analyze_image(&self, request: AnalyzeImageRequest) -> AIProviderResult<AnalyzeImageResponse>;
}
```

## Endpoint Ollama da integrare

- `GET /api/tags` → `list_models()`, `check_health()`
- `POST /api/generate` → `analyze_image()` (con immagine base64)

## Sequenza implementazione

1. **Fase 1 - Fondamenta** ✅
   - Creare `src/services/ai/error.rs` (tipi errore)
   - Creare `src/services/ai/provider.rs` (trait + tipi)
   - Creare `src/services/ai/mod.rs` (struttura modulo)
   - Aggiornare `src/services/mod.rs`

2. **Fase 2 - Configurazione** ✅
   - Aggiornare `src/config/mod.rs` (AIConfig, OllamaProviderConfig)
   - Aggiornare `config.toml.example`

3. **Fase 3 - Implementazione Ollama** ⏳ ✅
   - Creare `src/services/ai/ollama/types.rs` (tipi API) ✅
   - Creare `src/services/ai/ollama/provider.rs` (OllamaProvider) ✅
   - Creare `src/services/ai/ollama/mod.rs` ✅

4. **Fase 4 - Integrazione** ⏳ ✅
   - Creare `src/services/ai/registry.rs` (ProviderRegistry) ✅
   - Aggiornare `src/app_state.rs` (aggiungere ai_providers) ✅
   - Aggiornare `src/startup.rs` (inizializzare registry) ✅
   - Aggiornare `Cargo.toml` (dipendenze) ✅

5. **Fase 5 - Test** ⏳ ✅
   - Unit test nei moduli ✅
   - Integration test con WireMock (`tests/ai_provider_tests.rs`) ✅

6. **Fase 6 - Documentazione** ✅ Completato
   - Aggiornare `docs/PROVIDER_ABSTRACTION.md` (allineare al nuovo design) ✅
   - Aggiornare `api/docs/configuration.md` (nuova sezione `[ai]`) ✅
   - Aggiornare `api/docs/architecture.md` (aggiungere services/ai) ✅

## Dipendenze da aggiungere

```toml
[dependencies]
base64 = "0.22"

[dev-dependencies]
wiremock = "0.6"
```

Nota: `reqwest` è già presente.

## Verifica

1. `cargo build` - compilazione senza errori
2. `cargo test` - tutti i test passano
3. `cargo clippy` - nessun warning
4. Test manuale con Ollama in esecuzione:
   ```bash
   # Avviare il server
   cargo run

   # Verificare health (futuro endpoint)
   # Per ora, verificare che l'app si avvii senza errori
   # e che i log mostrino "Initialized Ollama provider"
   ```
