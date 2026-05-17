//! Shared types, traits, and interface contracts for agent subsystems.
//!
//! This module defines the foundational type definitions, trait interfaces,
//! and message formats that all agent-related subsystems code against. It
//! produces no runtime behavior -- only type definitions, trait declarations,
//! and module scaffolding.
//!
//! Tauri event channel constants live in the desktop-app crate (they depend
//! on Tauri, which is not a dependency of this crate).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors returned by [`ChatInferenceEngine`] methods.
#[derive(Debug, Error)]
pub enum InferenceError {
    /// No model is currently loaded.
    #[error("no model loaded")]
    NoModelLoaded,

    /// The model ran out of context window space.
    #[error("context window exceeded: {0}")]
    ContextOverflow(String),

    /// An internal engine error occurred.
    #[error("inference engine error: {0}")]
    Engine(String),

    /// Catch-all for unexpected errors.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Errors returned by [`ModelManager`] methods.
#[derive(Debug, Error)]
pub enum ModelError {
    /// The requested model ID does not exist in the catalog.
    #[error("model not found: {0}")]
    NotFound(String),

    /// A download was already in progress for this model.
    #[error("download already in progress for model: {0}")]
    DownloadInProgress(String),

    /// Network or I/O failure during download.
    #[error("download failed: {0}")]
    DownloadFailed(String),

    /// Verification (SHA-256 checksum) failed after download.
    #[error("verification failed for model: {0}")]
    VerificationFailed(String),

    /// The model file could not be loaded into memory.
    #[error("failed to load model: {0}")]
    LoadFailed(String),

    /// Catch-all for unexpected errors.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Errors returned by [`AgentToolExecutor`] methods.
#[derive(Debug, Error)]
pub enum ToolError {
    /// The requested tool name is not registered.
    #[error("unknown tool: {0}")]
    UnknownTool(String),

    /// The tool received invalid arguments.
    #[error("invalid arguments for tool {tool}: {reason}")]
    InvalidArguments {
        /// Name of the tool.
        tool: String,
        /// Explanation of what was wrong.
        reason: String,
    },

    /// The tool execution itself failed.
    #[error("tool execution failed: {0}")]
    ExecutionFailed(String),

    /// Catch-all for unexpected errors.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Errors returned when writing a context file for a PTY agent session (ADR-032).
#[derive(Debug, Error)]
pub enum ContextError {
    /// The requested node could not be found.
    #[error("node not found: {0}")]
    NodeNotFound(String),

    /// Writing the context file to disk failed.
    #[error("context file write failed: {0}")]
    WriteFailed(#[from] std::io::Error),

    /// Catch-all for unexpected errors.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

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

/// A single chunk emitted during streaming inference.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamingChunk {
    /// A token of generated text.
    Token {
        /// The text content of this token.
        text: String,
    },
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
        args_json: String,
    },
    /// Inference is complete.
    Done {
        /// Token usage statistics for the completed turn.
        usage: InferenceUsage,
    },
    /// An error occurred during streaming.
    Error {
        /// Human-readable error description.
        message: String,
    },
}

/// Current status of a model in the local catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ModelStatus {
    /// Model is known but not yet downloaded.
    NotDownloaded,
    /// Model is currently being downloaded.
    Downloading {
        /// Download progress as a percentage (0.0 -- 100.0).
        progress_pct: f32,
        /// Bytes downloaded so far.
        bytes_downloaded: u64,
        /// Total bytes to download.
        bytes_total: u64,
    },
    /// Download complete, verifying integrity (SHA-256).
    Verifying,
    /// Model is on disk and ready to be loaded.
    Ready,
    /// Model is loaded into memory and available for inference.
    Loaded,
    /// An error occurred (download, verification, or loading).
    Error {
        /// Human-readable error description.
        message: String,
    },
}

/// External agent CLI catalogued for PTY spawning (ADR-032).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentType {
    ClaudeCode,
    Codex,
    GeminiCli,
    Pi,
    OpenCode,
}

/// Context file convention an external agent expects on launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContextFile {
    /// Claude Code reads `CLAUDE.md` from its working directory.
    ClaudeMd,
    /// All other supported agents read `AGENTS.md`.
    AgentsMd,
}

impl ContextFile {
    /// Filename written to the session directory.
    pub fn filename(self) -> &'static str {
        match self {
            ContextFile::ClaudeMd => "CLAUDE.md",
            ContextFile::AgentsMd => "AGENTS.md",
        }
    }
}

/// Current status of the local agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum LocalAgentStatus {
    /// Agent is idle, waiting for user input.
    Idle,
    /// Agent is processing a request (pre-generation).
    Thinking,
    /// Agent is executing a tool.
    ToolExecution {
        /// Name of the tool currently being executed.
        tool_name: String,
    },
    /// Agent is streaming a response to the user.
    Streaming,
    /// Agent encountered an error.
    Error {
        /// Human-readable error description.
        message: String,
    },
}

/// Family of language models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelFamily {
    /// Ministral -- Mistral AI's small model series (Ministral 3B, Ministral 8B).
    Ministral,
    /// Gemma 4 -- Google's multimodal model series (E4B, 31B).
    Gemma4,
    /// Model served via Ollama (family determined by Ollama).
    Ollama,
}

/// Backend used to serve a language model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ModelBackend {
    /// Local GGUF model loaded via llama.cpp.
    #[default]
    Gguf,
    /// Model served by a local Ollama daemon.
    Ollama,
}

// ---------------------------------------------------------------------------
// Structs -- Chat & Inference
// ---------------------------------------------------------------------------

/// A single message in a chat conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Role of the message author.
    pub role: Role,
    /// Text content of the message.
    pub content: String,
    /// If this message is a tool result, the ID of the originating tool call.
    pub tool_call_id: Option<String>,
    /// Optional name for tool-role messages (the tool name).
    pub name: Option<String>,
}

/// Parameters for an inference request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    /// Ordered list of chat messages forming the conversation.
    pub messages: Vec<ChatMessage>,
    /// Tool definitions available for the model to invoke.
    pub tools: Option<Vec<ToolDefinition>>,
    /// Sampling temperature (0.0 = deterministic, higher = more creative).
    pub temperature: Option<f32>,
    /// Maximum number of tokens to generate.
    pub max_tokens: Option<u32>,
}

/// Token usage statistics for a completed inference turn.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct InferenceUsage {
    /// Number of tokens in the input prompt.
    pub prompt_tokens: u32,
    /// Number of tokens generated by the model.
    pub completion_tokens: u32,
}

// ---------------------------------------------------------------------------
// Structs -- Tools
// ---------------------------------------------------------------------------

/// Definition of a tool that the model can invoke.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Unique name of the tool (e.g. "search_nodes").
    pub name: String,
    /// Human-readable description of what the tool does.
    pub description: String,
    /// JSON Schema describing the tool's parameters.
    pub parameters_schema: serde_json::Value,
}

/// Result of a single tool invocation, returned to the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// ID of the tool call this result corresponds to.
    pub tool_call_id: String,
    /// Name of the tool that was executed.
    pub name: String,
    /// The output produced by the tool.
    pub result: serde_json::Value,
    /// Whether the tool execution itself failed.
    pub is_error: bool,
}

/// A raw tool call parsed from model output before execution.
///
/// Represents the model's intent to invoke a tool. The `arguments_json` field
/// contains the raw JSON string as emitted by the model (may need validation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRaw {
    /// Unique identifier for this tool call (from the model).
    pub id: String,
    /// Name of the tool the model wants to invoke.
    pub function_name: String,
    /// Raw JSON string of tool arguments as produced by the model.
    pub arguments_json: String,
}

/// Complete record of a tool execution for session history / debugging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionRecord {
    /// ID of the tool call.
    pub tool_call_id: String,
    /// Name of the tool.
    pub name: String,
    /// Arguments passed to the tool.
    pub args: serde_json::Value,
    /// Output produced by the tool.
    pub result: serde_json::Value,
    /// Whether the tool execution failed.
    pub is_error: bool,
    /// Wall-clock duration of execution in milliseconds.
    pub duration_ms: u64,
}

// ---------------------------------------------------------------------------
// Structs -- Model Management
// ---------------------------------------------------------------------------

/// Metadata about a language model in the local catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Unique identifier for this model.
    pub id: String,
    /// Model family (e.g. Ministral).
    pub family: ModelFamily,
    /// Human-readable model name.
    pub name: String,
    /// Filename of the model weights on disk.
    pub filename: Option<String>,
    /// Size of the model file in bytes.
    pub size_bytes: u64,
    /// Quantization format (e.g. "Q4_K_M").
    pub quantization: String,
    /// URL to download the model weights.
    pub url: Option<String>,
    /// Expected SHA-256 hash of the model file.
    pub sha256: Option<String>,
    /// Backend used to serve this model.
    #[serde(default)]
    pub backend: ModelBackend,
    /// Current download / load status.
    pub status: ModelStatus,
}

/// Specification of a chat model's capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatModelSpec {
    /// Identifier of the model this spec describes.
    pub model_id: String,
    /// Family the model belongs to (drives any per-family behavior, e.g. UI
    /// hardware-requirement labels).
    pub family: ModelFamily,
    /// Maximum number of tokens the model can process.
    pub context_window: u32,
    /// Default sampling temperature.
    pub default_temperature: f32,
}

/// Event payload emitted during model download progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadEvent {
    /// Identifier of the model being downloaded.
    pub model_id: String,
    /// Bytes downloaded so far.
    pub bytes_downloaded: u64,
    /// Total bytes to download.
    pub bytes_total: u64,
    /// Current download speed in bytes per second.
    pub speed_bps: u64,
}

// ---------------------------------------------------------------------------
// Structs -- Agent Session
// ---------------------------------------------------------------------------

/// State of a local agent conversation session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    /// Unique session identifier.
    pub id: String,
    /// Identifier of the model used for this session, if any.
    pub model_id: Option<String>,
    /// Ordered list of messages in this session.
    pub messages: Vec<ChatMessage>,
    /// Current status of the local agent.
    pub status: LocalAgentStatus,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// Record of tool executions during this session.
    pub tool_executions: Vec<ToolExecutionRecord>,
    /// Cached dynamic context string (workspace schemas, collections, playbooks).
    /// Built once per session on first turn, then reused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dynamic_context: Option<String>,
    /// Full system prompt override (bypasses PromptAssembler / fallback).
    /// Test-only: integration tests inject a pre-built prompt without a live
    /// database. Gated by the `testing` feature so the field does not exist
    /// in production builds and never reaches the Tauri serialization layer.
    #[cfg(any(test, feature = "testing"))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt_override: Option<String>,
}

/// Result of a complete agent turn (one round of generation + tool execution).
///
/// Captures the final assistant response text, any tool calls that were made
/// and executed, and token usage for the turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTurnResult {
    /// The final text response produced by the agent (after all tool calls).
    pub response: String,
    /// Tool calls that were made and executed during this turn.
    pub tool_calls_made: Vec<ToolExecutionRecord>,
    /// Token usage statistics for this turn.
    pub usage: InferenceUsage,
}

// ---------------------------------------------------------------------------
// Traits
// ---------------------------------------------------------------------------

/// Engine for running chat inference against a loaded language model.
///
/// Implementors manage model state and produce streaming or complete responses.
#[async_trait]
pub trait ChatInferenceEngine: Send + Sync {
    /// Run streaming inference on the given request.
    async fn generate(
        &self,
        request: InferenceRequest,
        on_chunk: Box<dyn Fn(StreamingChunk) + Send>,
    ) -> Result<InferenceUsage, InferenceError>;

    /// Return metadata about the currently loaded model.
    async fn model_info(&self) -> Result<Option<ChatModelSpec>, InferenceError>;

    /// Estimate the token count for the given text.
    async fn token_count(&self, text: &str) -> Result<u32, InferenceError>;
}

/// Manager for the local model catalog: download, verify, load, and unload.
#[async_trait]
pub trait ModelManager: Send + Sync {
    /// List all known models in the catalog.
    async fn list(&self) -> Result<Vec<ModelInfo>, ModelError>;

    /// Begin downloading a model by its identifier.
    async fn download(&self, model_id: &str) -> Result<(), ModelError>;

    /// Cancel an in-progress download.
    async fn cancel_download(&self, model_id: &str) -> Result<(), ModelError>;

    /// Delete a downloaded model from disk.
    async fn delete(&self, model_id: &str) -> Result<(), ModelError>;

    /// Load a downloaded model into memory for inference.
    async fn load(&self, model_id: &str) -> Result<(), ModelError>;

    /// Unload the currently loaded model, freeing resources.
    async fn unload(&self) -> Result<(), ModelError>;

    /// Return the identifier of the currently loaded model, if any.
    async fn loaded_model(&self) -> Result<Option<String>, ModelError>;

    /// Return the identifier of the recommended default model.
    async fn recommended_model(&self) -> Result<String, ModelError>;
}

/// Executor for agent tools (function calling).
///
/// Each tool is identified by name and accepts/returns JSON values.
#[async_trait]
pub trait AgentToolExecutor: Send + Sync {
    /// Return definitions of all currently available tools.
    async fn available_tools(&self) -> Result<Vec<ToolDefinition>, ToolError>;

    /// Execute a tool by name with the given JSON arguments.
    async fn execute(&self, name: &str, args: serde_json::Value) -> Result<ToolResult, ToolError>;
}
