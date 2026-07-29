//! `ChatInferenceEngine` implementation for user-configured OpenAI-compatible
//! HTTP endpoints (e.g. a self-hosted vLLM/LM Studio server, or Ollama's
//! `/v1` endpoint).
//!
//! This is the single path for every remotely-served model. Ollama is reached
//! here through its OpenAI-compatible `/v1` API rather than its native wire
//! format — one protocol implementation serves every such provider.

use crate::agent_types::{
    ChatInferenceEngine, ChatModelSpec, InferenceError, InferenceRequest, InferenceUsage,
    ModelFamily, StreamingChunk,
};
use crate::local_agent::ndjson::NdjsonLineBuffer;
use async_trait::async_trait;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Prefix used to identify OpenAI-compatible provider configs. The suffix is
/// the UUID of the config stored in daemon.toml (`[[openai_compat.configs]]`).
pub const OPENAI_COMPAT_PREFIX: &str = "openai-compat:";

/// Check if a model ID represents an OpenAI-compatible provider config.
pub fn is_openai_compat(model_id: &str) -> bool {
    model_id.starts_with(OPENAI_COMPAT_PREFIX)
}

/// Strip the `"openai-compat:"` prefix, returning everything after it.
///
/// Returns the original model ID unchanged if it does not have the prefix.
/// Prefer [`parse_openai_compat_id`] when you need the config UUID on its own —
/// the remainder may also carry a discovered model segment.
pub fn strip_openai_compat_prefix(model_id: &str) -> &str {
    model_id
        .strip_prefix(OPENAI_COMPAT_PREFIX)
        .unwrap_or(model_id)
}

/// Split an `openai-compat:` model ID into its config UUID and optional model.
///
/// Two forms are accepted:
/// - `openai-compat:<uuid>` — the config's own `model` field is used.
/// - `openai-compat:<uuid>:<model>` — a specific model discovered at that
///   endpoint, which is how one config exposes the several models a server
///   serves.
///
/// The split is on the **first** colon after the prefix: a UUID never contains
/// one, whereas a model identifier routinely does (`mistral:7b`), so anything
/// past that first separator belongs to the model.
pub fn parse_openai_compat_id(model_id: &str) -> (&str, Option<&str>) {
    let rest = strip_openai_compat_prefix(model_id);
    match rest.split_once(':') {
        Some((config_id, model)) if !model.is_empty() => (config_id, Some(model)),
        _ => (rest, None),
    }
}

pub struct OpenAiCompatInferenceEngine {
    http_client: reqwest::Client,
    /// Base URL of the endpoint, e.g. "https://api.openai.com/v1". Requests
    /// are sent to "<base_url>/chat/completions".
    base_url: String,
    api_key: String,
    /// Model identifier sent as the request body's "model" field — must be
    /// the provider's actual model id (e.g. "gpt-4o"), not a cosmetic UI
    /// label. Real OpenAI-API and multi-model servers reject or misroute an
    /// unrecognized value; single-model local servers (Ollama, LM Studio)
    /// generally ignore it.
    model_name: String,
}

/// Time allowed to establish a TCP connection to the endpoint.
///
/// Deliberately short: a wrong or dead `base_url` should surface as an error in
/// seconds rather than hanging the chat turn.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Time allowed for a single request, measured to the *end* of the response body.
///
/// A generation legitimately runs for minutes on a slow local model, so this is
/// far longer than a typical HTTP timeout. Its job is to bound the pathological
/// case the bare default could not: an endpoint that accepts the connection and
/// then goes silent forever. `reqwest` surfaces an error only on transport
/// failure, and a connected-but-stalled server is not one.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(600);

impl OpenAiCompatInferenceEngine {
    pub fn new(base_url: String, api_key: String, model_name: String) -> Self {
        Self {
            http_client: Self::build_http_client(),
            base_url,
            api_key,
            model_name,
        }
    }

    /// Build the HTTP client with connect and request timeouts applied.
    ///
    /// Falls back to a default client if the builder fails, which it does only
    /// on TLS backend initialization errors — losing the timeouts is strictly
    /// better than refusing to construct the engine at all.
    fn build_http_client() -> reqwest::Client {
        reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    }

    /// The wire-protocol model identifier this engine sends as the request
    /// body's "model" field. Exposed so callers (and tests) can distinguish
    /// it from a config's cosmetic `name` — see
    /// `LocalAgentService::load_model_and_collect_events`, which must pass
    /// `config.model`, not `config.name`, into [`Self::new`].
    pub fn model_name(&self) -> &str {
        &self.model_name
    }
}

// ---------------------------------------------------------------------------
// Private types for OpenAI chat-completions API serialization/deserialization
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct OpenAiChatRequest<'a> {
    model: &'a str,
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAiTool>>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Serialize)]
struct OpenAiMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Serialize)]
struct OpenAiTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OpenAiFunction,
}

#[derive(Serialize)]
struct OpenAiFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

/// Shape shared by both the non-streaming response body and each streaming
/// SSE `data:` chunk — OpenAI's `chat.completion` and `chat.completion.chunk`
/// objects differ only in whether `message` or `delta` is populated.
#[derive(Deserialize)]
struct OpenAiChatResponse {
    #[serde(default)]
    choices: Vec<OpenAiChoice>,
    #[serde(default)]
    usage: Option<OpenAiUsage>,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    #[serde(default)]
    message: Option<OpenAiResponseMessage>,
    #[serde(default)]
    delta: Option<OpenAiResponseMessage>,
}

#[derive(Deserialize, Default)]
struct OpenAiResponseMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<OpenAiToolCall>,
}

#[derive(Deserialize)]
struct OpenAiToolCall {
    #[serde(default)]
    id: Option<String>,
    function: OpenAiToolCallFunction,
}

#[derive(Deserialize)]
struct OpenAiToolCallFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: String,
}

#[derive(Deserialize)]
struct OpenAiUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
}

#[async_trait]
impl ChatInferenceEngine for OpenAiCompatInferenceEngine {
    async fn generate(
        &self,
        request: InferenceRequest,
        on_chunk: Box<dyn Fn(StreamingChunk) + Send>,
    ) -> Result<InferenceUsage, InferenceError> {
        let messages: Vec<OpenAiMessage> = request
            .messages
            .iter()
            .map(|msg| OpenAiMessage {
                role: msg.role.as_str().to_string(),
                content: msg.content.clone(),
                tool_call_id: msg.tool_call_id.clone(),
            })
            .collect();

        let tools = request.tools.map(|tool_defs| {
            tool_defs
                .iter()
                .map(|t| OpenAiTool {
                    tool_type: "function".to_string(),
                    function: OpenAiFunction {
                        name: t.name.clone(),
                        description: t.description.clone(),
                        parameters: t.parameters_schema.clone(),
                    },
                })
                .collect()
        });

        // Tool-calling responses are not reliably delivered incrementally by
        // every OpenAI-compatible server, so use the same non-streaming
        // fallback strategy as the Ollama engine when tools are present.
        let use_stream = tools.is_none();
        let openai_request = OpenAiChatRequest {
            model: &self.model_name,
            messages,
            tools,
            stream: use_stream,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
        };

        if tracing::enabled!(tracing::Level::DEBUG) {
            if let Ok(json) = serde_json::to_string_pretty(&openai_request) {
                tracing::debug!(base_url = %self.base_url, payload = %json, "OpenAI-compat request");
            }
        }
        tracing::info!(
            base_url = %self.base_url,
            message_count = openai_request.messages.len(),
            tool_count = openai_request.tools.as_ref().map(|t| t.len()).unwrap_or(0),
            "Sending request to OpenAI-compatible endpoint"
        );

        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let mut req_builder = self.http_client.post(&url).json(&openai_request);
        if !self.api_key.is_empty() {
            req_builder = req_builder.bearer_auth(&self.api_key);
        }

        let response = req_builder
            .send()
            .await
            .map_err(|e| InferenceError::Engine(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            return Err(InferenceError::Engine(format!(
                "OpenAI-compat API error {}: {}",
                status, body
            )));
        }

        let mut final_usage = InferenceUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
        };

        if use_stream {
            let mut stream = response.bytes_stream();
            let mut buffer = NdjsonLineBuffer::new();
            // Accumulates streamed tool-call fragments by index, since the
            // SSE delta format sends id/name once and arguments incrementally.
            let mut tool_call_ids: Vec<Option<String>> = Vec::new();

            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| InferenceError::Engine(e.to_string()))?;
                let lines = buffer
                    .push(&chunk)
                    .map_err(|e| InferenceError::Engine(e.to_string()))?;

                for line in lines {
                    let Some(data) = line.strip_prefix("data:") else {
                        continue;
                    };
                    let data = data.trim();
                    if data == "[DONE]" {
                        on_chunk(StreamingChunk::Done { usage: final_usage });
                        return Ok(final_usage);
                    }

                    match serde_json::from_str::<OpenAiChatResponse>(data) {
                        Ok(resp) => {
                            if let Some(usage) = &resp.usage {
                                final_usage = InferenceUsage {
                                    prompt_tokens: usage.prompt_tokens,
                                    completion_tokens: usage.completion_tokens,
                                };
                            }
                            for choice in &resp.choices {
                                if let Some(delta) = &choice.delta {
                                    if let Some(content) = &delta.content {
                                        if !content.is_empty() {
                                            on_chunk(StreamingChunk::Token {
                                                text: content.clone(),
                                            });
                                        }
                                    }
                                    for (i, tool_call) in delta.tool_calls.iter().enumerate() {
                                        if tool_call_ids.len() <= i {
                                            tool_call_ids.resize(i + 1, None);
                                        }
                                        if let Some(id) = &tool_call.id {
                                            tool_call_ids[i] = Some(id.clone());
                                            on_chunk(StreamingChunk::ToolCallStart {
                                                id: id.clone(),
                                                name: tool_call
                                                    .function
                                                    .name
                                                    .clone()
                                                    .unwrap_or_default(),
                                            });
                                        }
                                        if !tool_call.function.arguments.is_empty() {
                                            let id = tool_call_ids
                                                .get(i)
                                                .cloned()
                                                .flatten()
                                                .unwrap_or_else(|| format!("call_{}", i));
                                            on_chunk(StreamingChunk::ToolCallArgs {
                                                id,
                                                args_json: tool_call.function.arguments.clone(),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => tracing::warn!("Failed to parse OpenAI-compat SSE chunk: {e}"),
                    }
                }
            }
            // Stream ended without an explicit [DONE] sentinel.
            on_chunk(StreamingChunk::Done { usage: final_usage });
        } else {
            let body = response
                .text()
                .await
                .map_err(|e| InferenceError::Engine(e.to_string()))?;

            match serde_json::from_str::<OpenAiChatResponse>(&body) {
                Ok(resp) => {
                    if let Some(usage) = &resp.usage {
                        final_usage = InferenceUsage {
                            prompt_tokens: usage.prompt_tokens,
                            completion_tokens: usage.completion_tokens,
                        };
                    }
                    if let Some(choice) = resp.choices.first() {
                        if let Some(msg) = &choice.message {
                            if let Some(content) = &msg.content {
                                if !content.is_empty() {
                                    on_chunk(StreamingChunk::Token {
                                        text: content.clone(),
                                    });
                                }
                            }
                            for (i, tool_call) in msg.tool_calls.iter().enumerate() {
                                let call_id = tool_call
                                    .id
                                    .clone()
                                    .unwrap_or_else(|| format!("call_{}", i));
                                on_chunk(StreamingChunk::ToolCallStart {
                                    id: call_id.clone(),
                                    name: tool_call.function.name.clone().unwrap_or_default(),
                                });
                                on_chunk(StreamingChunk::ToolCallArgs {
                                    id: call_id,
                                    args_json: tool_call.function.arguments.clone(),
                                });
                            }
                        }
                    }
                    on_chunk(StreamingChunk::Done { usage: final_usage });
                }
                Err(e) => {
                    tracing::warn!("Failed to parse OpenAI-compat non-streaming response: {e}");
                    on_chunk(StreamingChunk::Done {
                        usage: InferenceUsage::default(),
                    });
                }
            }
        }

        Ok(final_usage)
    }

    async fn model_info(&self) -> Result<Option<ChatModelSpec>, InferenceError> {
        // OpenAI-compatible servers have no standard equivalent of Ollama's
        // /api/show for context-window introspection. Callers fall back to a
        // conservative default via the None case.
        Ok(Some(ChatModelSpec {
            model_id: self.model_name.clone(),
            family: ModelFamily::OpenAiCompat,
            context_window: 32_768,
            default_temperature: 0.7,
            type_k: None,
            type_v: None,
        }))
    }

    async fn token_count(&self, text: &str) -> Result<u32, InferenceError> {
        Ok((text.len() / 4) as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_openai_compat_prefix() {
        assert!(is_openai_compat("openai-compat:abc-123"));
        assert!(!is_openai_compat("ollama:llama3.2:3b"));
        assert!(!is_openai_compat("ministral-3b-q4km"));
        assert!(is_openai_compat("openai-compat:"));
    }

    #[test]
    fn test_strip_openai_compat_prefix() {
        assert_eq!(
            strip_openai_compat_prefix("openai-compat:abc-123"),
            "abc-123"
        );
        assert_eq!(strip_openai_compat_prefix("ministral"), "ministral");
    }

    #[test]
    fn test_parse_non_streaming_response_with_tool_calls() {
        let json = r#"{
            "choices": [{
                "message": {
                    "content": null,
                    "tool_calls": [
                        {
                            "id": "call_abc",
                            "function": {
                                "name": "search",
                                "arguments": "{\"query\":\"test\"}"
                            }
                        }
                    ]
                }
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5}
        }"#;

        let resp: OpenAiChatResponse = serde_json::from_str(json).expect("should deserialize");
        let usage = resp.usage.expect("usage should be present");
        assert_eq!(usage.prompt_tokens, 10);
        assert_eq!(usage.completion_tokens, 5);

        let msg = resp.choices[0].message.as_ref().expect("message present");
        assert_eq!(msg.tool_calls.len(), 1);
        assert_eq!(msg.tool_calls[0].id.as_deref(), Some("call_abc"));
        assert_eq!(msg.tool_calls[0].function.name.as_deref(), Some("search"));
        assert_eq!(msg.tool_calls[0].function.arguments, "{\"query\":\"test\"}");
    }

    #[test]
    fn test_parse_streaming_delta_token() {
        let json = r#"{
            "choices": [{
                "delta": {"content": "hello"}
            }]
        }"#;

        let resp: OpenAiChatResponse = serde_json::from_str(json).expect("should deserialize");
        let delta = resp.choices[0].delta.as_ref().expect("delta present");
        assert_eq!(delta.content.as_deref(), Some("hello"));
        assert!(delta.tool_calls.is_empty());
    }

    #[test]
    fn test_token_count_estimate() {
        let engine = OpenAiCompatInferenceEngine::new(
            "http://127.0.0.1:11434/v1".to_string(),
            String::new(),
            "placeholder".to_string(),
        );
        let text = "hello world"; // len=11, 11/4=2 by integer division
        let count = futures::executor::block_on(engine.token_count(text))
            .expect("token_count should succeed");
        assert_eq!(count, 2);
    }

    #[test]
    fn model_name_reflects_the_constructor_arg_not_a_display_label() {
        // Regression guard: callers (LocalAgentService::load_model_and_collect_events)
        // must pass the config's wire-protocol `model` field into `new`, never the
        // cosmetic `name` field a user typed into the Settings UI.
        let engine = OpenAiCompatInferenceEngine::new(
            "https://api.openai.com/v1".to_string(),
            "sk-test".to_string(),
            "gpt-4o".to_string(),
        );
        assert_eq!(engine.model_name(), "gpt-4o");
    }
}
