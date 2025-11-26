# Profile Creator Implementation - WASM/WASI + Type-State Pattern Design

## Overview

This document outlines a Rust implementation using **WASM/WASI Component Model + Builder Pattern + Type-State Pattern** to create a type-safe, sandboxed AI profile system where:

1. **Capabilities run in WASM sandboxes** - Each capability/tool/skill is a WASM component with explicit permissions
2. **WIT interfaces define contracts** - Tools, skills, and mini-apps expose WIT interfaces
3. **Host controls permissions** - The native app (host) grants capabilities to WASM guests
4. **Models are decoupled by function** - Text generation, image generation, embeddings are separate
5. **Type-State for compile-time safety** - Profile builder validates configurations
6. **Dynamic plugin loading** - Skills and mini-apps can be loaded at runtime

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                        Native Host (gpui app)                        │
├─────────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐              │
│  │   Profile    │  │    Model     │  │   Wasmtime   │              │
│  │   Manager    │  │   Registry   │  │   Runtime    │              │
│  └──────────────┘  └──────────────┘  └──────────────┘              │
│         │                  │                  │                     │
│         ▼                  ▼                  ▼                     │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │                    Capability Host APIs                      │   │
│  │  (HTTP, Filesystem, Secrets, Model API, UI Callbacks)       │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                              │                                      │
│         ┌────────────────────┼────────────────────┐                │
│         ▼                    ▼                    ▼                │
│  ┌────────────┐      ┌────────────┐      ┌────────────┐           │
│  │   Tool     │      │   Skill    │      │  Mini-App  │           │
│  │   WASM     │      │   WASM     │      │   WASM     │           │
│  │ Component  │      │ Component  │      │ Component  │           │
│  └────────────┘      └────────────┘      └────────────┘           │
│  web-search.wasm     study.wasm          canva.wasm               │
│  code-exec.wasm      canvas.wasm         figma.wasm               │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 1. WIT Interface Definitions

### Core Capability Interfaces (`wit/capabilities.wit`)

```wit
package nativechat:capabilities@0.1.0;

/// Permissions that can be granted to components
interface permissions {
    /// Check if a permission is granted
    has-permission: func(name: string) -> bool;
    
    /// Request a permission (may prompt user)
    request-permission: func(name: string) -> result<bool, string>;
}

/// Model capabilities that the host exposes
interface model-api {
    /// Text generation request
    record chat-request {
        messages: list<message>,
        temperature: option<f32>,
        max-tokens: option<u32>,
        tools: option<list<tool-definition>>,
    }
    
    record message {
        role: string,
        content: string,
        images: option<list<list<u8>>>,
    }
    
    record tool-definition {
        name: string,
        description: string,
        parameters: string, // JSON schema
    }
    
    record chat-response {
        content: string,
        tool-calls: option<list<tool-call>>,
        usage: token-usage,
    }
    
    record tool-call {
        id: string,
        name: string,
        arguments: string, // JSON
    }
    
    record token-usage {
        input-tokens: u32,
        output-tokens: u32,
    }
    
    /// Generate text completion
    generate: func(request: chat-request) -> result<chat-response, string>;
    
    /// Generate embeddings
    embed: func(text: string) -> result<list<f32>, string>;
    
    /// Generate image
    generate-image: func(prompt: string, size: tuple<u32, u32>) -> result<list<u8>, string>;
}

/// HTTP client capability
interface http-client {
    record http-request {
        method: string,
        url: string,
        headers: list<tuple<string, string>>,
        body: option<list<u8>>,
    }
    
    record http-response {
        status: u16,
        headers: list<tuple<string, string>>,
        body: list<u8>,
    }
    
    fetch: func(request: http-request) -> result<http-response, string>;
}

/// Secrets/credential management
interface secrets {
    /// Get a secret by key (e.g., API key)
    get-secret: func(key: string) -> result<string, string>;
    
    /// Check if secret exists
    has-secret: func(key: string) -> bool;
}

/// UI callback interface
interface ui-callbacks {
    /// Show a toast notification
    show-toast: func(message: string, level: string);
    
    /// Request user confirmation
    confirm: func(message: string) -> bool;
    
    /// Open a URL in browser
    open-url: func(url: string);
    
    /// Trigger a UI update
    notify-update: func();
}
```

### Tool Interface (`wit/tool.wit`)

```wit
package nativechat:tool@0.1.0;

use nativechat:capabilities/model-api.{chat-request, chat-response, tool-call};
use nativechat:capabilities/http-client.{http-request, http-response};
use nativechat:capabilities/permissions.{has-permission};
use nativechat:capabilities/secrets.{get-secret};

/// Tool metadata
interface tool-info {
    record tool-metadata {
        id: string,
        name: string,
        description: string,
        icon: string,
        required-permissions: list<string>,
        required-capabilities: list<string>,
    }
    
    /// Get tool metadata
    get-metadata: func() -> tool-metadata;
}

/// Tool execution interface
interface tool-executor {
    record tool-input {
        name: string,
        arguments: string, // JSON
        context: option<string>,
    }
    
    record tool-output {
        result: string, // JSON or text
        artifacts: option<list<artifact>>,
    }
    
    record artifact {
        name: string,
        mime-type: string,
        data: list<u8>,
    }
    
    /// Execute the tool
    execute: func(input: tool-input) -> result<tool-output, string>;
}

/// World for a tool component
world tool {
    import nativechat:capabilities/permissions;
    import nativechat:capabilities/http-client;
    import nativechat:capabilities/secrets;
    import nativechat:capabilities/model-api;
    
    export tool-info;
    export tool-executor;
}
```

### Skill Interface (`wit/skill.wit`)

```wit
package nativechat:skill@0.1.0;

use nativechat:capabilities/model-api.{chat-request, chat-response};
use nativechat:capabilities/http-client.{http-request, http-response};
use nativechat:capabilities/ui-callbacks.{show-toast, notify-update};

/// Skill metadata
interface skill-info {
    record skill-metadata {
        id: string,
        name: string,
        description: string,
        icon: string,
        required-tools: list<string>,
        prompt-template: option<string>,
    }
    
    get-metadata: func() -> skill-metadata;
}

/// Skill orchestration interface
interface skill-orchestrator {
    record skill-context {
        user-input: string,
        conversation-history: list<string>,
        available-tools: list<string>,
    }
    
    record skill-action {
        action-type: action-type,
        payload: string, // JSON
    }
    
    enum action-type {
        send-message,
        call-tool,
        show-ui,
        complete,
    }
    
    /// Process user input and return next action
    process: func(context: skill-context) -> result<skill-action, string>;
    
    /// Handle tool result and continue
    handle-tool-result: func(tool-name: string, result: string) -> result<skill-action, string>;
}

/// World for a skill component
world skill {
    import nativechat:capabilities/permissions;
    import nativechat:capabilities/model-api;
    import nativechat:capabilities/http-client;
    import nativechat:capabilities/ui-callbacks;
    
    export skill-info;
    export skill-orchestrator;
}
```

### Mini-App Interface (`wit/miniapp.wit`)

```wit
package nativechat:miniapp@0.1.0;

use nativechat:capabilities/http-client.{http-request, http-response};
use nativechat:capabilities/secrets.{get-secret};
use nativechat:capabilities/ui-callbacks.{show-toast, open-url};

/// Mini-app metadata
interface app-info {
    record app-metadata {
        id: string,
        name: string,
        description: string,
        icon: string,
        oauth-config: option<oauth-config>,
        mcp-server-url: option<string>,
    }
    
    record oauth-config {
        client-id: string,
        auth-url: string,
        token-url: string,
        scopes: list<string>,
    }
    
    get-metadata: func() -> app-metadata;
}

/// Mini-app actions interface
interface app-actions {
    record action-request {
        action: string,
        parameters: string, // JSON
    }
    
    record action-response {
        success: bool,
        result: option<string>,
        error: option<string>,
    }
    
    /// List available actions
    list-actions: func() -> list<string>;
    
    /// Execute an action
    execute-action: func(request: action-request) -> result<action-response, string>;
    
    /// Check if authenticated
    is-authenticated: func() -> bool;
    
    /// Initiate OAuth flow
    start-oauth: func() -> result<string, string>; // Returns auth URL
}

/// World for a mini-app component
world miniapp {
    import nativechat:capabilities/permissions;
    import nativechat:capabilities/http-client;
    import nativechat:capabilities/secrets;
    import nativechat:capabilities/ui-callbacks;
    
    export app-info;
    export app-actions;
}
```

---

## 2. Host Runtime Implementation

### Wasmtime Component Host

```rust
use wasmtime::component::*;
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::preview2::*;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Host state shared with WASM components
pub struct HostState {
    pub wasi: WasiCtx,
    pub http_client: reqwest::Client,
    pub secrets: Arc<RwLock<SecretStore>>,
    pub model_client: Arc<dyn ModelClient>,
    pub permissions: PermissionSet,
    pub ui_tx: tokio::sync::mpsc::Sender<UiEvent>,
}

/// Permission set for a component
#[derive(Clone, Default)]
pub struct PermissionSet {
    pub http_allowed_domains: Vec<String>,
    pub can_access_model: bool,
    pub can_access_secrets: bool,
    pub can_generate_images: bool,
    pub can_execute_code: bool,
    pub can_access_filesystem: bool,
}

impl PermissionSet {
    pub fn tool_default() -> Self {
        Self {
            http_allowed_domains: vec!["*".into()], // Tools often need HTTP
            can_access_model: true,
            can_access_secrets: false,
            can_generate_images: false,
            can_execute_code: false,
            can_access_filesystem: false,
        }
    }
    
    pub fn skill_default() -> Self {
        Self {
            http_allowed_domains: vec![],
            can_access_model: true,
            can_access_secrets: false,
            can_generate_images: false,
            can_execute_code: false,
            can_access_filesystem: false,
        }
    }
    
    pub fn miniapp_default() -> Self {
        Self {
            http_allowed_domains: vec![], // Set per-app
            can_access_model: false,
            can_access_secrets: true, // For OAuth tokens
            can_generate_images: false,
            can_execute_code: false,
            can_access_filesystem: false,
        }
    }
}

/// Component runtime manager
pub struct ComponentRuntime {
    engine: Engine,
    linker: Linker<HostState>,
}

impl ComponentRuntime {
    pub fn new() -> anyhow::Result<Self> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        config.async_support(true);
        
        let engine = Engine::new(&config)?;
        let mut linker = Linker::new(&engine);
        
        // Add WASI preview 2
        wasmtime_wasi::preview2::add_to_linker_async(&mut linker)?;
        
        // Add our capability interfaces
        Self::add_capabilities_to_linker(&mut linker)?;
        
        Ok(Self { engine, linker })
    }
    
    fn add_capabilities_to_linker(linker: &mut Linker<HostState>) -> anyhow::Result<()> {
        // HTTP client capability
        linker.func_wrap_async(
            "nativechat:capabilities/http-client",
            "fetch",
            |mut caller: Caller<'_, HostState>, request: HttpRequest| {
                Box::new(async move {
                    let state = caller.data();
                    
                    // Check permissions
                    let url = url::Url::parse(&request.url)
                        .map_err(|e| format!("Invalid URL: {}", e))?;
                    
                    let domain = url.domain().unwrap_or("");
                    let allowed = state.permissions.http_allowed_domains.iter()
                        .any(|d| d == "*" || d == domain);
                    
                    if !allowed {
                        return Err(format!("HTTP access to {} not permitted", domain));
                    }
                    
                    // Execute request
                    let mut req = state.http_client.request(
                        request.method.parse().unwrap_or(reqwest::Method::GET),
                        &request.url,
                    );
                    
                    for (key, value) in request.headers {
                        req = req.header(&key, &value);
                    }
                    
                    if let Some(body) = request.body {
                        req = req.body(body);
                    }
                    
                    let response = req.send().await
                        .map_err(|e| format!("HTTP error: {}", e))?;
                    
                    Ok(HttpResponse {
                        status: response.status().as_u16(),
                        headers: response.headers()
                            .iter()
                            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                            .collect(),
                        body: response.bytes().await
                            .map_err(|e| format!("Body error: {}", e))?
                            .to_vec(),
                    })
                })
            },
        )?;
        
        // Model API capability
        linker.func_wrap_async(
            "nativechat:capabilities/model-api",
            "generate",
            |mut caller: Caller<'_, HostState>, request: ChatRequest| {
                Box::new(async move {
                    let state = caller.data();
                    
                    if !state.permissions.can_access_model {
                        return Err("Model access not permitted".into());
                    }
                    
                    state.model_client.generate(request).await
                })
            },
        )?;
        
        // Secrets capability
        linker.func_wrap_async(
            "nativechat:capabilities/secrets",
            "get-secret",
            |mut caller: Caller<'_, HostState>, key: String| {
                Box::new(async move {
                    let state = caller.data();
                    
                    if !state.permissions.can_access_secrets {
                        return Err("Secret access not permitted".into());
                    }
                    
                    state.secrets.read().await
                        .get(&key)
                        .cloned()
                        .ok_or_else(|| format!("Secret '{}' not found", key))
                })
            },
        )?;
        
        // UI callbacks
        linker.func_wrap(
            "nativechat:capabilities/ui-callbacks",
            "show-toast",
            |mut caller: Caller<'_, HostState>, message: String, level: String| {
                let _ = caller.data().ui_tx.blocking_send(UiEvent::Toast { message, level });
            },
        )?;
        
        Ok(())
    }
    
    /// Load and instantiate a tool component
    pub async fn load_tool(&self, wasm_path: &str, permissions: PermissionSet) -> anyhow::Result<ToolInstance> {
        let component = Component::from_file(&self.engine, wasm_path)?;
        let mut store = Store::new(&self.engine, HostState::new(permissions));
        
        let instance = self.linker.instantiate_async(&mut store, &component).await?;
        
        Ok(ToolInstance { store, instance })
    }
    
    /// Load and instantiate a skill component
    pub async fn load_skill(&self, wasm_path: &str, permissions: PermissionSet) -> anyhow::Result<SkillInstance> {
        let component = Component::from_file(&self.engine, wasm_path)?;
        let mut store = Store::new(&self.engine, HostState::new(permissions));
        
        let instance = self.linker.instantiate_async(&mut store, &component).await?;
        
        Ok(SkillInstance { store, instance })
    }
}

/// Loaded tool instance
pub struct ToolInstance {
    store: Store<HostState>,
    instance: Instance,
}

impl ToolInstance {
    pub async fn get_metadata(&mut self) -> anyhow::Result<ToolMetadata> {
        let func = self.instance
            .get_typed_func::<(), (ToolMetadata,)>(&mut self.store, "get-metadata")?;
        let (metadata,) = func.call_async(&mut self.store, ()).await?;
        Ok(metadata)
    }
    
    pub async fn execute(&mut self, input: ToolInput) -> anyhow::Result<ToolOutput> {
        let func = self.instance
            .get_typed_func::<(ToolInput,), (Result<ToolOutput, String>,)>(&mut self.store, "execute")?;
        let (result,) = func.call_async(&mut self.store, (input,)).await?;
        result.map_err(|e| anyhow::anyhow!(e))
    }
}
```

---

## 3. Type-State Pattern for Profile Builder

### Capability Marker Traits (Zero-Sized Types)

```rust
use std::marker::PhantomData;

// Capability states (zero-sized types for phantom data)
pub struct Yes;
pub struct No;

// Capability marker traits
pub trait CapabilityState {}
impl CapabilityState for Yes {}
impl CapabilityState for No {}

// Individual capability markers
pub trait HasVision {}
pub trait HasImageGeneration {}
pub trait HasAudioInput {}
pub trait HasAudioOutput {}
pub trait HasFunctionCalling {}
pub trait HasWebSearch {}
pub trait HasFileSearch {}
pub trait HasCodeExecution {}
pub trait HasComputerUse {}
pub trait HasThinking {}
pub trait HasStreaming {}
pub trait HasStructuredOutput {}
pub trait HasMCP {}

// Implement markers for Yes state
impl HasVision for Yes {}
impl HasImageGeneration for Yes {}
impl HasAudioInput for Yes {}
impl HasAudioOutput for Yes {}
impl HasFunctionCalling for Yes {}
impl HasWebSearch for Yes {}
impl HasFileSearch for Yes {}
impl HasCodeExecution for Yes {}
impl HasComputerUse for Yes {}
impl HasThinking for Yes {}
impl HasStreaming for Yes {}
impl HasStructuredOutput for Yes {}
impl HasMCP for Yes {}
```

### The Model Type with Phantom Capabilities

```rust
/// A model with compile-time capability tracking
pub struct Model<
    Vision = No,
    ImageGen = No,
    AudioIn = No,
    AudioOut = No,
    FnCall = No,
    WebSearch = No,
    FileSearch = No,
    CodeExec = No,
    CompUse = No,
    Thinking = No,
    Streaming = No,
    StructOut = No,
    Mcp = No,
> where
    Vision: CapabilityState,
    ImageGen: CapabilityState,
    AudioIn: CapabilityState,
    AudioOut: CapabilityState,
    FnCall: CapabilityState,
    WebSearch: CapabilityState,
    FileSearch: CapabilityState,
    CodeExec: CapabilityState,
    CompUse: CapabilityState,
    Thinking: CapabilityState,
    Streaming: CapabilityState,
    StructOut: CapabilityState,
    Mcp: CapabilityState,
{
    pub id: String,
    pub display_name: String,
    pub provider: Provider,
    pub input_token_limit: u32,
    pub output_token_limit: u32,
    
    // Phantom markers (zero runtime cost)
    _vision: PhantomData<Vision>,
    _image_gen: PhantomData<ImageGen>,
    _audio_in: PhantomData<AudioIn>,
    _audio_out: PhantomData<AudioOut>,
    _fn_call: PhantomData<FnCall>,
    _web_search: PhantomData<WebSearch>,
    _file_search: PhantomData<FileSearch>,
    _code_exec: PhantomData<CodeExec>,
    _comp_use: PhantomData<CompUse>,
    _thinking: PhantomData<Thinking>,
    _streaming: PhantomData<Streaming>,
    _struct_out: PhantomData<StructOut>,
    _mcp: PhantomData<Mcp>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Provider {
    OpenAI,
    Anthropic,
    Gemini,
}
```

---

## 2. Capability-Gated Methods

Methods only available when the model has specific capabilities:

```rust
// Text generation - base capability (all models)
impl<V, I, Ai, Ao, F, W, Fs, C, Cu, T, St, So, M> Model<V, I, Ai, Ao, F, W, Fs, C, Cu, T, St, So, M>
where
    V: CapabilityState,
    I: CapabilityState,
    Ai: CapabilityState,
    Ao: CapabilityState,
    F: CapabilityState,
    W: CapabilityState,
    Fs: CapabilityState,
    C: CapabilityState,
    Cu: CapabilityState,
    T: CapabilityState,
    St: CapabilityState,
    So: CapabilityState,
    M: CapabilityState,
{
    pub async fn generate_text(&self, prompt: &str) -> Result<String, ApiError> {
        // All models support text generation
        todo!()
    }
}

// Vision methods - only when Vision = Yes
impl<I, Ai, Ao, F, W, Fs, C, Cu, T, St, So, M> Model<Yes, I, Ai, Ao, F, W, Fs, C, Cu, T, St, So, M>
where
    I: CapabilityState,
    Ai: CapabilityState,
    Ao: CapabilityState,
    F: CapabilityState,
    W: CapabilityState,
    Fs: CapabilityState,
    C: CapabilityState,
    Cu: CapabilityState,
    T: CapabilityState,
    St: CapabilityState,
    So: CapabilityState,
    M: CapabilityState,
{
    pub async fn analyze_image(&self, image: &[u8], prompt: &str) -> Result<String, ApiError> {
        // Only available when Vision = Yes
        todo!()
    }
    
    pub async fn analyze_pdf(&self, pdf: &[u8], prompt: &str) -> Result<String, ApiError> {
        todo!()
    }
}

// Function calling - only when FnCall = Yes
impl<V, I, Ai, Ao, W, Fs, C, Cu, T, St, So, M> Model<V, I, Ai, Ao, Yes, W, Fs, C, Cu, T, St, So, M>
where
    V: CapabilityState,
    I: CapabilityState,
    Ai: CapabilityState,
    Ao: CapabilityState,
    W: CapabilityState,
    Fs: CapabilityState,
    C: CapabilityState,
    Cu: CapabilityState,
    T: CapabilityState,
    St: CapabilityState,
    So: CapabilityState,
    M: CapabilityState,
{
    pub async fn call_function(&self, tools: &[Tool], prompt: &str) -> Result<ToolCall, ApiError> {
        todo!()
    }
}

// Web search - only when WebSearch = Yes
impl<V, I, Ai, Ao, F, Fs, C, Cu, T, St, So, M> Model<V, I, Ai, Ao, F, Yes, Fs, C, Cu, T, St, So, M>
where
    V: CapabilityState,
    I: CapabilityState,
    Ai: CapabilityState,
    Ao: CapabilityState,
    F: CapabilityState,
    Fs: CapabilityState,
    C: CapabilityState,
    Cu: CapabilityState,
    T: CapabilityState,
    St: CapabilityState,
    So: CapabilityState,
    M: CapabilityState,
{
    pub async fn search_web(&self, query: &str) -> Result<Vec<SearchResult>, ApiError> {
        todo!()
    }
}

// Thinking/Reasoning - only when Thinking = Yes
impl<V, I, Ai, Ao, F, W, Fs, C, Cu, St, So, M> Model<V, I, Ai, Ao, F, W, Fs, C, Cu, Yes, St, So, M>
where
    V: CapabilityState,
    I: CapabilityState,
    Ai: CapabilityState,
    Ao: CapabilityState,
    F: CapabilityState,
    W: CapabilityState,
    Fs: CapabilityState,
    C: CapabilityState,
    Cu: CapabilityState,
    St: CapabilityState,
    So: CapabilityState,
    M: CapabilityState,
{
    pub async fn think(&self, prompt: &str, budget_tokens: u32) -> Result<ThinkingResponse, ApiError> {
        todo!()
    }
}

// Streaming - only when Streaming = Yes
impl<V, I, Ai, Ao, F, W, Fs, C, Cu, T, So, M> Model<V, I, Ai, Ao, F, W, Fs, C, Cu, T, Yes, So, M>
where
    V: CapabilityState,
    I: CapabilityState,
    Ai: CapabilityState,
    Ao: CapabilityState,
    F: CapabilityState,
    W: CapabilityState,
    Fs: CapabilityState,
    C: CapabilityState,
    Cu: CapabilityState,
    T: CapabilityState,
    So: CapabilityState,
    M: CapabilityState,
{
    pub fn stream_text(&self, prompt: &str) -> impl Stream<Item = Result<String, ApiError>> {
        todo!()
    }
}

// Computer use - only when CompUse = Yes  
impl<V, I, Ai, Ao, F, W, Fs, C, T, St, So, M> Model<V, I, Ai, Ao, F, W, Fs, C, Yes, T, St, So, M>
where
    V: CapabilityState,
    I: CapabilityState,
    Ai: CapabilityState,
    Ao: CapabilityState,
    F: CapabilityState,
    W: CapabilityState,
    Fs: CapabilityState,
    C: CapabilityState,
    T: CapabilityState,
    St: CapabilityState,
    So: CapabilityState,
    M: CapabilityState,
{
    pub async fn computer_action(&self, screenshot: &[u8], instruction: &str) -> Result<ComputerAction, ApiError> {
        todo!()
    }
}
```

---

## 3. Decoupled Model Types

### Separate Types for Different Functions

```rust
/// Text generation model (chat, completion)
pub struct TextModel<Caps> {
    inner: Model<Caps>,
}

/// Image generation model (DALL-E, Gemini Image)
pub struct ImageModel {
    pub id: String,
    pub provider: Provider,
    pub max_resolution: (u32, u32),
    pub supports_editing: bool,
    pub supports_variations: bool,
}

/// Embedding model
pub struct EmbeddingModel {
    pub id: String,
    pub provider: Provider,
    pub dimensions: u32,
    pub max_input_tokens: u32,
}

/// Audio model (TTS, STT)
pub struct AudioModel {
    pub id: String,
    pub provider: Provider,
    pub mode: AudioMode,
    pub sample_rate: u32,
    pub voices: Vec<String>,
}

#[derive(Clone, Debug)]
pub enum AudioMode {
    TextToSpeech,
    SpeechToText,
    Realtime,
}

/// Video model (Sora)
pub struct VideoModel {
    pub id: String,
    pub provider: Provider,
    pub max_duration_seconds: u32,
    pub max_resolution: (u32, u32),
}
```

### Model Implementations

```rust
impl ImageModel {
    pub async fn generate(&self, prompt: &str, size: (u32, u32)) -> Result<Vec<u8>, ApiError> {
        todo!()
    }
    
    pub async fn edit(&self, image: &[u8], mask: &[u8], prompt: &str) -> Result<Vec<u8>, ApiError> {
        if !self.supports_editing {
            return Err(ApiError::UnsupportedOperation("edit".into()));
        }
        todo!()
    }
}

impl EmbeddingModel {
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>, ApiError> {
        todo!()
    }
    
    pub async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, ApiError> {
        todo!()
    }
}

impl AudioModel {
    pub async fn synthesize(&self, text: &str, voice: &str) -> Result<Vec<u8>, ApiError> {
        match self.mode {
            AudioMode::TextToSpeech | AudioMode::Realtime => todo!(),
            _ => Err(ApiError::UnsupportedOperation("synthesize".into())),
        }
    }
    
    pub async fn transcribe(&self, audio: &[u8]) -> Result<String, ApiError> {
        match self.mode {
            AudioMode::SpeechToText | AudioMode::Realtime => todo!(),
            _ => Err(ApiError::UnsupportedOperation("transcribe".into())),
        }
    }
}
```

---

## 4. Profile Builder with Type-State

### Builder States

```rust
// Builder states
pub struct NoChatModel;
pub struct HasChatModel;
pub struct NoEmbeddingModel;
pub struct HasEmbeddingModel;
pub struct NoImageModel;
pub struct HasImageModel;

/// Profile builder with type-state validation
pub struct ProfileBuilder<ChatState = NoChatModel, EmbedState = NoEmbeddingModel, ImageState = NoImageModel> {
    name: Option<String>,
    chat_model: Option<DynTextModel>,
    embedding_model: Option<EmbeddingModel>,
    image_model: Option<ImageModel>,
    audio_model: Option<AudioModel>,
    enabled_tools: Vec<ToolDefinition>,
    enabled_skills: Vec<SkillDefinition>,
    enabled_apps: Vec<MiniAppDefinition>,
    _chat: PhantomData<ChatState>,
    _embed: PhantomData<EmbedState>,
    _image: PhantomData<ImageState>,
}

impl ProfileBuilder<NoChatModel, NoEmbeddingModel, NoImageModel> {
    pub fn new() -> Self {
        Self {
            name: None,
            chat_model: None,
            embedding_model: None,
            image_model: None,
            audio_model: None,
            enabled_tools: Vec::new(),
            enabled_skills: Vec::new(),
            enabled_apps: Vec::new(),
            _chat: PhantomData,
            _embed: PhantomData,
            _image: PhantomData,
        }
    }
}

impl<C, E, I> ProfileBuilder<C, E, I> {
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

// Transitions: NoChatModel -> HasChatModel
impl<E, I> ProfileBuilder<NoChatModel, E, I> {
    pub fn chat_model(self, model: DynTextModel) -> ProfileBuilder<HasChatModel, E, I> {
        ProfileBuilder {
            name: self.name,
            chat_model: Some(model),
            embedding_model: self.embedding_model,
            image_model: self.image_model,
            audio_model: self.audio_model,
            enabled_tools: self.enabled_tools,
            enabled_skills: self.enabled_skills,
            enabled_apps: self.enabled_apps,
            _chat: PhantomData,
            _embed: self._embed,
            _image: self._image,
        }
    }
}

// Transitions: NoEmbeddingModel -> HasEmbeddingModel
impl<C, I> ProfileBuilder<C, NoEmbeddingModel, I> {
    pub fn embedding_model(self, model: EmbeddingModel) -> ProfileBuilder<C, HasEmbeddingModel, I> {
        ProfileBuilder {
            name: self.name,
            chat_model: self.chat_model,
            embedding_model: Some(model),
            image_model: self.image_model,
            audio_model: self.audio_model,
            enabled_tools: self.enabled_tools,
            enabled_skills: self.enabled_skills,
            enabled_apps: self.enabled_apps,
            _chat: self._chat,
            _embed: PhantomData,
            _image: self._image,
        }
    }
}

// Transitions: NoImageModel -> HasImageModel
impl<C, E> ProfileBuilder<C, E, NoImageModel> {
    pub fn image_model(self, model: ImageModel) -> ProfileBuilder<C, E, HasImageModel> {
        ProfileBuilder {
            name: self.name,
            chat_model: self.chat_model,
            embedding_model: self.embedding_model,
            image_model: Some(model),
            audio_model: self.audio_model,
            enabled_tools: self.enabled_tools,
            enabled_skills: self.enabled_skills,
            enabled_apps: self.enabled_apps,
            _chat: self._chat,
            _embed: self._embed,
            _image: PhantomData,
        }
    }
}

// Only allow build when chat model is set (minimum requirement)
impl<E, I> ProfileBuilder<HasChatModel, E, I> {
    pub fn build(self) -> Result<Profile, ProfileError> {
        let name = self.name.ok_or(ProfileError::MissingName)?;
        let chat_model = self.chat_model.ok_or(ProfileError::MissingChatModel)?;
        
        Ok(Profile {
            name,
            chat_model,
            embedding_model: self.embedding_model,
            image_model: self.image_model,
            audio_model: self.audio_model,
            enabled_tools: self.enabled_tools,
            enabled_skills: self.enabled_skills,
            enabled_apps: self.enabled_apps,
        })
    }
}
```

---

## 5. Dynamic Dispatch for Runtime Flexibility

For UI interactions where you need runtime polymorphism:

```rust
/// Runtime capability flags (for UI/dynamic scenarios)
#[derive(Clone, Debug, Default)]
pub struct Capabilities {
    pub vision: bool,
    pub image_generation: bool,
    pub audio_input: bool,
    pub audio_output: bool,
    pub function_calling: bool,
    pub web_search: bool,
    pub file_search: bool,
    pub code_execution: bool,
    pub computer_use: bool,
    pub thinking: bool,
    pub streaming: bool,
    pub structured_output: bool,
    pub mcp: bool,
}

/// Dynamic text model trait for runtime polymorphism
#[async_trait]
pub trait TextModelTrait: Send + Sync {
    fn id(&self) -> &str;
    fn provider(&self) -> Provider;
    fn capabilities(&self) -> &Capabilities;
    
    async fn generate(&self, request: &ChatRequest) -> Result<ChatResponse, ApiError>;
    async fn stream(&self, request: &ChatRequest) -> Result<BoxStream<'_, Result<ChatChunk, ApiError>>, ApiError>;
}

/// Type-erased model for storage
pub type DynTextModel = Arc<dyn TextModelTrait>;

/// Bridge: Convert typed model to dynamic
impl<V, I, Ai, Ao, F, W, Fs, C, Cu, T, St, So, M> From<Model<V, I, Ai, Ao, F, W, Fs, C, Cu, T, St, So, M>> 
    for DynTextModel 
where
    V: CapabilityState + Send + Sync + 'static,
    I: CapabilityState + Send + Sync + 'static,
    // ... etc
{
    fn from(model: Model<V, I, Ai, Ao, F, W, Fs, C, Cu, T, St, So, M>) -> Self {
        Arc::new(TypedModelWrapper(model))
    }
}
```

---

## 6. Pre-defined Model Instances

### Model Registry with Known Capabilities

```rust
pub mod models {
    use super::*;
    
    // OpenAI Models
    pub fn gpt_5_1() -> Model<Yes, No, No, No, Yes, Yes, Yes, Yes, No, Yes, Yes, Yes, Yes> {
        Model {
            id: "gpt-5.1".into(),
            display_name: "GPT-5.1".into(),
            provider: Provider::OpenAI,
            input_token_limit: 400_000,
            output_token_limit: 128_000,
            _vision: PhantomData,
            _image_gen: PhantomData,
            _audio_in: PhantomData,
            _audio_out: PhantomData,
            _fn_call: PhantomData,
            _web_search: PhantomData,
            _file_search: PhantomData,
            _code_exec: PhantomData,
            _comp_use: PhantomData,
            _thinking: PhantomData,
            _streaming: PhantomData,
            _struct_out: PhantomData,
            _mcp: PhantomData,
        }
    }
    
    pub fn gpt_5_mini() -> Model<Yes, No, No, No, Yes, Yes, Yes, Yes, No, Yes, Yes, Yes, Yes> {
        Model {
            id: "gpt-5-mini".into(),
            display_name: "GPT-5 mini".into(),
            provider: Provider::OpenAI,
            input_token_limit: 200_000,
            output_token_limit: 64_000,
            ..Default::default() // Would need custom Default impl
        }
    }
    
    // Anthropic Models
    pub fn claude_sonnet_4_5() -> Model<Yes, No, No, No, Yes, Yes, No, Yes, Yes, Yes, Yes, Yes, Yes> {
        Model {
            id: "claude-sonnet-4-20250514".into(),
            display_name: "Claude Sonnet 4.5".into(),
            provider: Provider::Anthropic,
            input_token_limit: 1_000_000,
            output_token_limit: 128_000,
            // All phantoms...
        }
    }
    
    pub fn claude_opus_4_5() -> Model<Yes, No, No, No, Yes, Yes, No, Yes, Yes, Yes, Yes, Yes, Yes> {
        Model {
            id: "claude-opus-4-20250514".into(),
            display_name: "Claude Opus 4.5".into(),
            provider: Provider::Anthropic,
            input_token_limit: 1_000_000,
            output_token_limit: 128_000,
            // ...
        }
    }
    
    // Gemini Models
    pub fn gemini_2_5_pro() -> Model<Yes, No, Yes, No, Yes, Yes, Yes, Yes, No, Yes, Yes, Yes, No> {
        Model {
            id: "gemini-2.5-pro".into(),
            display_name: "Gemini 2.5 Pro".into(),
            provider: Provider::Gemini,
            input_token_limit: 1_048_576,
            output_token_limit: 65_536,
            // ...
        }
    }
    
    pub fn gemini_2_5_flash() -> Model<Yes, No, Yes, No, Yes, Yes, Yes, Yes, No, Yes, Yes, Yes, No> {
        Model {
            id: "gemini-2.5-flash".into(),
            display_name: "Gemini 2.5 Flash".into(),
            provider: Provider::Gemini,
            input_token_limit: 1_048_576,
            output_token_limit: 65_536,
            // ...
        }
    }
    
    // Image Models
    pub fn gpt_image_1() -> ImageModel {
        ImageModel {
            id: "gpt-image-1".into(),
            provider: Provider::OpenAI,
            max_resolution: (4096, 4096),
            supports_editing: true,
            supports_variations: true,
        }
    }
    
    pub fn gemini_imagen() -> ImageModel {
        ImageModel {
            id: "imagen-4".into(),
            provider: Provider::Gemini,
            max_resolution: (2048, 2048),
            supports_editing: false,
            supports_variations: false,
        }
    }
    
    // Embedding Models
    pub fn text_embedding_3_large() -> EmbeddingModel {
        EmbeddingModel {
            id: "text-embedding-3-large".into(),
            provider: Provider::OpenAI,
            dimensions: 3072,
            max_input_tokens: 8191,
        }
    }
    
    pub fn gemini_embedding_004() -> EmbeddingModel {
        EmbeddingModel {
            id: "text-embedding-004".into(),
            provider: Provider::Gemini,
            dimensions: 768,
            max_input_tokens: 2048,
        }
    }
}
```

---

## 7. Tool/Skill/App Definitions

### Matching Your Current AppCapability Structure

```rust
/// Tool definition (e.g., Web Search, Code Execution)
#[derive(Clone, Debug)]
pub struct ToolDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub required_capability: CapabilityRequirement,
}

/// What capability a tool requires from the model
#[derive(Clone, Debug)]
pub enum CapabilityRequirement {
    WebSearch,
    FileSearch,
    CodeExecution,
    FunctionCalling,
    Vision,
    ComputerUse,
    Thinking,
    None, // No special requirement
}

/// Skill definition (e.g., Study, Canvas)
#[derive(Clone, Debug)]
pub struct SkillDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub prompt_template: String,
    pub required_tools: Vec<String>,
}

/// Mini-app definition (e.g., Canva, Figma integrations)
#[derive(Clone, Debug)]
pub struct MiniAppDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub oauth_config: Option<OAuthConfig>,
    pub mcp_server_url: Option<String>,
}

#[derive(Clone, Debug)]
pub struct OAuthConfig {
    pub client_id: String,
    pub auth_url: String,
    pub token_url: String,
    pub scopes: Vec<String>,
}
```

### Tool Registry

```rust
pub mod tools {
    use super::*;
    
    pub fn web_search() -> ToolDefinition {
        ToolDefinition {
            id: "web_search".into(),
            name: "Web Search".into(),
            description: "Search the web for current information".into(),
            icon: "icons/web_search.svg".into(),
            required_capability: CapabilityRequirement::WebSearch,
        }
    }
    
    pub fn image_generation() -> ToolDefinition {
        ToolDefinition {
            id: "image_generation".into(),
            name: "Image Generation".into(),
            description: "Generate images from text prompts".into(),
            icon: "icons/create_image.svg".into(),
            required_capability: CapabilityRequirement::None, // Uses separate ImageModel
        }
    }
    
    pub fn thinking() -> ToolDefinition {
        ToolDefinition {
            id: "thinking".into(),
            name: "Thinking".into(),
            description: "Extended reasoning for complex problems".into(),
            icon: "icons/thinking.svg".into(),
            required_capability: CapabilityRequirement::Thinking,
        }
    }
    
    pub fn deep_research() -> ToolDefinition {
        ToolDefinition {
            id: "deep_research".into(),
            name: "Deep Research".into(),
            description: "In-depth research with multiple sources".into(),
            icon: "icons/deep_search.svg".into(),
            required_capability: CapabilityRequirement::WebSearch, // Requires web search
        }
    }
    
    pub fn code_execution() -> ToolDefinition {
        ToolDefinition {
            id: "code_execution".into(),
            name: "Code Execution".into(),
            description: "Run code in a sandboxed environment".into(),
            icon: "icons/code.svg".into(),
            required_capability: CapabilityRequirement::CodeExecution,
        }
    }
    
    pub fn photos() -> ToolDefinition {
        ToolDefinition {
            id: "photos".into(),
            name: "Photos".into(),
            description: "Add photos & files".into(),
            icon: "icons/clip.svg".into(),
            required_capability: CapabilityRequirement::Vision,
        }
    }
}

pub mod skills {
    use super::*;
    
    pub fn study() -> SkillDefinition {
        SkillDefinition {
            id: "study".into(),
            name: "Study".into(),
            description: "Interactive learning and quizzes".into(),
            icon: "icons/study.svg".into(),
            prompt_template: "Help me study: {topic}".into(),
            required_tools: vec![],
        }
    }
    
    pub fn canvas() -> SkillDefinition {
        SkillDefinition {
            id: "canvas".into(),
            name: "Canvas".into(),
            description: "Visual thinking canvas".into(),
            icon: "icons/canvas.svg".into(),
            prompt_template: "".into(),
            required_tools: vec![],
        }
    }
}

pub mod apps {
    use super::*;
    
    pub fn canva() -> MiniAppDefinition {
        MiniAppDefinition {
            id: "canva".into(),
            name: "Canva".into(),
            description: "Design with Canva".into(),
            icon: "icons/canva.svg".into(),
            oauth_config: Some(OAuthConfig {
                client_id: "".into(),
                auth_url: "https://www.canva.com/oauth/authorize".into(),
                token_url: "https://www.canva.com/oauth/token".into(),
                scopes: vec!["design:read".into(), "design:write".into()],
            }),
            mcp_server_url: None,
        }
    }
    
    pub fn figma() -> MiniAppDefinition {
        MiniAppDefinition {
            id: "figma".into(),
            name: "Figma".into(),
            description: "Design with Figma".into(),
            icon: "icons/figma.svg".into(),
            oauth_config: Some(OAuthConfig {
                client_id: "".into(),
                auth_url: "https://www.figma.com/oauth".into(),
                token_url: "https://www.figma.com/api/oauth/token".into(),
                scopes: vec!["files:read".into()],
            }),
            mcp_server_url: None,
        }
    }
}
```

---

## 8. Profile with Capability Validation

### The Final Profile Type

```rust
/// A complete profile configuration
#[derive(Clone, Debug)]
pub struct Profile {
    pub name: String,
    pub chat_model: DynTextModel,
    pub embedding_model: Option<EmbeddingModel>,
    pub image_model: Option<ImageModel>,
    pub audio_model: Option<AudioModel>,
    pub enabled_tools: Vec<ToolDefinition>,
    pub enabled_skills: Vec<SkillDefinition>,
    pub enabled_apps: Vec<MiniAppDefinition>,
}

impl Profile {
    /// Get tools that are compatible with the chat model's capabilities
    pub fn available_tools(&self) -> Vec<&ToolDefinition> {
        let caps = self.chat_model.capabilities();
        
        self.enabled_tools.iter().filter(|tool| {
            match tool.required_capability {
                CapabilityRequirement::WebSearch => caps.web_search,
                CapabilityRequirement::FileSearch => caps.file_search,
                CapabilityRequirement::CodeExecution => caps.code_execution,
                CapabilityRequirement::FunctionCalling => caps.function_calling,
                CapabilityRequirement::Vision => caps.vision,
                CapabilityRequirement::ComputerUse => caps.computer_use,
                CapabilityRequirement::Thinking => caps.thinking,
                CapabilityRequirement::None => true,
            }
        }).collect()
    }
    
    /// Check if a specific tool is usable with current profile
    pub fn can_use_tool(&self, tool_id: &str) -> bool {
        self.available_tools().iter().any(|t| t.id == tool_id)
    }
    
    /// Check if image generation is available
    pub fn can_generate_images(&self) -> bool {
        self.image_model.is_some()
    }
    
    /// Check if embeddings are available  
    pub fn can_embed(&self) -> bool {
        self.embedding_model.is_some()
    }
}
```

---

## 9. Integration with Existing UI (profile_settings.rs)

### Updated AppState

```rust
// In src/state.rs

use crate::models::{Profile, ProfileBuilder, DynTextModel, EmbeddingModel, ImageModel};
use crate::models::registry::{ModelRegistry, ToolRegistry, SkillRegistry, AppRegistry};

pub struct AppState {
    // ... existing fields ...
    
    // New profile system
    pub model_registry: ModelRegistry,
    pub tool_registry: ToolRegistry,
    pub skill_registry: SkillRegistry,
    pub app_registry: AppRegistry,
    
    pub profiles: Vec<Profile>,
    pub active_profile: Option<Profile>,
    
    // Filtered capabilities based on active profile
    pub available_capabilities: Vec<AppCapability>,
}

impl AppState {
    pub fn set_active_profile(&mut self, profile: Profile, cx: &mut Context<Self>) {
        // Update available capabilities based on profile
        self.available_capabilities = self.compute_available_capabilities(&profile);
        self.active_profile = Some(profile);
        cx.notify();
    }
    
    fn compute_available_capabilities(&self, profile: &Profile) -> Vec<AppCapability> {
        let mut caps = Vec::new();
        let model_caps = profile.chat_model.capabilities();
        
        // Photos - requires vision
        if model_caps.vision {
            caps.push(AppCapability {
                name: "Photos".into(),
                label: "Add photos & files".into(),
                icon: "icons/clip.svg".into(),
                action_id: "SelectAppPhotos".into(),
                is_primary: true,
            });
        }
        
        // Image Generation - requires image model
        if profile.image_model.is_some() {
            caps.push(AppCapability {
                name: "Image Generation".into(),
                label: "Image Generation".into(),
                icon: "icons/create_image.svg".into(),
                action_id: "SelectAppImageGeneration".into(),
                is_primary: true,
            });
        }
        
        // Thinking - requires thinking capability
        if model_caps.thinking {
            caps.push(AppCapability {
                name: "Thinking".into(),
                label: "Thinking".into(),
                icon: "icons/thinking.svg".into(),
                action_id: "SelectAppThinking".into(),
                is_primary: true,
            });
        }
        
        // Deep Research - requires web search
        if model_caps.web_search {
            caps.push(AppCapability {
                name: "Deep Research".into(),
                label: "Deep Research".into(),
                icon: "icons/deep_search.svg".into(),
                action_id: "SelectAppDeepResearch".into(),
                is_primary: true,
            });
            
            caps.push(AppCapability {
                name: "Web search".into(),
                label: "Web search".into(),
                icon: "icons/web_search.svg".into(),
                action_id: "SelectAppWebSearch".into(),
                is_primary: false,
            });
        }
        
        // Skills (always available)
        caps.push(AppCapability {
            name: "Study".into(),
            label: "Study".into(),
            icon: "icons/study.svg".into(),
            action_id: "SelectAppStudy".into(),
            is_primary: true,
        });
        
        caps.push(AppCapability {
            name: "Canvas".into(),
            label: "Canvas".into(),
            icon: "icons/canvas.svg".into(),
            action_id: "SelectAppCanvas".into(),
            is_primary: false,
        });
        
        // Mini-apps from enabled apps
        for app in &profile.enabled_apps {
            caps.push(AppCapability {
                name: app.name.clone(),
                label: app.name.clone(),
                icon: app.icon.clone(),
                action_id: format!("SelectApp{}", app.name.replace(" ", "")),
                is_primary: false,
            });
        }
        
        caps
    }
}
```

### Updated ProfileSettingsModal

```rust
// In src/components/modals/profile_settings.rs

use crate::models::{Profile, DynTextModel, EmbeddingModel, ImageModel};
use crate::models::registry::ModelRegistry;

pub struct ProfileSettingsModal {
    state: Entity<AppState>,
    
    // Form state
    profile_name_input: Entity<InputState>,
    
    // Model selection (dropdowns would be better than text inputs)
    selected_provider: Provider,
    selected_chat_model: Option<DynTextModel>,
    selected_embedding_provider: Provider,
    selected_embedding_model: Option<EmbeddingModel>,
    selected_image_provider: Provider,
    selected_image_model: Option<ImageModel>,
    
    // API keys per provider
    api_keys: HashMap<Provider, String>,
    
    // Preview of available capabilities
    preview_capabilities: Vec<AppCapability>,
}

impl ProfileSettingsModal {
    fn on_chat_model_changed(&mut self, model: DynTextModel, cx: &mut Context<Self>) {
        self.selected_chat_model = Some(model.clone());
        
        // Update capability preview
        self.preview_capabilities = self.compute_preview_capabilities();
        cx.notify();
    }
    
    fn compute_preview_capabilities(&self) -> Vec<AppCapability> {
        let Some(chat_model) = &self.selected_chat_model else {
            return Vec::new();
        };
        
        let caps = chat_model.capabilities();
        let mut result = Vec::new();
        
        if caps.vision {
            result.push(/* Photos capability */);
        }
        if caps.thinking {
            result.push(/* Thinking capability */);
        }
        // ... etc
        
        result
    }
    
    fn save_profile(&mut self, cx: &mut Context<Self>) {
        let Some(chat_model) = self.selected_chat_model.clone() else {
            // Show error - chat model required
            return;
        };
        
        let profile = Profile {
            name: self.profile_name_input.read(cx).value(),
            chat_model,
            embedding_model: self.selected_embedding_model.clone(),
            image_model: self.selected_image_model.clone(),
            audio_model: None,
            enabled_tools: Vec::new(), // Configure separately
            enabled_skills: Vec::new(),
            enabled_apps: Vec::new(),
        };
        
        self.state.update(cx, |state, cx| {
            state.profiles.push(profile.clone());
            state.set_active_profile(profile, cx);
        });
    }
}
```

---

## 10. API Client Architecture

### Provider-Specific Clients

```rust
/// OpenAI API client
pub struct OpenAIClient {
    api_key: String,
    client: reqwest::Client,
    base_url: String,
}

impl OpenAIClient {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
            base_url: "https://api.openai.com/v1".into(),
        }
    }
    
    pub async fn chat(&self, model: &str, request: ChatRequest) -> Result<ChatResponse, ApiError> {
        let response = self.client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await?;
        
        // Handle response...
        todo!()
    }
    
    pub async fn generate_image(&self, model: &str, prompt: &str, size: (u32, u32)) -> Result<Vec<u8>, ApiError> {
        // POST /v1/images/generations
        todo!()
    }
    
    pub async fn embed(&self, model: &str, text: &str) -> Result<Vec<f32>, ApiError> {
        // POST /v1/embeddings
        todo!()
    }
}

/// Anthropic API client
pub struct AnthropicClient {
    api_key: String,
    client: reqwest::Client,
}

impl AnthropicClient {
    pub async fn messages(&self, model: &str, request: MessagesRequest) -> Result<MessagesResponse, ApiError> {
        // POST /v1/messages
        todo!()
    }
}

/// Gemini API client  
pub struct GeminiClient {
    api_key: String,
    client: reqwest::Client,
}

impl GeminiClient {
    pub async fn generate_content(&self, model: &str, request: GenerateContentRequest) -> Result<GenerateContentResponse, ApiError> {
        // POST /v1beta/models/{model}:generateContent
        todo!()
    }
    
    pub async fn stream_generate_content(&self, model: &str, request: GenerateContentRequest) -> Result<impl Stream<Item = Result<GenerateContentResponse, ApiError>>, ApiError> {
        // POST /v1beta/models/{model}:streamGenerateContent
        todo!()
    }
}

/// Unified client that routes to provider-specific implementations
pub struct UnifiedClient {
    openai: Option<OpenAIClient>,
    anthropic: Option<AnthropicClient>,
    gemini: Option<GeminiClient>,
}

impl UnifiedClient {
    pub fn configure_provider(&mut self, provider: Provider, api_key: String) {
        match provider {
            Provider::OpenAI => self.openai = Some(OpenAIClient::new(api_key)),
            Provider::Anthropic => self.anthropic = Some(AnthropicClient::new(api_key)),
            Provider::Gemini => self.gemini = Some(GeminiClient::new(api_key)),
        }
    }
    
    pub async fn chat(&self, model: &DynTextModel, request: ChatRequest) -> Result<ChatResponse, ApiError> {
        match model.provider() {
            Provider::OpenAI => {
                let client = self.openai.as_ref().ok_or(ApiError::NotConfigured(Provider::OpenAI))?;
                client.chat(model.id(), request).await
            }
            Provider::Anthropic => {
                let client = self.anthropic.as_ref().ok_or(ApiError::NotConfigured(Provider::Anthropic))?;
                // Convert to Anthropic format...
                todo!()
            }
            Provider::Gemini => {
                let client = self.gemini.as_ref().ok_or(ApiError::NotConfigured(Provider::Gemini))?;
                // Convert to Gemini format...
                todo!()
            }
        }
    }
}
```

---

## 11. File Structure Proposal

```
src/
├── models/
│   ├── mod.rs              # Re-exports
│   ├── capabilities.rs     # Capability markers and traits
│   ├── model.rs            # Model<...> type with phantom types
│   ├── text_model.rs       # TextModel wrapper
│   ├── image_model.rs      # ImageModel
│   ├── embedding_model.rs  # EmbeddingModel
│   ├── audio_model.rs      # AudioModel
│   ├── video_model.rs      # VideoModel
│   ├── profile.rs          # Profile and ProfileBuilder
│   └── registry.rs         # Pre-defined models, tools, skills, apps
├── api/
│   ├── mod.rs
│   ├── client.rs           # UnifiedClient
│   ├── openai.rs           # OpenAI-specific
│   ├── anthropic.rs        # Anthropic-specific
│   ├── gemini.rs           # Gemini-specific
│   ├── types.rs            # Request/Response types
│   └── error.rs            # ApiError
├── tools/
│   ├── mod.rs
│   ├── definition.rs       # ToolDefinition, SkillDefinition, etc.
│   ├── web_search.rs       # Web search implementation
│   ├── code_execution.rs   # Code execution implementation
│   └── ...
└── ...
```

---

## 12. Usage Examples

### Creating a Profile Programmatically

```rust
use models::{ProfileBuilder, models, tools, skills, apps};

// Compile-time safe profile creation
let profile = ProfileBuilder::new()
    .name("My GPT Profile")
    .chat_model(models::gpt_5_1().into())  // Type-safe conversion to DynTextModel
    .embedding_model(models::text_embedding_3_large())
    .image_model(models::gpt_image_1())
    .build()?;

// This would fail to compile - missing chat_model:
// let invalid = ProfileBuilder::new()
//     .name("Invalid")
//     .build(); // Error: build() not available for NoChatModel state
```

### Using Type-Safe Model Methods

```rust
// GPT-5.1 has vision capability
let gpt = models::gpt_5_1();

// This compiles - GPT-5.1 has Vision = Yes
let result = gpt.analyze_image(&image_bytes, "What's in this image?").await?;

// This compiles - GPT-5.1 has Thinking = Yes
let thought = gpt.think("Solve this math problem...", 10000).await?;

// This would NOT compile - GPT-5.1 has CompUse = No
// let action = gpt.computer_action(&screenshot, "Click the button").await?;

// For that, use Claude with computer use:
let claude = models::claude_sonnet_4_5();
let action = claude.computer_action(&screenshot, "Click the button").await?;
```

### Dynamic Runtime Usage

```rust
// In UI code where you need runtime flexibility
let profile = app_state.active_profile.as_ref().unwrap();

// Check capabilities at runtime for UI rendering
if profile.chat_model.capabilities().thinking {
    // Show thinking button
}

if profile.can_generate_images() {
    // Show image generation button
}

// Get only tools compatible with current model
let available_tools = profile.available_tools();
for tool in available_tools {
    // Render tool button
}
```

---

## 13. WASM Component Examples

### Example: Web Search Tool (Rust -> WASM)

```rust
// tools/web-search/src/lib.rs
wit_bindgen::generate!({
    world: "tool",
    path: "../../wit",
});

use exports::nativechat::tool::tool_executor::{Guest as ToolExecutor, ToolInput, ToolOutput, Artifact};
use exports::nativechat::tool::tool_info::{Guest as ToolInfo, ToolMetadata};
use nativechat::capabilities::http_client;
use nativechat::capabilities::permissions;

struct WebSearchTool;

impl ToolInfo for WebSearchTool {
    fn get_metadata() -> ToolMetadata {
        ToolMetadata {
            id: "web_search".into(),
            name: "Web Search".into(),
            description: "Search the web for current information".into(),
            icon: "icons/web_search.svg".into(),
            required_permissions: vec!["http".into()],
            required_capabilities: vec!["web_search".into()],
        }
    }
}

impl ToolExecutor for WebSearchTool {
    fn execute(input: ToolInput) -> Result<ToolOutput, String> {
        // Check permission
        if !permissions::has_permission("http") {
            return Err("HTTP permission not granted".into());
        }
        
        // Parse arguments
        let args: serde_json::Value = serde_json::from_str(&input.arguments)
            .map_err(|e| format!("Invalid arguments: {}", e))?;
        
        let query = args["query"].as_str()
            .ok_or("Missing 'query' argument")?;
        
        // Make search request
        let response = http_client::fetch(&http_client::HttpRequest {
            method: "GET".into(),
            url: format!("https://api.search.example.com/search?q={}", urlencoding::encode(query)),
            headers: vec![],
            body: None,
        })?;
        
        if response.status != 200 {
            return Err(format!("Search failed with status {}", response.status));
        }
        
        let body = String::from_utf8(response.body)
            .map_err(|e| format!("Invalid response: {}", e))?;
        
        Ok(ToolOutput {
            result: body,
            artifacts: None,
        })
    }
}

export!(WebSearchTool);
```

### Example: Study Skill (Rust -> WASM)

```rust
// skills/study/src/lib.rs
wit_bindgen::generate!({
    world: "skill",
    path: "../../wit",
});

use exports::nativechat::skill::skill_info::{Guest as SkillInfo, SkillMetadata};
use exports::nativechat::skill::skill_orchestrator::{Guest as SkillOrchestrator, SkillContext, SkillAction, ActionType};
use nativechat::capabilities::model_api;

struct StudySkill {
    state: StudyState,
}

enum StudyState {
    Initial,
    AskingQuestion { topic: String, question_index: usize },
    WaitingForAnswer { topic: String, question: String },
}

impl SkillInfo for StudySkill {
    fn get_metadata() -> SkillMetadata {
        SkillMetadata {
            id: "study".into(),
            name: "Study".into(),
            description: "Interactive learning and quizzes".into(),
            icon: "icons/study.svg".into(),
            required_tools: vec![],
            prompt_template: Some("Help me study: {topic}".into()),
        }
    }
}

impl SkillOrchestrator for StudySkill {
    fn process(context: SkillContext) -> Result<SkillAction, String> {
        // Use the model API to generate study questions
        let request = model_api::ChatRequest {
            messages: vec![
                model_api::Message {
                    role: "system".into(),
                    content: "You are a study assistant. Generate quiz questions.".into(),
                    images: None,
                },
                model_api::Message {
                    role: "user".into(),
                    content: context.user_input.clone(),
                    images: None,
                },
            ],
            temperature: Some(0.7),
            max_tokens: Some(1000),
            tools: None,
        };
        
        let response = model_api::generate(&request)?;
        
        Ok(SkillAction {
            action_type: ActionType::SendMessage,
            payload: serde_json::json!({
                "content": response.content
            }).to_string(),
        })
    }
    
    fn handle_tool_result(_tool_name: String, _result: String) -> Result<SkillAction, String> {
        Ok(SkillAction {
            action_type: ActionType::Complete,
            payload: "{}".into(),
        })
    }
}

export!(StudySkill);
```

### Building WASM Components

```bash
# Install cargo-component
cargo install cargo-component

# Build a tool
cd tools/web-search
cargo component build --release --target wasm32-wasip2

# Build a skill  
cd skills/study
cargo component build --release --target wasm32-wasip2

# Output: target/wasm32-wasip2/release/web_search.wasm
```

---

## 14. Project Structure with WASM

```
nativechat/
├── Cargo.toml                 # Workspace root
├── src/                       # Main native app
│   ├── main.rs
│   ├── state.rs
│   ├── components/
│   │   └── modals/
│   │       └── profile_settings.rs
│   ├── models/                # Model definitions & registry
│   │   ├── mod.rs
│   │   ├── capabilities.rs    # Phantom type markers
│   │   ├── model.rs           # Model<...> type
│   │   ├── profile.rs         # Profile & ProfileBuilder
│   │   └── registry.rs        # Pre-defined models
│   ├── runtime/               # WASM runtime
│   │   ├── mod.rs
│   │   ├── host.rs            # Host state & permissions
│   │   ├── loader.rs          # Component loader
│   │   └── bindings.rs        # Generated from WIT
│   └── api/                   # Provider API clients
│       ├── mod.rs
│       ├── openai.rs
│       ├── anthropic.rs
│       └── gemini.rs
├── wit/                       # WIT interface definitions
│   ├── capabilities.wit       # Host capabilities
│   ├── tool.wit               # Tool world
│   ├── skill.wit              # Skill world
│   └── miniapp.wit            # Mini-app world
├── tools/                     # WASM tool components
│   ├── web-search/
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   ├── code-execution/
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   └── image-generation/
│       ├── Cargo.toml
│       └── src/lib.rs
├── skills/                    # WASM skill components
│   ├── study/
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   └── canvas/
│       ├── Cargo.toml
│       └── src/lib.rs
├── apps/                      # WASM mini-app components
│   ├── canva/
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   └── figma/
│       ├── Cargo.toml
│       └── src/lib.rs
├── components/                # Pre-built WASM components
│   ├── tools/
│   │   ├── web_search.wasm
│   │   └── code_execution.wasm
│   ├── skills/
│   │   └── study.wasm
│   └── apps/
│       └── canva.wasm
└── ui/                        # UI components crate
    └── ...
```

---

## 15. Updated Cargo.toml Dependencies

```toml
[workspace]
members = [".", "ui", "tools/*", "skills/*", "apps/*"]
resolver = "2"

[package]
name = "nativechat"
version = "0.1.0"
edition = "2024"

[dependencies]
# Existing
gpui.workspace = true
anyhow.workspace = true
tokio = { version = "1.36", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# WASM Runtime
wasmtime = { version = "27", features = ["component-model", "async"] }
wasmtime-wasi = { version = "27", features = ["preview2"] }
wit-bindgen = "0.36"

# HTTP client for provider APIs
reqwest = { version = "0.12", features = ["json"] }

# For component tools
[workspace.dependencies]
wit-bindgen = "0.36"
```

---

## 16. Summary

### Architecture Benefits with WASM/WASI

1. **Sandboxed Execution**: Tools, skills, and mini-apps run in isolated WASM sandboxes
2. **Capability-Based Security**: Host explicitly grants permissions (HTTP, secrets, model access)
3. **Dynamic Loading**: Load/unload components at runtime without recompiling the host
4. **Language Agnostic**: Components can be written in any language that compiles to WASM
5. **Hot Reload**: Update components without restarting the app
6. **Type-Safe Contracts**: WIT interfaces define clear contracts between host and guests
7. **Model Decoupling**: Text/Image/Embedding/Audio models are separate, composable units

### Type-State Pattern Benefits

1. **Compile-Time Safety**: Invalid capability usage caught at compile time
2. **Zero Runtime Cost**: Phantom types have no memory overhead  
3. **Builder Validation**: ProfileBuilder ensures valid configurations
4. **IDE Support**: Autocomplete only shows valid methods for model capabilities

### Implementation Roadmap

1. **Phase 1: WIT Definitions**
   - Define `wit/capabilities.wit` with host interfaces
   - Define `wit/tool.wit`, `wit/skill.wit`, `wit/miniapp.wit`

2. **Phase 2: Host Runtime**
   - Set up Wasmtime with component model support
   - Implement host capability interfaces (HTTP, secrets, model API, UI)
   - Create component loader with permission management

3. **Phase 3: Model Registry**
   - Implement type-state `Model<...>` with phantom capabilities
   - Create pre-defined model instances for each provider
   - Build `ProfileBuilder` with type-state validation

4. **Phase 4: Built-in Tools**
   - Implement `web-search.wasm` component
   - Implement `code-execution.wasm` component
   - Implement `image-generation.wasm` (wraps ImageModel)

5. **Phase 5: Skills**
   - Implement `study.wasm` skill
   - Implement `canvas.wasm` skill

6. **Phase 6: Mini-Apps**
   - Implement OAuth flow support
   - Create `canva.wasm`, `figma.wasm` integrations

7. **Phase 7: UI Integration**
   - Update `ProfileSettingsModal` with model dropdowns
   - Filter `capabilities` in chat input based on active profile
   - Add component management UI

### Key Files to Create

| File | Purpose |
|------|---------|
| `wit/capabilities.wit` | Host capability interfaces |
| `wit/tool.wit` | Tool component world |
| `wit/skill.wit` | Skill component world |
| `wit/miniapp.wit` | Mini-app component world |
| `src/runtime/mod.rs` | WASM runtime initialization |
| `src/runtime/host.rs` | Host state and capability implementations |
| `src/models/capabilities.rs` | Type-state capability markers |
| `src/models/profile.rs` | Profile and ProfileBuilder |
| `tools/web-search/src/lib.rs` | Web search tool component |
| `skills/study/src/lib.rs` | Study skill component |
