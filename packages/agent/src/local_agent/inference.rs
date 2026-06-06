//! Bridge between `nlp-engine::ChatEngine` and the `ChatInferenceEngine` trait.
//!
//! Adapts the nlp-engine's `ChatEngine` (which speaks its own `ChatChunk` and
//! `ToolSpec` types) to the app-crate's `ChatInferenceEngine` trait (which uses
//! `StreamingChunk` and `ToolDefinition`). `ChatMessage` is the canonical type
//! shared by both crates — no conversion needed on the message path.

use std::sync::Arc;

use async_trait::async_trait;
use nodespace_nlp_engine::chat::{ChatChunk, ChatConfig, ChatEngine, ToolSpec};

use crate::agent_types::{
    ChatInferenceEngine, ChatModelSpec, InferenceError, InferenceRequest, InferenceUsage,
    ModelFamily, StreamingChunk, ToolDefinition,
};

/// Chat inference engine backed by llama.cpp via `nlp-engine::ChatEngine`.
///
/// Thread-safe: the underlying `ChatEngine` serializes inference requests
/// via a tokio Mutex, preventing Metal command-buffer collisions.
pub struct LlamaChatInferenceEngine {
    engine: Arc<ChatEngine>,
    family: ModelFamily,
    context_window: u32,
    default_temperature: f32,
}

impl LlamaChatInferenceEngine {
    /// Create a new engine, load the GGUF model, and return the bridge.
    ///
    /// This is a blocking operation (model load + Metal kernel compilation)
    /// and should be called from a context that can tolerate latency.
    pub fn load(
        model_path: &str,
        family: ModelFamily,
        config: ChatConfig,
    ) -> Result<Self, InferenceError> {
        let context_window = config.n_ctx;
        let default_temperature = config.default_temperature;

        let engine = ChatEngine::new(config)
            .map_err(|e| InferenceError::Engine(format!("Failed to create ChatEngine: {e}")))?;

        engine
            .load_model(model_path)
            .map_err(|e| InferenceError::Engine(format!("Failed to load model: {e}")))?;

        Ok(Self {
            engine: Arc::new(engine),
            family,
            context_window,
            default_temperature,
        })
    }
}

#[async_trait]
impl ChatInferenceEngine for LlamaChatInferenceEngine {
    async fn generate(
        &self,
        request: InferenceRequest,
        on_chunk: Box<dyn Fn(StreamingChunk) + Send>,
    ) -> Result<InferenceUsage, InferenceError> {
        // Convert ToolDefinition → ToolSpec
        let tools: Option<Vec<ToolSpec>> = request.tools.map(|defs| {
            defs.into_iter()
                .map(|d| ToolSpec {
                    name: d.name,
                    description: d.description,
                    parameters_schema: d.parameters_schema,
                })
                .collect()
        });

        let temperature = request.temperature.unwrap_or(self.default_temperature);
        // 4096 fallback for GGUF path when no cap is requested (tool-calling iterations
        // pass max_tokens: None). Ollama uses stream:false for tool turns so its
        // effective cap is the model's own EOS — the two backends diverge here intentionally.
        let max_tokens = request.max_tokens.unwrap_or(4096);

        // Bridge ChatChunk → StreamingChunk
        let usage_result = self
            .engine
            .generate_streaming(
                request.messages,
                tools,
                temperature,
                max_tokens,
                move |chunk| {
                    match chunk {
                        ChatChunk::Token(text) => {
                            on_chunk(StreamingChunk::Token { text });
                        }
                        ChatChunk::Reasoning(text) => {
                            on_chunk(StreamingChunk::Reasoning { text });
                        }
                        ChatChunk::ToolCallStart { id, name } => {
                            on_chunk(StreamingChunk::ToolCallStart { id, name });
                        }
                        ChatChunk::ToolCallArgs { id, json } => {
                            on_chunk(StreamingChunk::ToolCallArgs {
                                id,
                                args_json: json,
                            });
                        }
                        ChatChunk::Done => {
                            // Done is handled by the return value, not a chunk
                        }
                        ChatChunk::Error(msg) => {
                            tracing::error!("Inference error chunk: {}", msg);
                        }
                    }
                },
            )
            .await
            .map_err(|e| InferenceError::Engine(e.to_string()))?;

        Ok(InferenceUsage {
            prompt_tokens: usage_result.prompt_tokens,
            completion_tokens: usage_result.completion_tokens,
        })
    }

    async fn model_info(&self) -> Result<Option<ChatModelSpec>, InferenceError> {
        Ok(Some(ChatModelSpec {
            model_id: self
                .engine
                .model_info()
                .map(|info| info.model_path.clone())
                .unwrap_or_default(),
            family: self.family,
            context_window: self.context_window,
            default_temperature: self.default_temperature,
            type_k: None,
            type_v: None,
        }))
    }

    async fn token_count(&self, text: &str) -> Result<u32, InferenceError> {
        self.engine
            .token_count(text)
            .map_err(|e| InferenceError::Engine(e.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Convert ToolDefinition → ToolSpec (utility for external callers)
// ---------------------------------------------------------------------------

/// Convert app-crate `ToolDefinition` to nlp-engine `ToolSpec`.
pub fn to_tool_spec(def: &ToolDefinition) -> ToolSpec {
    ToolSpec {
        name: def.name.clone(),
        description: def.description.clone(),
        parameters_schema: def.parameters_schema.clone(),
    }
}
