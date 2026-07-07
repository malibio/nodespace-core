use crate::agent_types::{
    ChatInferenceEngine, ChatModelSpec, InferenceError, InferenceRequest, InferenceUsage,
    ModelFamily, StreamingChunk,
};
use crate::local_agent::ollama_ndjson::NdjsonLineBuffer;
use async_trait::async_trait;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::OnceCell;

pub struct OllamaInferenceEngine {
    http_client: reqwest::Client,
    base_url: String,
    model_name: String,
    /// Cached context window size from /api/show — fetched once per engine instance.
    cached_context_window: OnceCell<u32>,
}

impl OllamaInferenceEngine {
    pub fn new(model_name: String) -> Self {
        Self {
            http_client: reqwest::Client::new(),
            base_url: "http://127.0.0.1:11434".to_string(),
            model_name,
            cached_context_window: OnceCell::new(),
        }
    }

    pub fn with_base_url(model_name: String, base_url: String) -> Self {
        Self {
            http_client: reqwest::Client::new(),
            base_url,
            model_name,
            cached_context_window: OnceCell::new(),
        }
    }
}

// Private types for Ollama API serialization/deserialization
#[derive(Serialize)]
struct OllamaChatRequest<'a> {
    model: &'a str,
    messages: Vec<OllamaMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OllamaTool>>,
    stream: bool,
    /// Disable chain-of-thought thinking for thinking models (e.g. gemma4).
    /// Thinking tokens count against num_predict and cause long delays on
    /// tool-calling requests without adding value for structured outputs.
    think: bool,
    options: OllamaOptions,
}

#[derive(Serialize)]
struct OllamaMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Serialize)]
struct OllamaTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OllamaFunction,
}

#[derive(Serialize)]
struct OllamaFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Serialize)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_ctx: Option<u32>,
}

#[derive(Deserialize)]
struct OllamaChatChunk {
    message: Option<OllamaMessageChunk>,
    done: bool,
    #[serde(default)]
    prompt_eval_count: u32,
    #[serde(default)]
    eval_count: u32,
}

#[derive(Deserialize)]
struct OllamaMessageChunk {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<OllamaToolCall>,
}

#[derive(Deserialize)]
struct OllamaToolCall {
    function: OllamaToolCallFunction,
}

#[derive(Deserialize)]
struct OllamaToolCallFunction {
    name: String,
    arguments: serde_json::Value,
}

#[derive(Deserialize)]
struct OllamaShowResponse {
    model_info: Option<serde_json::Value>,
}

#[async_trait]
impl ChatInferenceEngine for OllamaInferenceEngine {
    async fn generate(
        &self,
        request: InferenceRequest,
        on_chunk: Box<dyn Fn(StreamingChunk) + Send>,
    ) -> Result<InferenceUsage, InferenceError> {
        // Map ChatMessage to OllamaMessage
        let messages: Vec<OllamaMessage> = request
            .messages
            .iter()
            .map(|msg| OllamaMessage {
                role: msg.role.as_str().to_string(),
                content: msg.content.clone(),
                tool_call_id: msg.tool_call_id.clone(),
            })
            .collect();

        // Map ToolDefinition to OllamaTool
        let tools = request.tools.map(|tool_defs| {
            tool_defs
                .iter()
                .map(|t| OllamaTool {
                    tool_type: "function".to_string(),
                    function: OllamaFunction {
                        name: t.name.clone(),
                        description: t.description.clone(),
                        parameters: t.parameters_schema.clone(),
                    },
                })
                .collect()
        });

        // When tools are present, set num_ctx so Ollama uses the model's full context
        // instead of its 4096-token default. The value is fetched from /api/show once
        // per engine instance (cached) and falls back to 32768 if unreachable.
        let num_ctx = if tools.is_some() {
            let ctx = self
                .cached_context_window
                .get_or_init(|| async {
                    match self.model_info().await {
                        Ok(Some(spec)) => spec.context_window,
                        _ => 32_768,
                    }
                })
                .await;
            Some(*ctx)
        } else {
            None
        };

        // Ollama does not deliver tool_calls through its streaming API for Gemma 4
        // (and likely other models) — tool calls only appear in the final
        // non-streaming response. Use stream:false so tool calls are reliably
        // returned, then emit the response content as a single chunk to preserve
        // the on_chunk streaming interface for callers.
        let use_stream = tools.is_none();
        let ollama_request = OllamaChatRequest {
            model: &self.model_name,
            messages,
            tools,
            stream: use_stream,
            think: false,
            options: OllamaOptions {
                temperature: request.temperature,
                num_predict: request.max_tokens,
                num_ctx,
            },
        };

        let url = format!("{}/api/chat", self.base_url);

        // Log the full request payload at DEBUG level so we can inspect exactly
        // what is sent to Ollama (model, messages, tools, options).
        if tracing::enabled!(tracing::Level::DEBUG) {
            if let Ok(json) = serde_json::to_string_pretty(&ollama_request) {
                tracing::debug!(model = %self.model_name, payload = %json, "Ollama request");
            }
        }
        // Always log message count + system prompt length at INFO to make it
        // easy to see the prompt size in production logs without full verbosity.
        let system_len = ollama_request
            .messages
            .first()
            .filter(|m| m.role == "system")
            .map(|m| m.content.len())
            .unwrap_or(0);
        tracing::info!(
            model = %self.model_name,
            message_count = ollama_request.messages.len(),
            system_prompt_bytes = system_len,
            tool_count = ollama_request.tools.as_ref().map(|t| t.len()).unwrap_or(0),
            "Sending request to Ollama"
        );

        let response = self
            .http_client
            .post(&url)
            .json(&ollama_request)
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
                "Ollama API error {}: {}",
                status, body
            )));
        }

        let mut final_usage = InferenceUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
        };

        if use_stream {
            // Streaming path: used when no tools are present.
            let mut stream = response.bytes_stream();
            let mut buffer = NdjsonLineBuffer::new();

            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| InferenceError::Engine(e.to_string()))?;
                let lines = buffer
                    .push(&chunk)
                    .map_err(|e| InferenceError::Engine(e.to_string()))?;

                for line in lines {
                    match serde_json::from_str::<OllamaChatChunk>(&line) {
                        Ok(chunk_data) => {
                            if let Some(msg) = &chunk_data.message {
                                if let Some(content) = &msg.content {
                                    if !content.is_empty() {
                                        on_chunk(StreamingChunk::Token {
                                            text: content.clone(),
                                        });
                                    }
                                }
                            }
                            if chunk_data.done {
                                final_usage = InferenceUsage {
                                    prompt_tokens: chunk_data.prompt_eval_count,
                                    completion_tokens: chunk_data.eval_count,
                                };
                                on_chunk(StreamingChunk::Done { usage: final_usage });
                            }
                        }
                        Err(e) => tracing::warn!("Failed to parse Ollama chunk: {e}"),
                    }
                }
            }
        } else {
            // Non-streaming path: used when tools are present. Ollama does not
            // deliver tool_calls in the streaming API for Gemma 4 — the full
            // response must be read at once to get structured tool_calls.
            let body = response
                .text()
                .await
                .map_err(|e| InferenceError::Engine(e.to_string()))?;

            match serde_json::from_str::<OllamaChatChunk>(&body) {
                Ok(chunk_data) => {
                    if let Some(msg) = &chunk_data.message {
                        if let Some(content) = &msg.content {
                            if !content.is_empty() {
                                on_chunk(StreamingChunk::Token {
                                    text: content.clone(),
                                });
                            }
                        }
                        for (i, tool_call) in msg.tool_calls.iter().enumerate() {
                            let call_id = format!("call_{}", i);
                            on_chunk(StreamingChunk::ToolCallStart {
                                id: call_id.clone(),
                                name: tool_call.function.name.clone(),
                            });
                            let args_json = serde_json::to_string(&tool_call.function.arguments)
                                .unwrap_or_default();
                            on_chunk(StreamingChunk::ToolCallArgs {
                                id: call_id,
                                args_json,
                            });
                        }
                    }
                    final_usage = InferenceUsage {
                        prompt_tokens: chunk_data.prompt_eval_count,
                        completion_tokens: chunk_data.eval_count,
                    };
                    on_chunk(StreamingChunk::Done { usage: final_usage });
                }
                Err(e) => {
                    tracing::warn!("Failed to parse Ollama non-streaming response: {e}");
                    // Emit Done so callers are not left waiting for a terminal chunk.
                    on_chunk(StreamingChunk::Done {
                        usage: InferenceUsage::default(),
                    });
                }
            }
        }

        Ok(final_usage)
    }

    async fn model_info(&self) -> Result<Option<ChatModelSpec>, InferenceError> {
        let url = format!("{}/api/show", self.base_url);
        let show_request = serde_json::json!({ "name": self.model_name });

        match self.http_client.post(&url).json(&show_request).send().await {
            Ok(response) => match response.json::<OllamaShowResponse>().await {
                Ok(show_response) => {
                    let context_window = show_response
                        .model_info
                        .as_ref()
                        .and_then(|info| {
                            // Try well-known family-specific keys first, then fall back to
                            // any key ending in ".context_length" (covers future model families).
                            let known_keys = [
                                "llm.context_length",
                                "gemma4.context_length",
                                "llama.context_length",
                                "qwen2.context_length",
                                "mistral.context_length",
                                "phi3.context_length",
                            ];
                            known_keys
                                .iter()
                                .find_map(|k| info.get(*k).and_then(|v| v.as_u64()))
                                .or_else(|| {
                                    info.as_object().and_then(|obj| {
                                        obj.iter()
                                            .find(|(k, _)| k.ends_with(".context_length"))
                                            .and_then(|(_, v)| v.as_u64())
                                    })
                                })
                        })
                        .map(|n| n as u32)
                        .unwrap_or(32_768);

                    Ok(Some(ChatModelSpec {
                        model_id: self.model_name.clone(),
                        family: ModelFamily::Ollama,
                        context_window,
                        default_temperature: 0.7,
                        type_k: None,
                        type_v: None,
                    }))
                }
                Err(e) => {
                    tracing::warn!(
                        "ollama: failed to parse /api/show response for '{}': {e}",
                        self.model_name
                    );
                    Ok(None)
                }
            },
            Err(_) => Ok(None), // daemon unreachable — model_info is optional
        }
    }

    async fn token_count(&self, text: &str) -> Result<u32, InferenceError> {
        Ok((text.len() / 4) as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_types::Role;

    #[test]
    fn test_token_count_estimate() {
        let engine = OllamaInferenceEngine::new("llama3.2:3b".to_string());
        let text = "hello world"; // len=11, 11/4=2 by integer division
        let count = futures::executor::block_on(engine.token_count(text))
            .expect("token_count should succeed");
        assert_eq!(
            count,
            2,
            "token_count should be text.len()/4 = {}",
            text.len() / 4
        );
    }

    #[test]
    fn test_role_as_str() {
        assert_eq!(Role::System.as_str(), "system");
        assert_eq!(Role::User.as_str(), "user");
        assert_eq!(Role::Assistant.as_str(), "assistant");
        assert_eq!(Role::Tool.as_str(), "tool");
    }

    #[test]
    fn test_parse_ollama_chat_chunk_token() {
        let json = r#"{
            "message": {
                "content": "hello",
                "tool_calls": []
            },
            "done": false,
            "prompt_eval_count": 0,
            "eval_count": 0
        }"#;

        let chunk: OllamaChatChunk =
            serde_json::from_str(json).expect("should deserialize OllamaChatChunk");
        assert!(!chunk.done);
        assert_eq!(chunk.eval_count, 0);
        assert_eq!(chunk.prompt_eval_count, 0);

        if let Some(msg) = chunk.message {
            assert_eq!(msg.content, Some("hello".to_string()));
            assert!(msg.tool_calls.is_empty());
        } else {
            panic!("message should not be None");
        }
    }

    #[test]
    fn test_parse_ollama_chat_chunk_done() {
        let json = r#"{
            "message": null,
            "done": true,
            "prompt_eval_count": 10,
            "eval_count": 20
        }"#;

        let chunk: OllamaChatChunk =
            serde_json::from_str(json).expect("should deserialize OllamaChatChunk");
        assert!(chunk.done);
        assert_eq!(chunk.prompt_eval_count, 10);
        assert_eq!(chunk.eval_count, 20);
        assert!(chunk.message.is_none());
    }

    #[test]
    fn test_parse_ollama_tool_call() {
        let json = r#"{
            "message": {
                "content": null,
                "tool_calls": [
                    {
                        "function": {
                            "name": "search",
                            "arguments": {"query": "test"}
                        }
                    }
                ]
            },
            "done": false,
            "prompt_eval_count": 0,
            "eval_count": 0
        }"#;

        let chunk: OllamaChatChunk =
            serde_json::from_str(json).expect("should deserialize OllamaChatChunk");

        if let Some(msg) = chunk.message {
            assert_eq!(msg.tool_calls.len(), 1);
            let tool_call = &msg.tool_calls[0];
            assert_eq!(tool_call.function.name, "search");
            assert_eq!(
                tool_call
                    .function
                    .arguments
                    .get("query")
                    .and_then(|v| v.as_str()),
                Some("test")
            );
        } else {
            panic!("message should not be None");
        }
    }
}
