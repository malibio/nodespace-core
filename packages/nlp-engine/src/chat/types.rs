/// Types for the chat inference engine.
use serde::{Deserialize, Serialize};

/// Configuration for the chat inference engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatConfig {
    /// Context window size in tokens.
    pub n_ctx: u32,

    /// Default sampling temperature (0.0 = deterministic, higher = more creative).
    pub default_temperature: f32,

    /// Number of GPU layers to offload. Use 99 to offload all.
    pub n_gpu_layers: u32,

    /// Number of threads for CPU computation.
    pub n_threads: i32,
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            n_ctx: 32_768,
            default_temperature: 0.1,
            n_gpu_layers: 99,
            n_threads: std::thread::available_parallelism()
                .map(|p| p.get() as i32)
                .unwrap_or(4),
        }
    }
}

impl ChatConfig {
    /// Validate configuration values.
    pub fn validate(&self) -> Result<(), String> {
        if self.n_ctx == 0 {
            return Err("n_ctx must be greater than 0".to_string());
        }
        if self.default_temperature < 0.0 {
            return Err("default_temperature must be non-negative".to_string());
        }
        Ok(())
    }
}

/// Role of a participant in a chat conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// System prompt providing instructions to the model.
    System,
    /// Message from the human user.
    User,
    /// Response from the AI assistant.
    Assistant,
    /// Output from a tool invocation.
    Tool,
}

impl Role {
    /// Canonical lowercase string used by chat templates and wire formats.
    pub fn as_str(self) -> &'static str {
        match self {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        }
    }
}

/// A raw tool call issued by an assistant message, carried back through a re-prompt.
///
/// The chat template renders these into the assistant turn (e.g. OpenAI-format
/// `tool_calls`) so the subsequent `tool` result messages have a matching call
/// to pair with. Omitting them leaves orphan tool results in the history, which
/// destabilizes generation on some models.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRaw {
    /// Unique id of this tool call (must match the paired tool result's call_id).
    pub id: String,
    /// Name of the invoked tool.
    pub function_name: String,
    /// Raw JSON arguments string as the model emitted them.
    pub arguments_json: String,
}

/// A single message in a chat conversation, used for both agent history and inference input.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Role of the message author.
    pub role: Role,
    /// Text content of the message.
    pub content: String,
    /// Tool calls this (assistant) message made, in order. Empty for non-tool turns.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCallRaw>,
    /// If this message is a tool result, the ID of the originating tool call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Optional name for tool-role messages (the tool name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The model's internal reasoning (chain-of-thought) for this assistant message.
    /// `None` for non-assistant turns or when the model produced no reasoning.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
}

impl ChatMessage {
    /// A plain text message (no tool calls). Covers system/user/assistant-text.
    pub fn text(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
            name: None,
            reasoning: None,
        }
    }

    /// An assistant message that issued one or more tool calls.
    pub fn assistant_with_tool_calls(
        content: impl Into<String>,
        tool_calls: Vec<ToolCallRaw>,
    ) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            tool_calls,
            tool_call_id: None,
            name: None,
            reasoning: None,
        }
    }

    /// A tool-result message paired to the tool call `tool_call_id`.
    pub fn tool_result(
        content: impl Into<String>,
        tool_call_id: impl Into<String>,
        name: impl Into<String>,
    ) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id.into()),
            name: Some(name.into()),
            reasoning: None,
        }
    }
}

/// Specification of a tool the model may invoke.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    /// Unique name of the tool.
    pub name: String,
    /// Human-readable description of what the tool does.
    pub description: String,
    /// JSON Schema describing the tool's parameters.
    pub parameters_schema: serde_json::Value,
}

/// A chunk emitted during streaming inference.
#[derive(Debug, Clone)]
pub enum ChatChunk {
    /// A generated text token (answer content shown to the user).
    Token(String),
    /// A span of the model's internal reasoning (chain-of-thought).
    ///
    /// Emitted for text the model wrapped in channel markers
    /// (`<|channel> … <channel|>`). Routed to a separate reasoning stream rather
    /// than the answer, so the answer bubble stays clean while the reasoning can
    /// be surfaced in a dedicated collapsible UI section.
    Reasoning(String),
    /// The model is starting a tool call.
    ToolCallStart {
        /// Unique identifier for this tool call.
        id: String,
        /// Name of the tool being invoked.
        name: String,
    },
    /// Incremental arguments JSON for an in-progress tool call.
    ToolCallArgs {
        /// Identifier matching the corresponding `ToolCallStart`.
        id: String,
        /// Partial JSON string of tool arguments.
        json: String,
    },
    /// Inference is complete.
    Done,
    /// An error occurred during streaming.
    Error(String),
}

/// Token usage statistics for a completed inference turn.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ChatUsage {
    /// Number of tokens in the input prompt.
    pub prompt_tokens: u32,
    /// Number of tokens generated by the model.
    pub completion_tokens: u32,
}

/// Information about the currently loaded model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedModelInfo {
    /// Path to the model file.
    pub model_path: String,
    /// Context window size.
    pub context_size: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_chat_config() {
        let config = ChatConfig::default();
        assert_eq!(config.n_ctx, 32_768);
        assert!((config.default_temperature - 0.1).abs() < f32::EPSILON);
        assert_eq!(config.n_gpu_layers, 99);
        assert!(config.n_threads > 0);
    }

    #[test]
    fn test_chat_config_validation() {
        let config = ChatConfig::default();
        assert!(config.validate().is_ok());

        let bad = ChatConfig {
            n_ctx: 0,
            ..Default::default()
        };
        assert!(bad.validate().is_err());

        let bad2 = ChatConfig {
            default_temperature: -1.0,
            ..Default::default()
        };
        assert!(bad2.validate().is_err());
    }

    #[test]
    fn test_role_as_str() {
        assert_eq!(Role::System.as_str(), "system");
        assert_eq!(Role::User.as_str(), "user");
        assert_eq!(Role::Assistant.as_str(), "assistant");
        assert_eq!(Role::Tool.as_str(), "tool");
    }

    #[test]
    fn test_chat_message_constructors() {
        let m = ChatMessage::text(Role::User, "hello");
        assert_eq!(m.role, Role::User);
        assert_eq!(m.content, "hello");
        assert!(m.tool_calls.is_empty());
        assert!(m.tool_call_id.is_none());

        let tc = ToolCallRaw {
            id: "tc1".into(),
            function_name: "search".into(),
            arguments_json: "{}".into(),
        };
        let a = ChatMessage::assistant_with_tool_calls("", vec![tc]);
        assert_eq!(a.role, Role::Assistant);
        assert_eq!(a.tool_calls.len(), 1);

        let r = ChatMessage::tool_result("result", "tc1", "search");
        assert_eq!(r.role, Role::Tool);
        assert_eq!(r.tool_call_id.as_deref(), Some("tc1"));
        assert_eq!(r.name.as_deref(), Some("search"));
    }
}
