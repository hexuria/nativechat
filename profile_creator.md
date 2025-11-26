# Profile Creator - AI Provider Model Capabilities Research

## Overview

This document outlines the APIs needed to fetch model information, capabilities, and tool support from major AI providers (Google Gemini, OpenAI, Anthropic) for building a profile creator feature.

---

## 1. Google Gemini API

### API Endpoints

#### List Models
```
GET https://generativelanguage.googleapis.com/v1beta/models
```

**Query Parameters:**
- `pageSize` (integer): Max models per page (default: 50, max: 1000)
- `pageToken` (string): Pagination token

#### Get Model Details
```
GET https://generativelanguage.googleapis.com/v1beta/models/{model_name}
```

**Example:**
```bash
curl "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.0-flash?key=$GEMINI_API_KEY"
```

### Response Schema (Model Object)

```json
{
  "name": "models/gemini-2.5-pro",
  "baseModelId": "gemini-2.5-pro",
  "version": "2.5",
  "displayName": "Gemini 2.5 Pro",
  "description": "Our state-of-the-art thinking model...",
  "inputTokenLimit": 1048576,
  "outputTokenLimit": 65536,
  "supportedGenerationMethods": [
    "generateContent",
    "streamGenerateContent",
    "embedContent"
  ],
  "thinking": true,
  "temperature": 1.0,
  "maxTemperature": 2.0,
  "topP": 0.95,
  "topK": 64
}
```

### Key Fields for Capabilities

| Field | Description |
|-------|-------------|
| `supportedGenerationMethods` | Array of supported methods (e.g., `generateContent`, `embedContent`) |
| `thinking` | Boolean - whether model supports thinking/reasoning |
| `inputTokenLimit` | Max input tokens |
| `outputTokenLimit` | Max output tokens |

### Capabilities NOT in API Response

**Important:** The Gemini API's model endpoint does NOT return detailed capability flags. The documentation lists these capabilities per model, but they are NOT programmatically available:

| Capability | Example Models Supporting |
|------------|--------------------------|
| Audio generation | gemini-2.5-pro-preview-tts |
| Batch API | Most models |
| Caching | gemini-2.5-pro, gemini-2.5-flash |
| Code execution | gemini-3-pro-preview, gemini-2.5-pro |
| File search | gemini-3-pro-preview, gemini-2.5-pro |
| Function calling | Most text models |
| Grounding with Google Maps | gemini-2.5-pro |
| Image generation | gemini-3-pro-image-preview |
| Live API | gemini-2.0-flash-live-001 |
| Search grounding | Most models |
| Structured outputs | Most models |
| Thinking | gemini-3-pro-preview, gemini-2.5-pro |
| URL context | gemini-3-pro-preview, gemini-2.5-pro |

### Recommendation for Gemini

You'll need to **maintain a local mapping** of model capabilities since the API only returns basic metadata. The capabilities list from their docs:
- https://ai.google.dev/gemini-api/docs/models

---

## 2. OpenAI API

### API Endpoints

#### List Models
```
GET https://api.openai.com/v1/models
Authorization: Bearer $OPENAI_API_KEY
```

#### Get Model Details
```
GET https://api.openai.com/v1/models/{model_id}
Authorization: Bearer $OPENAI_API_KEY
```

### Response Schema (Model Object)

```json
{
  "id": "gpt-5.1",
  "object": "model",
  "created": 1686935002,
  "owned_by": "openai"
}
```

### Capabilities NOT in API Response

**Critical Limitation:** OpenAI's `/v1/models` endpoint returns **minimal metadata** - just `id`, `object`, `created`, and `owned_by`. It does NOT include:

- Token limits
- Supported features
- Tool capabilities
- Pricing
- Context window size

### Capabilities by Model (from documentation)

#### GPT-5.1 (Flagship)
| Feature | Status |
|---------|--------|
| Streaming | Supported |
| Function calling | Supported |
| Structured outputs | Supported |
| Web search | Supported |
| File search | Supported |
| Image generation | Supported |
| Code interpreter | Supported |
| Computer use | Not supported |
| MCP | Supported |
| Vision (image input) | Supported |
| Audio input/output | Not supported |
| Reasoning tokens | Supported |

**Context:** 400,000 tokens input, 128,000 max output

#### Endpoints Supported per Model
| Endpoint | Description |
|----------|-------------|
| `/v1/chat/completions` | Chat completions |
| `/v1/responses` | Responses API (unified) |
| `/v1/realtime` | Realtime audio/text |
| `/v1/assistants` | Assistants API |
| `/v1/batch` | Batch processing |
| `/v1/fine-tuning` | Fine-tuning |
| `/v1/embeddings` | Embeddings |
| `/v1/images/generations` | Image generation |
| `/v1/audio/speech` | Text-to-speech |
| `/v1/audio/transcriptions` | Speech-to-text |

### Recommendation for OpenAI

You **must maintain a local capability mapping** since the API doesn't expose this data. Consider scraping or manually maintaining:
- https://platform.openai.com/docs/models

---

## 3. Anthropic Claude API

### API Endpoints

#### List Models
```
GET https://api.anthropic.com/v1/models
x-api-key: $ANTHROPIC_API_KEY
anthropic-version: 2023-06-01
```

**Query Parameters:**
- `limit` (integer): Items per page (default: 20, max: 1000)
- `after_id` (string): Cursor for pagination
- `before_id` (string): Cursor for pagination

#### Get Model Details
```
GET https://api.anthropic.com/v1/models/{model_id}
x-api-key: $ANTHROPIC_API_KEY
anthropic-version: 2023-06-01
```

### Response Schema (Model Object)

```json
{
  "data": [
    {
      "id": "claude-sonnet-4-20250514",
      "created_at": "2025-02-19T00:00:00Z",
      "display_name": "Claude Sonnet 4",
      "type": "model"
    }
  ],
  "first_id": "first_id",
  "has_more": true,
  "last_id": "last_id"
}
```

### Capabilities NOT in API Response

**Critical Limitation:** Like OpenAI, Anthropic's model endpoint returns **minimal metadata** - just `id`, `created_at`, `display_name`, and `type`. No capability flags.

### Capabilities by Model (from documentation)

#### Core Capabilities (All Claude 3+/4+ Models)
| Feature | Availability |
|---------|--------------|
| 1M token context window | Beta |
| Extended thinking | Supported |
| Vision (image input) | Supported |
| PDF support | Supported |
| Tool use / Function calling | Supported |
| Structured outputs | Beta (Sonnet 4.5, Opus 4.1) |
| Batch processing | Supported |
| Citations | Supported |
| Prompt caching (5m) | Supported |
| Prompt caching (1hr) | Supported |
| Token counting | Supported |

#### Tools Available
| Tool | Availability |
|------|--------------|
| Bash | Supported |
| Code execution | Beta |
| Computer use | Beta |
| Text editor | Supported |
| Web search | Supported |
| Web fetch | Beta |
| Memory | Beta |
| MCP connector | Beta |
| Tool search | Beta |
| Agent Skills | Beta |

#### Model Variants
| Model | Strengths |
|-------|-----------|
| Claude Opus 4.5 | Most intelligent, complex tasks |
| Claude Sonnet 4.5 | Balanced intelligence/speed/cost |
| Claude Haiku 4.5 | Fastest, near-frontier intelligence |
| Claude Opus 4.1 | Specialized reasoning |

### Recommendation for Anthropic

Maintain a local capability mapping. Reference:
- https://docs.anthropic.com/en/docs/resources/api-features
- https://docs.anthropic.com/en/docs/about-claude/models/overview

---

## 4. Unified Data Structure Proposal

Since none of the providers return comprehensive capability data via API, here's a proposed unified schema for your profile creator:

```rust
struct ModelProfile {
    // Basic Info (from API)
    provider: Provider,              // Gemini, OpenAI, Anthropic
    id: String,                      // e.g., "gpt-5.1", "claude-sonnet-4"
    display_name: String,
    created_at: Option<DateTime>,
    
    // Token Limits (manual/scraped)
    input_token_limit: u32,
    output_token_limit: u32,
    context_window: u32,
    
    // Input Modalities
    supports_text_input: bool,
    supports_image_input: bool,      // Vision
    supports_audio_input: bool,
    supports_video_input: bool,
    supports_pdf_input: bool,
    
    // Output Modalities
    supports_text_output: bool,
    supports_image_output: bool,     // Image generation
    supports_audio_output: bool,
    supports_video_output: bool,
    
    // Capabilities
    supports_streaming: bool,
    supports_function_calling: bool,
    supports_structured_outputs: bool,
    supports_reasoning: bool,        // Thinking/reasoning tokens
    supports_caching: bool,
    supports_batch: bool,
    supports_fine_tuning: bool,
    
    // Tools
    supports_web_search: bool,
    supports_file_search: bool,
    supports_code_execution: bool,
    supports_computer_use: bool,
    supports_mcp: bool,
    
    // Pricing (per 1M tokens)
    input_price: Option<f64>,
    output_price: Option<f64>,
    
    // Metadata
    knowledge_cutoff: Option<String>,
    deprecation_date: Option<DateTime>,
}

enum Provider {
    Gemini,
    OpenAI,
    Anthropic,
}
```

---

## 5. Implementation Strategy

### Option A: Static Configuration (Recommended)

Maintain a JSON/TOML config file with model capabilities that you update periodically:

```json
{
  "models": {
    "gpt-5.1": {
      "provider": "openai",
      "display_name": "GPT-5.1",
      "input_token_limit": 400000,
      "output_token_limit": 128000,
      "capabilities": {
        "vision": true,
        "function_calling": true,
        "web_search": true,
        "file_search": true,
        "image_generation": true,
        "code_interpreter": true,
        "computer_use": false,
        "streaming": true,
        "reasoning": true
      }
    }
  }
}
```

**Pros:**
- Fast, no API calls needed
- Complete control over data
- Works offline

**Cons:**
- Requires manual updates when providers add models

### Option B: Hybrid Approach

1. Call provider APIs to get list of available models
2. Merge with local capability config
3. Flag any new models without capability data

```rust
async fn get_models() -> Vec<ModelProfile> {
    // 1. Fetch from APIs
    let gemini_models = fetch_gemini_models().await;
    let openai_models = fetch_openai_models().await;
    let anthropic_models = fetch_anthropic_models().await;
    
    // 2. Load local capability config
    let capabilities = load_capabilities_config();
    
    // 3. Merge
    merge_models_with_capabilities(
        [gemini_models, openai_models, anthropic_models],
        capabilities
    )
}
```

### Option C: Web Scraping (Not Recommended)

Scrape provider documentation pages. This is fragile and may violate ToS.

---

## 6. API Code Examples

### Fetching Gemini Models (Rust)

```rust
use reqwest::Client;
use serde::Deserialize;

#[derive(Deserialize)]
struct GeminiModel {
    name: String,
    display_name: Option<String>,
    description: Option<String>,
    input_token_limit: Option<u32>,
    output_token_limit: Option<u32>,
    supported_generation_methods: Vec<String>,
    thinking: Option<bool>,
}

#[derive(Deserialize)]
struct ListModelsResponse {
    models: Vec<GeminiModel>,
    next_page_token: Option<String>,
}

async fn fetch_gemini_models(api_key: &str) -> Result<Vec<GeminiModel>, reqwest::Error> {
    let client = Client::new();
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models?key={}",
        api_key
    );
    
    let response: ListModelsResponse = client
        .get(&url)
        .send()
        .await?
        .json()
        .await?;
    
    Ok(response.models)
}
```

### Fetching OpenAI Models (Rust)

```rust
use reqwest::Client;
use serde::Deserialize;

#[derive(Deserialize)]
struct OpenAIModel {
    id: String,
    object: String,
    created: u64,
    owned_by: String,
}

#[derive(Deserialize)]
struct ListModelsResponse {
    data: Vec<OpenAIModel>,
}

async fn fetch_openai_models(api_key: &str) -> Result<Vec<OpenAIModel>, reqwest::Error> {
    let client = Client::new();
    
    let response: ListModelsResponse = client
        .get("https://api.openai.com/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await?
        .json()
        .await?;
    
    Ok(response.data)
}
```

### Fetching Anthropic Models (Rust)

```rust
use reqwest::Client;
use serde::Deserialize;

#[derive(Deserialize)]
struct AnthropicModel {
    id: String,
    created_at: String,
    display_name: String,
    #[serde(rename = "type")]
    model_type: String,
}

#[derive(Deserialize)]
struct ListModelsResponse {
    data: Vec<AnthropicModel>,
    has_more: bool,
    first_id: Option<String>,
    last_id: Option<String>,
}

async fn fetch_anthropic_models(api_key: &str) -> Result<Vec<AnthropicModel>, reqwest::Error> {
    let client = Client::new();
    
    let response: ListModelsResponse = client
        .get("https://api.anthropic.com/v1/models")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .send()
        .await?
        .json()
        .await?;
    
    Ok(response.data)
}
```

---

## 7. Summary & Key Takeaways

| Provider | API Returns | Capability Data |
|----------|-------------|-----------------|
| **Gemini** | Model name, token limits, supported methods, thinking flag | Basic - missing most capability flags |
| **OpenAI** | Model ID, created date, owner | Minimal - no capabilities at all |
| **Anthropic** | Model ID, display name, created date | Minimal - no capabilities at all |

### Bottom Line

**None of the major AI providers expose comprehensive model capabilities via their APIs.** You will need to:

1. **Maintain a local configuration** with capability mappings
2. **Periodically update** when providers announce new models/features
3. **Use the APIs** to fetch the list of available models and merge with your config
4. **Consider caching** the model list to reduce API calls

### Useful Documentation Links

- **Gemini**: https://ai.google.dev/gemini-api/docs/models
- **OpenAI**: https://platform.openai.com/docs/models
- **Anthropic**: https://docs.anthropic.com/en/docs/about-claude/models/overview

---

## 8. Future Considerations

1. **Azure OpenAI** has a richer API that returns capabilities like `fine_tune`, `inference`, `completion`, `chat_completion`, `embeddings` - consider if you want to support Azure as a provider

2. **Model versioning** - all providers use snapshot versioning (e.g., `gpt-5.1-2025-11-13`, `claude-sonnet-4-20250514`) - handle aliases vs specific versions

3. **Deprecation tracking** - build alerts for model deprecation dates

4. **Rate limits** vary by model and tier - may want to track these as well
