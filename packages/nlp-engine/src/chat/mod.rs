/// Chat inference engine using llama.cpp.
///
/// Provides streaming text generation from GGUF chat models with tool-call
/// parsing for the Mistral raw format. Designed to coexist with the embedding
/// service on the same GPU (validated in PoC with shared Metal backend).
///
/// # Architecture
///
/// The `ChatEngine` lives in the nlp-engine crate and exposes a
/// crate-local API. The app crate wraps it to implement the
/// `ChatInferenceEngine` trait — the same pattern used for embeddings.
///
/// # GPU Scheduling
///
/// A `tokio::sync::Mutex` serializes all inference requests so that only
/// one generation runs at a time. This prevents Metal command-buffer
/// collisions between concurrent requests.
pub mod error;
pub mod parser;
pub mod types;

pub use error::{ChatError, Result};
pub use parser::{parse_tool_calls, ParseResult, ParsedToolCall, StreamingToolCallParser};
pub use types::{ChatChunk, ChatConfig, ChatMessageInput, ChatUsage, LoadedModelInfo, ToolSpec};

#[cfg(feature = "chat-service")]
use crate::embedding::{get_or_init_backend, register_atexit_handler};

#[cfg(feature = "chat-service")]
use llama_cpp_2::context::params::LlamaContextParams;
#[cfg(feature = "chat-service")]
use llama_cpp_2::context::LlamaContext;
#[cfg(feature = "chat-service")]
use llama_cpp_2::model::params::LlamaModelParams;
#[cfg(feature = "chat-service")]
use llama_cpp_2::model::{AddBos, LlamaModel};
#[cfg(feature = "chat-service")]
use llama_cpp_2::openai::OpenAIChatTemplateParams;
#[cfg(feature = "chat-service")]
use llama_cpp_2::sampling::LlamaSampler;

#[cfg(feature = "chat-service")]
use std::sync::{Arc, Mutex};

/// Helper to convert backend init errors into ChatError.
#[cfg(feature = "chat-service")]
fn backend() -> Result<crate::embedding::BackendGuard> {
    get_or_init_backend().map_err(ChatError::ModelLoadError)
}

/// Chat inference engine backed by llama.cpp.
///
/// Loads a GGUF chat model and provides streaming text generation.
/// Thread-safe: a `tokio::sync::Mutex` serializes inference requests.
pub struct ChatEngine {
    config: ChatConfig,
    #[cfg(feature = "chat-service")]
    state: Arc<Mutex<Option<ChatLlamaState>>>,
    #[cfg(feature = "chat-service")]
    inference_lock: tokio::sync::Mutex<()>,
}

/// Internal state holding the loaded model and its context.
///
/// # Safety
///
/// Uses the same lifetime-extension pattern as `embedding.rs::LlamaState`.
/// The context is created with a transmuted `'static` lifetime because:
/// 1. The context is stored alongside the model that owns it.
/// 2. Drop order is guaranteed: context drops before model.
/// 3. Access is serialized through the outer Mutex.
#[cfg(feature = "chat-service")]
struct ChatLlamaState {
    model: LlamaModel,
    context: Option<LlamaContext<'static>>,
    model_path: String,
    context_size: u32,
    n_threads: i32,
    /// Cached `[TOOL_CALLS]` control token id, resolved once at load time.
    /// `None` if the model's vocab does not contain such a token (e.g. Gemma 4,
    /// which emits tool calls as plain text rather than a control token).
    tool_calls_token_id: Option<llama_cpp_2::token::LlamaToken>,
}

#[cfg(feature = "chat-service")]
impl ChatLlamaState {
    fn new(model: LlamaModel, model_path: String, context_size: u32, n_threads: i32) -> Self {
        let tool_calls_token_id = detect_tool_calls_token(&model);
        Self {
            model,
            context: None,
            model_path,
            context_size,
            n_threads,
            tool_calls_token_id,
        }
    }

    /// Get or create the generation context.
    ///
    /// Unlike the embedding context, the chat context does NOT use embeddings
    /// mode and has a fixed batch size matching the context window.
    fn get_or_create_context(&mut self) -> Result<&mut LlamaContext<'static>> {
        if self.context.is_none() {
            tracing::info!(
                "Creating chat LlamaContext (n_ctx={}, n_threads={})",
                self.context_size,
                self.n_threads,
            );

            let ctx_params = LlamaContextParams::default()
                .with_n_ctx(std::num::NonZeroU32::new(self.context_size))
                .with_n_batch(self.context_size)
                .with_n_threads(self.n_threads)
                .with_n_threads_batch(self.n_threads);

            let backend = backend()?;
            let ctx = self.model.new_context(&backend, ctx_params).map_err(|e| {
                ChatError::InferenceError(format!("Context creation failed: {}", e))
            })?;

            // SAFETY: Same pattern as embedding.rs. The context is stored in this
            // struct alongside model. Drop order is guaranteed (context before model).
            let ctx: LlamaContext<'static> = unsafe { std::mem::transmute(ctx) };
            self.context = Some(ctx);

            tracing::info!("Chat context created — Metal kernels compiled");
        }

        Ok(self.context.as_mut().expect("context just created"))
    }
}

#[cfg(feature = "chat-service")]
unsafe impl Send for ChatLlamaState {}
#[cfg(feature = "chat-service")]
unsafe impl Sync for ChatLlamaState {}

/// Find the model token id whose textual piece is exactly `[TOOL_CALLS]`.
///
/// Ministral 2512 emits this as a control token (typically id 9). At inference
/// time we need the id so we can re-inject the sentinel text into the streaming
/// parser even though `token_to_piece(..., special=false)` would strip it.
/// Gemma 4 does not have such a control token — it streams the literal
/// characters — and this returns `None` for that case.
///
/// Resolved once per model load; called from `ChatLlamaState::new`.
#[cfg(feature = "chat-service")]
fn detect_tool_calls_token(model: &LlamaModel) -> Option<llama_cpp_2::token::LlamaToken> {
    let mut decoder = encoding_rs::UTF_8.new_decoder();
    // The token is typically at a low ID in Mistral-family vocabularies, but
    // scan the full vocab so we are not coupled to that assumption.
    for id in 0..model.n_vocab() {
        let token = llama_cpp_2::token::LlamaToken(id);
        if let Ok(text) = model.token_to_piece(token, &mut decoder, true, None) {
            if text.contains("[TOOL_CALLS]") {
                return Some(token);
            }
        }
    }
    None
}

impl ChatEngine {
    /// Create a new chat engine with the given configuration.
    pub fn new(config: ChatConfig) -> Result<Self> {
        config.validate().map_err(ChatError::ConfigError)?;

        Ok(Self {
            config,
            #[cfg(feature = "chat-service")]
            state: Arc::new(Mutex::new(None)),
            #[cfg(feature = "chat-service")]
            inference_lock: tokio::sync::Mutex::new(()),
        })
    }

    /// Load a GGUF chat model from the given path.
    ///
    /// The model file must exist and be a valid GGUF file with an embedded
    /// chat template. GPU layers are offloaded according to `ChatConfig`.
    pub fn load_model(&self, model_path: &str) -> Result<()> {
        #[cfg(feature = "chat-service")]
        {
            tracing::info!("Loading chat model: {}", model_path);

            let path = std::path::Path::new(model_path);
            if !path.exists() {
                return Err(ChatError::ModelLoadError(format!(
                    "Model file not found: {}",
                    model_path
                )));
            }

            // Get global backend (shares with embedding service)
            let backend = backend()?;

            let model_params =
                LlamaModelParams::default().with_n_gpu_layers(self.config.n_gpu_layers);

            let model = LlamaModel::load_from_file(&backend, path, &model_params)
                .map_err(|e| ChatError::ModelLoadError(format!("Failed to load model: {}", e)))?;

            tracing::info!(
                "Chat model loaded: vocab_size={}, n_ctx_train={}",
                model.n_vocab(),
                model.n_ctx_train(),
            );

            let state = ChatLlamaState::new(
                model,
                model_path.to_string(),
                self.config.n_ctx,
                self.config.n_threads,
            );

            {
                let mut guard = self.state.lock().unwrap_or_else(|p| p.into_inner());
                *guard = Some(state);
            }

            register_atexit_handler();

            tracing::info!("Chat model ready for inference");
        }

        #[cfg(not(feature = "chat-service"))]
        {
            let _ = model_path;
            tracing::info!("STUB: Chat model load (feature disabled)");
        }

        Ok(())
    }

    /// Run streaming inference on a conversation.
    ///
    /// Applies the model's built-in chat template, generates tokens one by one,
    /// and invokes `on_chunk` for each token. Tool calls are detected by the
    /// streaming parser and emitted as `ChatChunk::ToolCallStart` /
    /// `ChatChunk::ToolCallArgs` events.
    ///
    /// Returns usage statistics when generation completes.
    ///
    /// # GPU Scheduling
    ///
    /// This method acquires a tokio Mutex to ensure only one generation runs
    /// at a time. Concurrent callers will wait.
    pub async fn generate_streaming(
        &self,
        messages: Vec<ChatMessageInput>,
        tools: Option<Vec<ToolSpec>>,
        temperature: f32,
        max_tokens: u32,
        on_chunk: impl Fn(ChatChunk) + Send + 'static,
    ) -> Result<ChatUsage> {
        #[cfg(feature = "chat-service")]
        {
            // Serialize inference requests
            let _lock = self.inference_lock.lock().await;

            // Run the blocking llama.cpp inference on a blocking thread
            let state = Arc::clone(&self.state);
            let config_n_ctx = self.config.n_ctx;

            tokio::task::spawn_blocking(move || {
                Self::generate_blocking(
                    &state,
                    messages,
                    tools,
                    temperature,
                    max_tokens,
                    config_n_ctx,
                    &on_chunk,
                )
            })
            .await
            .map_err(|e| ChatError::InferenceError(format!("Task join error: {}", e)))?
        }

        #[cfg(not(feature = "chat-service"))]
        {
            let _ = (messages, tools, temperature, max_tokens);
            on_chunk(ChatChunk::Token("STUB: chat disabled".to_string()));
            on_chunk(ChatChunk::Done);
            Ok(ChatUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
            })
        }
    }

    /// Blocking inference implementation (runs on a blocking thread).
    #[cfg(feature = "chat-service")]
    fn generate_blocking(
        state: &Arc<Mutex<Option<ChatLlamaState>>>,
        messages: Vec<ChatMessageInput>,
        tools: Option<Vec<ToolSpec>>,
        temperature: f32,
        max_tokens: u32,
        config_n_ctx: u32,
        on_chunk: &(impl Fn(ChatChunk) + Send),
    ) -> Result<ChatUsage> {
        let mut state_guard = state.lock().unwrap_or_else(|p| p.into_inner());
        let llama = state_guard.as_mut().ok_or(ChatError::ModelNotLoaded)?;

        // --- Apply chat template ---
        let prompt = Self::apply_chat_template(&llama.model, &messages, &tools)?;
        tracing::debug!(
            "Chat prompt ({} chars): {:?}",
            prompt.len(),
            &prompt[..prompt.len().min(200)]
        );

        // --- Tokenize ---
        // AddBos::Never -- the OAI-compat Jinja template above already injects
        // BOS where appropriate, and adding it again here would double-BOS.
        let tokens = llama
            .model
            .str_to_token(&prompt, AddBos::Never)
            .map_err(|e| ChatError::TokenizationError(e.to_string()))?;

        let prompt_tokens = tokens.len() as u32;

        if prompt_tokens >= config_n_ctx {
            return Err(ChatError::ContextOverflow(format!(
                "Prompt uses {} tokens but context window is {}",
                prompt_tokens, config_n_ctx
            )));
        }

        tracing::debug!("Prompt tokenized: {} tokens", prompt_tokens);

        // --- Extract model info before taking mutable borrow for context ---
        let eos_token = llama.model.token_eos();

        // --- Prepare context and batch ---
        let ctx = llama.get_or_create_context()?;
        ctx.clear_kv_cache();

        let mut batch = llama_cpp_2::llama_batch::LlamaBatch::new(config_n_ctx as usize, 1);
        let last_idx = tokens.len() - 1;
        for (i, &token) in tokens.iter().enumerate() {
            let logits = i == last_idx; // Only need logits for the last token
            batch
                .add(token, i as i32, &[0], logits)
                .map_err(|e| ChatError::InferenceError(format!("Batch add failed: {}", e)))?;
        }

        // Decode the prompt
        ctx.decode(&mut batch)
            .map_err(|e| ChatError::InferenceError(format!("Prompt decode failed: {}", e)))?;

        // --- Sampling setup ---
        let mut sampler = LlamaSampler::chain_simple([
            LlamaSampler::temp(temperature),
            LlamaSampler::dist(0), // seed=0 for deterministic given temperature
        ]);

        // --- Token generation loop ---
        // Reborrow model and context separately to satisfy the borrow checker.
        // get_or_create_context() ensured the context exists, so we can safely
        // split the struct fields.
        let model_ref = &llama.model;
        let tool_calls_token_id = llama.tool_calls_token_id;
        let ctx = llama.context.as_mut().expect("context was just created");

        let mut streaming_parser = StreamingToolCallParser::new();
        let mut piece_decoder = encoding_rs::UTF_8.new_decoder();
        let mut completion_tokens: u32 = 0;
        let mut n_cur = tokens.len();

        loop {
            if completion_tokens >= max_tokens {
                tracing::debug!("Max tokens reached ({})", max_tokens);
                break;
            }

            if n_cur as u32 >= config_n_ctx {
                on_chunk(ChatChunk::Error("Context window full".to_string()));
                break;
            }

            // Sample next token
            let new_token = sampler.sample(ctx, batch.n_tokens() - 1);
            sampler.accept(new_token);

            // Check for end of sequence
            if new_token == eos_token {
                tracing::debug!("EOS token after {} completion tokens", completion_tokens);
                break;
            }

            completion_tokens += 1;

            // If this is the [TOOL_CALLS] control token, inject the sentinel
            // text so the streaming parser can detect tool call mode.
            if tool_calls_token_id == Some(new_token) {
                let event = streaming_parser.feed("[TOOL_CALLS]");
                match event {
                    parser::StreamEvent::Buffering => {}
                    parser::StreamEvent::TextToken(text) => on_chunk(ChatChunk::Token(text)),
                    _ => {}
                }
                // Prepare batch for next token
                batch.clear();
                batch
                    .add(new_token, n_cur as i32, &[0], true)
                    .map_err(|e| ChatError::InferenceError(format!("Batch add failed: {}", e)))?;
                ctx.decode(&mut batch)
                    .map_err(|e| ChatError::InferenceError(format!("Decode failed: {}", e)))?;
                n_cur += 1;
                continue;
            }

            // Convert token to text
            let piece = match model_ref.token_to_piece(new_token, &mut piece_decoder, false, None) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("Failed to decode token {}: {}", new_token.0, e);
                    // Still need to prepare batch for next token even on decode failure
                    batch.clear();
                    batch
                        .add(new_token, n_cur as i32, &[0], true)
                        .map_err(|e| {
                            ChatError::InferenceError(format!("Batch add failed: {}", e))
                        })?;
                    ctx.decode(&mut batch)
                        .map_err(|e| ChatError::InferenceError(format!("Decode failed: {}", e)))?;
                    n_cur += 1;
                    continue;
                }
            };

            // Feed into streaming parser
            let event = streaming_parser.feed(&piece);
            match event {
                parser::StreamEvent::TextToken(text) => {
                    on_chunk(ChatChunk::Token(text));
                }
                parser::StreamEvent::Buffering => {
                    // Parser is accumulating potential tool-call sentinel
                }
                parser::StreamEvent::ToolCall(tc) => {
                    let id = format!("tc_{}", uuid_v4_simple());
                    on_chunk(ChatChunk::ToolCallStart {
                        id: id.clone(),
                        name: tc.name.clone(),
                    });
                    on_chunk(ChatChunk::ToolCallArgs {
                        id,
                        json: tc.args.to_string(),
                    });
                }
                parser::StreamEvent::Finished(_) => break,
            }

            // Prepare batch for next token
            batch.clear();
            batch
                .add(new_token, n_cur as i32, &[0], true)
                .map_err(|e| ChatError::InferenceError(format!("Batch add failed: {}", e)))?;

            ctx.decode(&mut batch)
                .map_err(|e| ChatError::InferenceError(format!("Decode failed: {}", e)))?;

            n_cur += 1;
        }

        // Finalize streaming parser to extract any remaining tool calls
        let parse_result = streaming_parser.finish();
        match parse_result {
            ParseResult::ToolCalls(calls) => {
                for tc in calls {
                    let id = format!("tc_{}", uuid_v4_simple());
                    on_chunk(ChatChunk::ToolCallStart {
                        id: id.clone(),
                        name: tc.name.clone(),
                    });
                    on_chunk(ChatChunk::ToolCallArgs {
                        id,
                        json: tc.args.to_string(),
                    });
                }
            }
            ParseResult::PlainText(_) => {
                // All text was already emitted via TextToken events
            }
            ParseResult::Error(msg) => {
                tracing::warn!("Tool-call parse error at end of stream: {}", msg);
                on_chunk(ChatChunk::Error(format!("Tool-call parse error: {}", msg)));
            }
        }

        on_chunk(ChatChunk::Done);

        let usage = ChatUsage {
            prompt_tokens,
            completion_tokens,
        };

        tracing::info!(
            "Generation complete: {} prompt + {} completion tokens",
            prompt_tokens,
            completion_tokens
        );

        Ok(usage)
    }

    /// Apply the model's built-in chat template to the messages.
    ///
    /// Routes through llama.cpp's OAI-compat Jinja machinery (`common_chat_*`),
    /// which handles family-specific prompt and tool formatting natively for
    /// Mistral, Gemma 4, and any other model with an embedded Jinja template.
    /// The simple `apply_chat_template` C API does not work for Gemma 4 — its
    /// chat template requires the full Jinja engine plus llama.cpp's chat
    /// specialization layer.
    #[cfg(feature = "chat-service")]
    fn apply_chat_template(
        model: &LlamaModel,
        messages: &[ChatMessageInput],
        tools: &Option<Vec<ToolSpec>>,
    ) -> Result<String> {
        // Build OpenAI-format messages JSON. Tool-result messages carry
        // `tool_call_id`; the Jinja template handles family-specific wrapping
        // (Mistral [TOOL_RESULTS], Gemma 4 turn format, etc.).
        let messages_value: Vec<serde_json::Value> = messages
            .iter()
            .map(|msg| {
                if msg.role == "tool" {
                    serde_json::json!({
                        "role": "tool",
                        "tool_call_id": msg.call_id.as_deref().unwrap_or("unknown"),
                        "content": msg.content,
                    })
                } else {
                    serde_json::json!({
                        "role": msg.role,
                        "content": msg.content,
                    })
                }
            })
            .collect();

        let messages_json = serde_json::to_string(&messages_value)
            .map_err(|e| ChatError::TemplateError(format!("Message JSON error: {}", e)))?;

        // Build OpenAI tool-spec JSON if tools are provided. The Jinja template
        // formats these per-family (Ministral [AVAILABLE_TOOLS], Gemma 4 <tools>).
        let tools_json_string = if let Some(tool_specs) = tools.as_ref() {
            if tool_specs.is_empty() {
                None
            } else {
                let tools_value: Vec<serde_json::Value> = tool_specs
                    .iter()
                    .map(|t| {
                        serde_json::json!({
                            "type": "function",
                            "function": {
                                "name": t.name,
                                "description": t.description,
                                "parameters": t.parameters_schema,
                            }
                        })
                    })
                    .collect();
                Some(
                    serde_json::to_string(&tools_value)
                        .map_err(|e| ChatError::TemplateError(format!("Tool JSON error: {}", e)))?,
                )
            }
        } else {
            None
        };

        // Retrieve the model's embedded chat template
        let tmpl = model
            .chat_template(None)
            .map_err(|e| ChatError::TemplateError(format!("No chat template in model: {}", e)))?;

        let params = OpenAIChatTemplateParams {
            messages_json: &messages_json,
            tools_json: tools_json_string.as_deref(),
            tool_choice: None,
            json_schema: None,
            grammar: None,
            reasoning_format: None,
            chat_template_kwargs: None,
            add_generation_prompt: true,
            use_jinja: true,
            parallel_tool_calls: false,
            enable_thinking: false,
            // The Jinja template injects BOS where appropriate; AddBos::Never
            // at tokenization time avoids double-BOS.
            add_bos: false,
            add_eos: false,
            // We want raw tokens out so our streaming parser can detect
            // [TOOL_CALLS] sentinels itself.
            parse_tool_calls: false,
        };

        let result = model
            .apply_chat_template_oaicompat(&tmpl, &params)
            .map_err(|e| {
                ChatError::TemplateError(format!("Failed to apply chat template: {}", e))
            })?;

        Ok(result.prompt)
    }

    /// Count the number of tokens in the given text.
    pub fn token_count(&self, text: &str) -> Result<u32> {
        #[cfg(feature = "chat-service")]
        {
            let state_guard = self.state.lock().unwrap_or_else(|p| p.into_inner());
            let llama = state_guard.as_ref().ok_or(ChatError::ModelNotLoaded)?;

            let tokens = llama
                .model
                .str_to_token(text, AddBos::Never)
                .map_err(|e| ChatError::TokenizationError(e.to_string()))?;

            Ok(tokens.len() as u32)
        }

        #[cfg(not(feature = "chat-service"))]
        {
            // Rough estimate: ~4 chars per token (common for English)
            Ok((text.len() as f32 / 4.0).ceil() as u32)
        }
    }

    /// Return information about the currently loaded model.
    ///
    /// Returns `None` if no model is loaded.
    pub fn model_info(&self) -> Option<LoadedModelInfo> {
        #[cfg(feature = "chat-service")]
        {
            let state_guard = self.state.lock().unwrap_or_else(|p| p.into_inner());
            state_guard.as_ref().map(|s| LoadedModelInfo {
                model_path: s.model_path.clone(),
                context_size: s.context_size,
            })
        }

        #[cfg(not(feature = "chat-service"))]
        {
            None
        }
    }

    /// Release GPU resources held by the chat model.
    ///
    /// After calling this, `generate_streaming` will return `ModelNotLoaded`.
    /// The model can be reloaded with `load_model`.
    pub fn unload_model(&self) {
        #[cfg(feature = "chat-service")]
        {
            let mut state_guard = self.state.lock().unwrap_or_else(|p| p.into_inner());
            if state_guard.take().is_some() {
                tracing::info!("Chat model unloaded, GPU resources released");
            }
        }
    }

    /// Check if a model is currently loaded.
    pub fn is_loaded(&self) -> bool {
        #[cfg(feature = "chat-service")]
        {
            let state_guard = self.state.lock().unwrap_or_else(|p| p.into_inner());
            state_guard.is_some()
        }

        #[cfg(not(feature = "chat-service"))]
        {
            false
        }
    }
}

/// Generate a simple UUID-like string for tool call IDs.
/// Not cryptographically random — just unique enough for local use.
#[cfg(feature = "chat-service")]
fn uuid_v4_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:x}", nanos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_engine_creation() {
        let config = ChatConfig::default();
        let engine = ChatEngine::new(config);
        assert!(engine.is_ok());
    }

    #[test]
    fn test_chat_engine_not_loaded() {
        let engine = ChatEngine::new(ChatConfig::default()).unwrap();
        assert!(!engine.is_loaded());
        assert!(engine.model_info().is_none());
    }

    #[test]
    fn test_chat_engine_token_count_stub() {
        let engine = ChatEngine::new(ChatConfig::default()).unwrap();
        // Without the chat-service feature, this uses the rough estimator
        #[cfg(not(feature = "chat-service"))]
        {
            let count = engine.token_count("Hello world").unwrap();
            assert!(count > 0);
        }
        // With the feature, it should fail because no model is loaded
        #[cfg(feature = "chat-service")]
        {
            let result = engine.token_count("Hello world");
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_chat_config_validation_error() {
        let config = ChatConfig {
            n_ctx: 0,
            ..Default::default()
        };
        let result = ChatEngine::new(config);
        assert!(result.is_err());
    }
}
