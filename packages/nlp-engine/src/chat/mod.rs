/// Chat inference engine using llama.cpp.
///
/// Provides streaming text generation from GGUF chat models with tool-call
/// parsing via llama.cpp's native `ChatParseStateOaicompat` streaming parser.
/// This handles all model families (Mistral, Gemma 4, etc.) natively at the
/// C++ level without custom sentinel detection. Designed to coexist with the
/// embedding service on the same GPU (validated in PoC with shared Metal backend).
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
pub use types::{
    ChatChunk, ChatConfig, ChatMessage, ChatUsage, LoadedModelInfo, Role, ToolCallRaw, ToolSpec,
};

#[cfg(feature = "chat-service")]
use crate::embedding::{get_or_init_backend, register_atexit_handler};

#[cfg(feature = "chat-service")]
use llama_cpp_2::context::params::{KvCacheType, LlamaContextParams};
#[cfg(feature = "chat-service")]
use llama_cpp_2::context::LlamaContext;
#[cfg(feature = "chat-service")]
use llama_cpp_2::model::params::LlamaModelParams;
#[cfg(feature = "chat-service")]
use llama_cpp_2::model::ChatTemplateResult;
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
    type_k: Option<crate::chat::types::KvCacheQuantType>,
    type_v: Option<crate::chat::types::KvCacheQuantType>,
    /// Tokens from the last decoded prompt, used to find the reusable prefix.
    cached_prompt: Vec<llama_cpp_2::token::LlamaToken>,
}

#[cfg(feature = "chat-service")]
impl ChatLlamaState {
    fn new(
        model: LlamaModel,
        model_path: String,
        context_size: u32,
        n_threads: i32,
        type_k: Option<crate::chat::types::KvCacheQuantType>,
        type_v: Option<crate::chat::types::KvCacheQuantType>,
    ) -> Self {
        Self {
            model,
            context: None,
            model_path,
            context_size,
            n_threads,
            type_k,
            type_v,
            cached_prompt: Vec::new(),
        }
    }

    /// Get or create the generation context.
    ///
    /// Unlike the embedding context, the chat context does NOT use embeddings
    /// mode and has a fixed batch size matching the context window.
    fn get_or_create_context(&mut self) -> Result<&mut LlamaContext<'static>> {
        if self.context.is_none() {
            tracing::info!(
                "Creating chat LlamaContext (n_ctx={}, n_threads={}, type_k={:?}, type_v={:?})",
                self.context_size,
                self.n_threads,
                self.type_k,
                self.type_v,
            );

            let mut ctx_params = LlamaContextParams::default()
                .with_n_ctx(std::num::NonZeroU32::new(self.context_size))
                .with_n_batch(self.context_size)
                .with_n_threads(self.n_threads)
                .with_n_threads_batch(self.n_threads);

            if let Some(k) = self.type_k {
                ctx_params = ctx_params.with_type_k(kv_quant_to_llama(k));
            }
            if let Some(v) = self.type_v {
                ctx_params = ctx_params.with_type_v(kv_quant_to_llama(v));
            }

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
                self.config.type_k,
                self.config.type_v,
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
        messages: Vec<ChatMessage>,
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
        messages: Vec<ChatMessage>,
        tools: Option<Vec<ToolSpec>>,
        temperature: f32,
        max_tokens: u32,
        config_n_ctx: u32,
        on_chunk: &(impl Fn(ChatChunk) + Send),
    ) -> Result<ChatUsage> {
        let mut state_guard = state.lock().unwrap_or_else(|p| p.into_inner());
        let llama = state_guard.as_mut().ok_or(ChatError::ModelNotLoaded)?;

        // --- Apply chat template ---
        let tmpl_result = Self::apply_chat_template(&llama.model, &messages, &tools)?;
        tracing::debug!(
            "Chat template applied: chat_format={} parse_tool_calls={} additional_stops={:?}",
            tmpl_result.chat_format,
            tmpl_result.parse_tool_calls,
            tmpl_result.additional_stops,
        );
        // chat_format=0 (CONTENT_ONLY) means llama.cpp failed to detect the model's
        // specialized format (e.g. COMMON_CHAT_FORMAT_PEG_GEMMA4=3). When tools are
        // provided this is a routing failure — tool calls will be emitted as plain text
        // and never parsed. Warn so the failure is visible without a debug build.
        if let Some(active_tools) = tools.as_ref().filter(|t| !t.is_empty()) {
            if tmpl_result.chat_format == 0 {
                tracing::warn!(
                    "chat_format=0 (CONTENT_ONLY) with {} tools — specialized template \
                     detection may have failed; tool calls will not be parsed",
                    active_tools.len(),
                );
            }
        }
        let prompt = &tmpl_result.prompt;
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
            .str_to_token(prompt, AddBos::Never)
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
        // Additional stop sequences from the chat template (e.g. Gemma 4's "<end_of_turn>").
        let additional_stops = tmpl_result.additional_stops.clone();

        // --- Prepare context and batch ---
        // Find the longest token prefix shared with the last decoded prompt.
        // Any matching prefix is already in the KV cache — only the delta needs
        // decoding, which eliminates redundant attention computation for the
        // stable system prompt on every ReAct iteration.
        //
        // Compute prefix metrics before the context borrow so the borrow
        // checker sees no overlap between `llama.cached_prompt` reads and the
        // `&mut llama` taken by `get_or_create_context`.
        let prefix_len = tokens
            .iter()
            .zip(llama.cached_prompt.iter())
            .take_while(|(a, b)| a == b)
            .count();
        let has_reusable_prefix = prefix_len > 0 && prefix_len < tokens.len();

        let ctx = llama.get_or_create_context()?;

        // Determine how many tokens to skip in the batch.  If the KV trim
        // succeeds we decode only the delta; if it fails we fall back to a
        // full decode so the batch and KV cache are never out of sync.
        let decode_from = if has_reusable_prefix {
            tracing::debug!(
                "KV cache reuse: {} prefix tokens cached, decoding {} delta tokens",
                prefix_len,
                tokens.len() - prefix_len
            );
            match ctx.clear_kv_cache_seq(Some(0), Some(prefix_len as u32), None) {
                Ok(true) => prefix_len,
                // `false` means the backend (e.g. recurrent models) does not
                // support partial removal — fall back to full decode so the
                // KV cache and batch are never out of sync.
                Ok(false) => {
                    tracing::warn!(
                        "KV cache partial trim returned false, falling back to full decode"
                    );
                    ctx.clear_kv_cache();
                    0
                }
                Err(e) => {
                    tracing::warn!("KV cache trim failed ({}), falling back to full decode", e);
                    ctx.clear_kv_cache();
                    0
                }
            }
        } else {
            // No reusable prefix (first call, or prompt changed entirely): full decode.
            ctx.clear_kv_cache();
            0
        };

        let mut batch = llama_cpp_2::llama_batch::LlamaBatch::new(config_n_ctx as usize, 1);
        let last_idx = tokens.len() - 1;
        for (i, &token) in tokens[decode_from..].iter().enumerate() {
            let pos = (decode_from + i) as i32;
            let logits = decode_from + i == last_idx;
            batch
                .add(token, pos, &[0], logits)
                .map_err(|e| ChatError::InferenceError(format!("Batch add failed: {}", e)))?;
        }

        // Decode the prompt (or just the delta on subsequent calls)
        ctx.decode(&mut batch)
            .map_err(|e| ChatError::InferenceError(format!("Prompt decode failed: {}", e)))?;

        // --- Sampling setup ---
        // A repetition penalty runs first so it reshapes the logits before the
        // temperature softens them. Without it, small models at near-greedy
        // temperature (0.1) fall into degenerate loops — repeating a closing
        // phrase like "I'm ready to help you. What would you like to do next?"
        // until they exhaust max_tokens (a ~100s hang that never emits the
        // <end_of_turn> stop token). penalty_last_n=256 covers the recent
        // window; repeat=1.3 is firm enough to escape a loop without distorting
        // normal prose. Frequency/presence penalties stay disabled (0.0).
        let mut sampler = LlamaSampler::chain_simple([
            LlamaSampler::penalties(256, 1.3, 0.0, 0.0),
            LlamaSampler::temp(temperature),
            LlamaSampler::dist(0), // seed=0 for deterministic given temperature
        ]);

        // --- Initialize llama.cpp's native streaming tool-call parser ---
        // ChatParseStateOaicompat handles all model families (Mistral, Gemma 4,
        // etc.) natively at the C++ level — no custom sentinel detection needed.
        let mut oai_parser = tmpl_result.streaming_state_oaicompat().map_err(|e| {
            ChatError::InferenceError(format!("Failed to init streaming parser: {}", e))
        })?;

        // Track tool call ids by index so ToolCallStart/Args pair correctly.
        let mut tool_call_ids: std::collections::HashMap<u32, String> =
            std::collections::HashMap::new();


        // Route the model's `<|channel> … <channel|>` reasoning out of the answer
        // stream. Held across the whole loop so a span (or marker) that straddles
        // a delta boundary is resolved correctly.
        let mut channel_splitter = ChannelSplitter::default();

        // --- Token generation loop ---
        // Reborrow model and context separately to satisfy the borrow checker.
        // get_or_create_context() ensured the context exists, so we can safely
        // split the struct fields.
        let model_ref = &llama.model;
        let ctx = llama.context.as_mut().expect("context was just created");

        let mut piece_decoder = encoding_rs::UTF_8.new_decoder();
        let mut completion_tokens: u32 = 0;
        let mut n_cur = tokens.len();
        let mut context_overflowed = false;

        loop {
            if completion_tokens >= max_tokens {
                tracing::debug!("Max tokens reached ({})", max_tokens);
                break;
            }

            if n_cur as u32 >= config_n_ctx {
                on_chunk(ChatChunk::Error("Context window full".to_string()));
                context_overflowed = true;
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

            // Convert token to text (special=true so all tokens decode cleanly)
            let piece = match model_ref.token_to_piece(new_token, &mut piece_decoder, true, None) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("Failed to decode token {}: {}", new_token.0, e);
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

            // Stop on template-defined stop sequences (e.g. Gemma 4's "<end_of_turn>").
            if additional_stops.iter().any(|s| piece.contains(s.as_str())) {
                tracing::debug!("Stop sequence detected in piece: {:?}", piece);
                break;
            }

            // Feed each token piece to the OAI-compat parser incrementally.
            // update() takes only the new text added since the previous call.
            match oai_parser.update(&piece, true) {
                Ok(deltas) => {
                    for delta_json in deltas {
                        emit_oai_delta(
                            &delta_json,
                            &mut tool_call_ids,
                            &mut channel_splitter,
                            on_chunk,
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!("OAI parser update error: {}", e);
                }
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

        // Store the full prompt token sequence so the next call can reuse the
        // KV-cached prefix.  On context overflow the KV state is indeterminate,
        // so we clear cached_prompt to force a full decode on the next call.
        if context_overflowed {
            llama.cached_prompt.clear();
        } else {
            llama.cached_prompt = tokens;
        }

        // Finalize: signal end-of-stream with empty string and is_partial=false.
        match oai_parser.update("", false) {
            Ok(deltas) => {
                for delta_json in deltas {
                    emit_oai_delta(
                        &delta_json,
                        &mut tool_call_ids,
                        &mut channel_splitter,
                        on_chunk,
                    );
                }
            }
            Err(e) => tracing::warn!("OAI parser finalize error: {}", e),
        }

        // Drain any text the splitter held back (a trailing partial marker that
        // never completed, or an unclosed channel truncated by the token cap).
        let (final_answer, final_reasoning) = channel_splitter.finalize();
        if !final_reasoning.is_empty() {
            on_chunk(ChatChunk::Reasoning(final_reasoning));
        }
        let final_cleaned = strip_gemma_special_tokens(&final_answer);
        if !final_cleaned.is_empty() {
            on_chunk(ChatChunk::Token(final_cleaned));
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
}

/// Channel markers Gemma 4 uses to wrap its internal chain-of-thought.
const CHANNEL_OPEN: &str = "<|channel>";
const CHANNEL_CLOSE: &str = "<channel|>";

/// Splits a streamed content sequence into answer text and reasoning text.
///
/// Gemma 4 wraps its internal chain-of-thought in `<|channel> … <channel|>`
/// markers. llama.cpp's OAI-compat parser does not separate these — it passes
/// the whole span through as plain content — so we route it ourselves: text
/// between the markers is *reasoning* (surfaced in a dedicated collapsible UI
/// section), text outside is the *answer* (the clean message bubble).
///
/// The splitter is stateful because a single logical span — and even a single
/// marker — may be split across multiple streaming deltas. It tracks whether
/// it is currently inside a channel and carries a tail that could be the
/// beginning of a marker straddling a delta boundary.
#[derive(Debug, Default)]
struct ChannelSplitter {
    /// True while between an opener and its (not-yet-seen) closer.
    in_channel: bool,
    /// Tail of the last delta that may be the prefix of a marker spanning the
    /// boundary into the next delta. Always marker-free text otherwise.
    carry: String,
}

impl ChannelSplitter {
    /// Feed a content delta, returning `(answer, reasoning)` text resolved so far.
    ///
    /// Any trailing bytes that could still grow into a marker are held back in
    /// `carry` and resolved on a later `push` or on `finalize`.
    fn push(&mut self, content: &str) -> (String, String) {
        let mut answer = String::new();
        let mut reasoning = String::new();

        let mut buf = std::mem::take(&mut self.carry);
        buf.push_str(content);

        let mut rest = buf.as_str();
        loop {
            let marker = if self.in_channel {
                CHANNEL_CLOSE
            } else {
                CHANNEL_OPEN
            };
            match rest.find(marker) {
                Some(idx) => {
                    // Emit the text before the marker to the active sink, then
                    // toggle state and continue scanning past the marker.
                    let before = &rest[..idx];
                    if self.in_channel {
                        reasoning.push_str(before);
                    } else {
                        answer.push_str(before);
                    }
                    self.in_channel = !self.in_channel;
                    rest = &rest[idx + marker.len()..];
                }
                None => {
                    // No complete marker remains. Hold back any suffix that
                    // could be the prefix of the marker we're looking for; emit
                    // the rest to the active sink.
                    let keep = partial_marker_suffix_len(rest, marker);
                    let split = rest.len() - keep;
                    let emit = &rest[..split];
                    if self.in_channel {
                        reasoning.push_str(emit);
                    } else {
                        answer.push_str(emit);
                    }
                    self.carry = rest[split..].to_string();
                    break;
                }
            }
        }

        (answer, reasoning)
    }

    /// Flush any carried text once generation is complete.
    ///
    /// No further deltas can complete a marker, so the carry is plain text and
    /// is emitted to whichever sink is currently active.
    fn finalize(&mut self) -> (String, String) {
        let carried = std::mem::take(&mut self.carry);
        if self.in_channel {
            (String::new(), carried)
        } else {
            (carried, String::new())
        }
    }
}

/// Length of the longest suffix of `text` that is a (strict) prefix of `marker`.
///
/// Used to decide how much trailing text to hold back across a delta boundary:
/// e.g. if `text` ends in `"<chan"` and `marker` is `"<channel|>"`, the last 5
/// bytes must be carried in case the rest of the marker arrives next. Marker
/// bytes are ASCII, so a match position is always a UTF-8 char boundary.
fn partial_marker_suffix_len(text: &str, marker: &str) -> usize {
    let max = marker.len().min(text.len());
    // Longest first so we hold back the most that could still match.
    (1..=max)
        .rev()
        .find(|&k| {
            marker
                .as_bytes()
                .starts_with(&text.as_bytes()[text.len() - k..])
        })
        .unwrap_or(0)
}

/// Remove Gemma 4 special-token strings from a content string.
///
/// The PEG parser (`COMMON_CHAT_FORMAT_PEG_GEMMA4`) may emit residual special-token
/// text as content before it identifies a complete tool-call boundary. Gemma 4 uses
/// two special-token patterns:
/// - `<|...|>` — symmetric delimiters e.g. `<|"|>` (quote token)
/// - `<|...>` — asymmetric e.g. `<|tool_call>` (call start marker)
/// - `<...|>` — asymmetric e.g. `<tool_call|>` (call end marker)
///
/// We strip all three by removing any substring that starts with `<|` or ends with `|>`.
fn strip_gemma_special_tokens(s: &str) -> String {
    // Known Gemma 4 special token strings to strip.
    const SPECIAL: &[&str] = &[
        "<|tool_call>",
        "<tool_call|>",
        "<|tool_response>",
        "<|/tool_response>",
        "<|\"|>",
        "<turn|>",
        "<|turn>",
        "<end_of_turn>",
        "<start_of_turn>",
        "<|channel>",
        "<channel|>",
    ];
    let mut out = s.to_string();
    for token in SPECIAL {
        out = out.replace(token, "");
    }
    out
}

/// Convert one `ChatMessage` into an OpenAI-format message value for the
/// Jinja chat template.
///
/// Pure and model-independent so it can be unit-tested without a loaded model.
/// Two shapings here are load-bearing for the Gemma 4 template — emitting the
/// wrong shape makes `apply_chat_template` fail with llama.cpp `ffi error -3`,
/// aborting the whole turn:
/// - A tool-call assistant turn carries `content: null` (OpenAI convention)
///   when it has no text, never `""`.
/// - Each tool call's `arguments` must be a valid JSON *object* string; any
///   non-object (empty, malformed) is normalized to `"{}"`.
fn chat_message_to_oai_value(msg: &ChatMessage) -> serde_json::Value {
    if msg.role == Role::Tool {
        serde_json::json!({
            "role": "tool",
            "tool_call_id": msg.tool_call_id.as_deref().unwrap_or("unknown"),
            "content": msg.content,
        })
    } else if msg.role == Role::Assistant && !msg.tool_calls.is_empty() {
        let tool_calls: Vec<serde_json::Value> = msg
            .tool_calls
            .iter()
            .map(|tc| {
                let arguments = match serde_json::from_str::<serde_json::Value>(&tc.arguments_json)
                {
                    Ok(v) if v.is_object() => tc.arguments_json.clone(),
                    _ => "{}".to_string(),
                };
                serde_json::json!({
                    "id": tc.id,
                    "type": "function",
                    "function": { "name": tc.function_name, "arguments": arguments }
                })
            })
            .collect();
        let content: serde_json::Value = if msg.content.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::Value::String(msg.content.clone())
        };
        serde_json::json!({
            "role": "assistant",
            "content": content,
            "tool_calls": tool_calls,
        })
    } else {
        serde_json::json!({
            "role": msg.role.as_str(),
            "content": msg.content,
        })
    }
}

/// Parse an OpenAI-compat streaming delta JSON string and emit the appropriate
/// `ChatChunk` events. Called once per delta returned by `ChatParseStateOaicompat::update`.
///
/// Delta shape (subset we care about):
/// - plain text: `{"role":"assistant","content":"hello"}`
/// - tool call:  `{"role":"assistant","tool_calls":[{"index":0,"id":"...","type":"function",
///                "function":{"name":"search_nodes","arguments":"{\"q\":1}"}}]}`
#[cfg(feature = "chat-service")]
fn emit_oai_delta(
    delta_json: &str,
    tool_call_ids: &mut std::collections::HashMap<u32, String>,
    splitter: &mut ChannelSplitter,
    on_chunk: &(impl Fn(ChatChunk) + Send),
) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(delta_json) else {
        tracing::warn!("OAI delta is not valid JSON: {:?}", delta_json);
        return;
    };

    // Plain text content. Route `<|channel> … <channel|>` spans to a separate
    // reasoning stream (the splitter is stateful across deltas; a span or even a
    // single marker may straddle a delta boundary). The answer part still passes
    // through `strip_gemma_special_tokens` so other residual special tokens (e.g.
    // "<|tool_call>", "<|"|>") the PEG parser leaks before recognising a tool-call
    // boundary are scrubbed.
    if let Some(content) = v.get("content").and_then(|c| c.as_str()) {
        let (answer, reasoning) = splitter.push(content);
        if !reasoning.is_empty() {
            on_chunk(ChatChunk::Reasoning(reasoning));
        }
        let cleaned = strip_gemma_special_tokens(&answer);
        if !cleaned.is_empty() {
            on_chunk(ChatChunk::Token(cleaned));
        }
    }

    // Tool calls array
    if let Some(tool_calls) = v.get("tool_calls").and_then(|tc| tc.as_array()) {
        for tc in tool_calls {
            let index = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as u32;
            let function = tc.get("function");

            let name = function
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("");
            // Only present when the delta actually carries arguments — absent on
            // name-only deltas (common in Mistral streaming where args arrive later).
            let args: Option<&str> = function
                .and_then(|f| f.get("arguments"))
                .and_then(|a| a.as_str());

            // First delta for this index includes the name → emit ToolCallStart
            if !name.is_empty() && !tool_call_ids.contains_key(&index) {
                let id = tc
                    .get("id")
                    .and_then(|i| i.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("tc_{}", uuid_v4_simple()));
                tool_call_ids.insert(index, id.clone());
                on_chunk(ChatChunk::ToolCallStart {
                    id: id.clone(),
                    name: name.to_string(),
                });
                if let Some(a) = args.filter(|a| !a.is_empty()) {
                    on_chunk(ChatChunk::ToolCallArgs {
                        id,
                        json: a.to_string(),
                    });
                }
            } else if let Some(id) = tool_call_ids.get(&index) {
                // Subsequent arg deltas for the same call
                if let Some(a) = args.filter(|a| !a.is_empty()) {
                    on_chunk(ChatChunk::ToolCallArgs {
                        id: id.clone(),
                        json: a.to_string(),
                    });
                }
            }
        }
    }
}

impl ChatEngine {
    /// Apply the model's built-in chat template to the messages.
    ///
    /// Routes through llama.cpp's OAI-compat Jinja machinery (`common_chat_*`),
    /// which handles family-specific prompt and tool formatting natively for
    /// Mistral, Gemma 4, and any other model with an embedded Jinja template.
    /// The simple `apply_chat_template` C API does not work for Gemma 4 — its
    /// chat template requires the full Jinja engine plus llama.cpp's chat
    /// specialization layer.
    ///
    /// Returns the full `ChatTemplateResult` so the caller can initialize
    /// `ChatParseStateOaicompat` for streaming tool-call parsing.
    #[cfg(feature = "chat-service")]
    fn apply_chat_template(
        model: &LlamaModel,
        messages: &[ChatMessage],
        tools: &Option<Vec<ToolSpec>>,
    ) -> Result<ChatTemplateResult> {
        // Build OpenAI-format messages JSON. Tool-result messages carry
        // `tool_call_id`; the Jinja template handles family-specific wrapping
        // (Mistral [TOOL_RESULTS], Gemma 4 turn format, etc.).
        let messages_value: Vec<serde_json::Value> =
            messages.iter().map(chat_message_to_oai_value).collect();

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
            // Enable llama.cpp's native tool-call parsing so ChatTemplateResult
            // carries the chat_format + parser needed for ChatParseStateOaicompat.
            parse_tool_calls: true,
        };

        model
            .apply_chat_template_oaicompat(&tmpl, &params)
            .map_err(|e| ChatError::TemplateError(format!("Failed to apply chat template: {}", e)))
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

/// Convert our `KvCacheQuantType` to the llama-cpp-2 `KvCacheType`.
#[cfg(feature = "chat-service")]
fn kv_quant_to_llama(q: crate::chat::types::KvCacheQuantType) -> KvCacheType {
    match q {
        crate::chat::types::KvCacheQuantType::Q8_0 => KvCacheType::Q8_0,
        crate::chat::types::KvCacheQuantType::Q4_0 => KvCacheType::Q4_0,
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

    // --- ChannelSplitter: route <|channel> … <channel|> reasoning out of answer ---

    /// Drive a splitter over a sequence of deltas, returning the fully
    /// accumulated `(answer, reasoning)` including the finalize flush.
    fn split_all(deltas: &[&str]) -> (String, String) {
        let mut s = ChannelSplitter::default();
        let mut answer = String::new();
        let mut reasoning = String::new();
        for d in deltas {
            let (a, r) = s.push(d);
            answer.push_str(&a);
            reasoning.push_str(&r);
        }
        let (a, r) = s.finalize();
        answer.push_str(&a);
        reasoning.push_str(&r);
        (answer, reasoning)
    }

    #[test]
    fn channel_splitter_marker_within_one_delta() {
        let (answer, reasoning) =
            split_all(&["Done.<|channel>I should add the node.<channel|>Added it!"]);
        assert_eq!(answer, "Done.Added it!");
        assert_eq!(reasoning, "I should add the node.");
    }

    #[test]
    fn channel_splitter_marker_split_across_deltas() {
        // Both the opener and closer are fragmented across delta boundaries.
        let (answer, reasoning) =
            split_all(&["Hi<|cha", "nnel>think", "ing more<chan", "nel|>the answer"]);
        assert_eq!(answer, "Hithe answer");
        assert_eq!(reasoning, "thinking more");
    }

    #[test]
    fn channel_splitter_opener_without_closer_is_all_reasoning() {
        // Truncated by the token cap mid-thought: everything after the opener
        // is reasoning, and no answer text leaks.
        let (answer, reasoning) = split_all(&["Here.<|channel>reasoning that never closes"]);
        assert_eq!(answer, "Here.");
        assert_eq!(reasoning, "reasoning that never closes");
    }

    #[test]
    fn channel_splitter_clean_content_has_no_reasoning() {
        let (answer, reasoning) = split_all(&["Just a normal ", "answer."]);
        assert_eq!(answer, "Just a normal answer.");
        assert_eq!(reasoning, "");
    }

    #[test]
    fn channel_splitter_trailing_partial_marker_flushed_as_answer() {
        // A suffix that looks like the start of a marker but never completes
        // must be flushed to the answer on finalize, not swallowed.
        let (answer, reasoning) = split_all(&["answer<|chan"]);
        assert_eq!(answer, "answer<|chan");
        assert_eq!(reasoning, "");
    }

    #[test]
    fn channel_splitter_multiple_channels_concatenate() {
        let (answer, reasoning) = split_all(&["a<|channel>r1<channel|>b<|channel>r2<channel|>c"]);
        assert_eq!(answer, "abc");
        assert_eq!(reasoning, "r1r2");
    }

    #[test]
    fn channel_splitter_handles_multibyte_answer_text() {
        // Non-ASCII answer text must not panic the byte-based carry logic.
        let (answer, reasoning) = split_all(&["café ☕<|channel>think<channel|> déjà vu"]);
        assert_eq!(answer, "café ☕ déjà vu");
        assert_eq!(reasoning, "think");
    }

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

    #[test]
    fn test_strip_gemma_special_tokens() {
        assert_eq!(strip_gemma_special_tokens("hello"), "hello");
        assert_eq!(strip_gemma_special_tokens("<|tool_call>"), "");
        assert_eq!(strip_gemma_special_tokens("<|\"|>"), "");
        assert_eq!(strip_gemma_special_tokens("<tool_call|>"), "");
        assert_eq!(strip_gemma_special_tokens("<|tool_response>"), "");
        assert_eq!(strip_gemma_special_tokens("<|/tool_response>"), "");
        assert_eq!(
            strip_gemma_special_tokens("hello<|tool_call>world"),
            "helloworld"
        );
        assert_eq!(
            strip_gemma_special_tokens("<|tool_call>call:search<tool_call|>"),
            "call:search"
        );
        // Unknown <|...|>-style tokens are left as-is (only known tokens stripped)
        assert_eq!(
            strip_gemma_special_tokens("text<|unknown|>end"),
            "text<|unknown|>end"
        );
    }

    fn msg(role: Role, content: &str) -> ChatMessage {
        ChatMessage::text(role, content)
    }

    #[test]
    fn oai_value_plain_message_passes_through() {
        let v = chat_message_to_oai_value(&msg(Role::User, "hi"));
        assert_eq!(v["role"], "user");
        assert_eq!(v["content"], "hi");
        assert!(v.get("tool_calls").is_none());
    }

    #[test]
    fn oai_value_tool_call_turn_empty_content_is_null() {
        // The load-bearing shape: empty content on a tool-call turn must emit
        // `null`, not `""` — else Gemma's template fails (ffi error -3).
        let m = ChatMessage::assistant_with_tool_calls(
            "",
            vec![ToolCallRaw {
                id: "tc_1".into(),
                function_name: "search_nodes".into(),
                arguments_json: r#"{"query":"x"}"#.into(),
            }],
        );
        let v = chat_message_to_oai_value(&m);
        assert!(
            v["content"].is_null(),
            "empty content must serialize to null"
        );
        assert_eq!(v["tool_calls"][0]["function"]["name"], "search_nodes");
        assert_eq!(
            v["tool_calls"][0]["function"]["arguments"],
            r#"{"query":"x"}"#
        );
    }

    #[test]
    fn oai_value_tool_call_turn_keeps_real_content() {
        let m = ChatMessage::assistant_with_tool_calls(
            "Let me check.",
            vec![ToolCallRaw {
                id: "tc_1".into(),
                function_name: "get_node".into(),
                arguments_json: r#"{"id":"abc"}"#.into(),
            }],
        );
        let v = chat_message_to_oai_value(&m);
        assert_eq!(v["content"], "Let me check.");
    }

    #[test]
    fn oai_value_normalizes_non_object_arguments_to_empty_object() {
        // Empty / malformed / non-object arguments are normalized to "{}" so the
        // template never receives invalid JSON.
        for bad in ["", "not json", "[1,2]", "\"str\"", "42"] {
            let m = ChatMessage::assistant_with_tool_calls(
                "",
                vec![ToolCallRaw {
                    id: "tc_1".into(),
                    function_name: "create_schema".into(),
                    arguments_json: bad.into(),
                }],
            );
            let v = chat_message_to_oai_value(&m);
            assert_eq!(
                v["tool_calls"][0]["function"]["arguments"], "{}",
                "non-object arguments {bad:?} should normalize to {{}}"
            );
        }
    }

    #[test]
    fn oai_value_tool_result_carries_call_id() {
        let m = ChatMessage::tool_result("result text", "tc_1", "my_tool");
        let v = chat_message_to_oai_value(&m);
        assert_eq!(v["role"], "tool");
        assert_eq!(v["tool_call_id"], "tc_1");
        assert_eq!(v["content"], "result text");
    }
}
