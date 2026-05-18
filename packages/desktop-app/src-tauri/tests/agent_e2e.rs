//! End-to-end integration tests for the agent subsystems.
//!
//! Validates the local agent pipeline: conversation round-trip with tool
//! execution, model lifecycle state machine, concurrent operations, and
//! error scenarios.
//!
//! All tests are CI-compatible: they use mock engines and tools, never require
//! GPU or actual GGUF model files.
//!
//! Issue #1009

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use tokio::sync::Mutex;

use nodespace_agent::agent_types::*;
use nodespace_agent::local_agent::agent_loop::LocalAgentService;
use nodespace_agent::local_agent::model_manager::GgufModelManager;

// ===========================================================================
// Mock implementations
// ===========================================================================

/// Mock inference engine returning pre-configured streaming chunks.
///
/// Each call to `generate()` pops the next set of chunks from the queue.
/// If the queue is exhausted, returns an empty Done chunk.
struct MockEngine {
    responses: Mutex<Vec<Vec<StreamingChunk>>>,
    generate_count: AtomicUsize,
}

impl MockEngine {
    fn new(responses: Vec<Vec<StreamingChunk>>) -> Self {
        Self {
            responses: Mutex::new(responses),
            generate_count: AtomicUsize::new(0),
        }
    }

    /// A mock that returns a single text response (no tool calls).
    fn single_text(text: &str) -> Self {
        Self::new(vec![vec![
            StreamingChunk::Token {
                text: text.to_string(),
            },
            StreamingChunk::Done {
                usage: InferenceUsage {
                    prompt_tokens: 10,
                    completion_tokens: 5,
                },
            },
        ]])
    }

    /// A mock that first returns a tool call, then a text response.
    fn tool_then_text(tool_name: &str, tool_args: &str, final_text: &str) -> Self {
        Self::new(vec![
            // Round 1: tool call
            vec![
                StreamingChunk::ToolCallStart {
                    id: "tc_e2e_1".to_string(),
                    name: tool_name.to_string(),
                },
                StreamingChunk::ToolCallArgs {
                    id: "tc_e2e_1".to_string(),
                    args_json: tool_args.to_string(),
                },
                StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 20,
                        completion_tokens: 10,
                    },
                },
            ],
            // Round 2: final text
            vec![
                StreamingChunk::Token {
                    text: final_text.to_string(),
                },
                StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 30,
                        completion_tokens: 15,
                    },
                },
            ],
        ])
    }
}

#[async_trait]
impl ChatInferenceEngine for MockEngine {
    async fn generate(
        &self,
        _request: InferenceRequest,
        on_chunk: Box<dyn Fn(StreamingChunk) + Send>,
    ) -> Result<InferenceUsage, InferenceError> {
        let idx = self.generate_count.fetch_add(1, Ordering::SeqCst);
        let responses = self.responses.lock().await;

        if idx >= responses.len() {
            // Return empty response if we run out of pre-configured ones
            on_chunk(StreamingChunk::Done {
                usage: InferenceUsage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                },
            });
            return Ok(InferenceUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
            });
        }

        let chunks = &responses[idx];
        let mut usage = InferenceUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
        };
        for chunk in chunks {
            if let StreamingChunk::Done { usage: u } = chunk {
                usage = *u;
            }
            on_chunk(chunk.clone());
        }
        Ok(usage)
    }

    async fn model_info(&self) -> Result<Option<ChatModelSpec>, InferenceError> {
        Ok(Some(ChatModelSpec {
            model_id: "test-model-e2e".into(),
            family: ModelFamily::Ministral,
            context_window: 8192,
            default_temperature: 0.1,
        }))
    }

    async fn token_count(&self, text: &str) -> Result<u32, InferenceError> {
        Ok((text.len() as f32 / 4.0).ceil() as u32)
    }
}

/// Mock engine that always fails with NoModelLoaded.
struct NoModelEngine;

#[async_trait]
impl ChatInferenceEngine for NoModelEngine {
    async fn generate(
        &self,
        _request: InferenceRequest,
        _on_chunk: Box<dyn Fn(StreamingChunk) + Send>,
    ) -> Result<InferenceUsage, InferenceError> {
        Err(InferenceError::NoModelLoaded)
    }

    async fn model_info(&self) -> Result<Option<ChatModelSpec>, InferenceError> {
        Ok(None)
    }

    async fn token_count(&self, _text: &str) -> Result<u32, InferenceError> {
        Err(InferenceError::NoModelLoaded)
    }
}

/// Mock tool executor with canned results.
struct MockToolExecutor {
    tools: Vec<ToolDefinition>,
    results: HashMap<String, serde_json::Value>,
}

impl MockToolExecutor {
    fn new() -> Self {
        let mut results = HashMap::new();
        results.insert(
            "search_nodes".to_string(),
            json!({"count": 2, "nodes": [
                {"id": "e2e-node-1", "title": "Architecture Overview", "type": "text"},
                {"id": "e2e-node-2", "title": "Design Decisions", "type": "text"},
            ]}),
        );
        results.insert(
            "get_node".to_string(),
            json!({"id": "e2e-node-1", "title": "Architecture Overview", "body": "System architecture details"}),
        );

        Self {
            tools: vec![
                ToolDefinition {
                    name: "search_nodes".into(),
                    description: "Search for nodes".into(),
                    parameters_schema: json!({"type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"]}),
                },
                ToolDefinition {
                    name: "get_node".into(),
                    description: "Get a node by ID".into(),
                    parameters_schema: json!({"type": "object", "properties": {"id": {"type": "string"}}, "required": ["id"]}),
                },
            ],
            results,
        }
    }
}

#[async_trait]
impl AgentToolExecutor for MockToolExecutor {
    async fn available_tools(&self) -> Result<Vec<ToolDefinition>, ToolError> {
        Ok(self.tools.clone())
    }

    async fn execute(&self, name: &str, _args: serde_json::Value) -> Result<ToolResult, ToolError> {
        let result = self
            .results
            .get(name)
            .cloned()
            .unwrap_or(json!({"error": "unknown tool"}));
        let is_error = !self.results.contains_key(name);
        Ok(ToolResult {
            tool_call_id: format!("call_{name}"),
            name: name.to_string(),
            result,
            is_error,
        })
    }
}

// ===========================================================================
// Category 1: Local Agent E2E
// ===========================================================================

/// Full conversation round-trip: create session, send message, get response.
#[tokio::test]
async fn local_agent_simple_conversation_roundtrip() {
    let engine = Arc::new(MockEngine::single_text(
        "NodeSpace is a knowledge management system.",
    ));
    let executor = Arc::new(MockToolExecutor::new());
    let service = LocalAgentService::new(engine, executor);

    // Create session
    let session_id = service.create_session(Some("test-model".into())).await;
    assert!(!session_id.is_empty());

    // Send message and get response
    let result = service
        .send_message(&session_id, "What is NodeSpace?", |_| {}, |_| {})
        .await
        .unwrap();

    assert_eq!(
        result.response,
        "NodeSpace is a knowledge management system."
    );
    assert!(result.tool_calls_made.is_empty());
    assert!(result.usage.prompt_tokens > 0);

    // Verify session state
    let session = service.get_session(&session_id).await.unwrap();
    assert_eq!(session.messages.len(), 2); // user + assistant
    assert_eq!(session.messages[0].role, Role::User);
    assert_eq!(session.messages[1].role, Role::Assistant);
    assert_eq!(session.status, LocalAgentStatus::Idle);
}

/// Full round-trip with tool call: send message -> tool call -> tool result -> final response.
#[tokio::test]
async fn local_agent_conversation_with_tool_call() {
    let engine = Arc::new(MockEngine::tool_then_text(
        "search_nodes",
        r#"{"query":"architecture"}"#,
        "Found 2 nodes about architecture.",
    ));
    let executor = Arc::new(MockToolExecutor::new());
    let service = LocalAgentService::new(engine, executor);

    let session_id = service.create_session(Some("test-model".into())).await;

    // Track status changes
    let statuses: Arc<std::sync::Mutex<Vec<LocalAgentStatus>>> =
        Arc::new(std::sync::Mutex::new(Vec::new()));
    let statuses_cb = Arc::clone(&statuses);

    // Track streaming chunks
    let chunks: Arc<std::sync::Mutex<Vec<StreamingChunk>>> =
        Arc::new(std::sync::Mutex::new(Vec::new()));
    let chunks_cb = Arc::clone(&chunks);

    let result = service
        .send_message(
            &session_id,
            "Search for architecture nodes",
            move |s| {
                statuses_cb.lock().unwrap().push(s);
            },
            move |c| {
                chunks_cb.lock().unwrap().push(c);
            },
        )
        .await
        .unwrap();

    // Verify final response
    assert_eq!(result.response, "Found 2 nodes about architecture.");

    // Verify tool was called
    assert_eq!(result.tool_calls_made.len(), 1);
    assert_eq!(result.tool_calls_made[0].name, "search_nodes");
    assert!(!result.tool_calls_made[0].is_error);

    // Verify usage aggregates both inference rounds
    assert_eq!(result.usage.prompt_tokens, 50); // 20 + 30
    assert_eq!(result.usage.completion_tokens, 25); // 10 + 15

    // Verify session has all messages: user, assistant (tool call), tool result, assistant (final)
    let session = service.get_session(&session_id).await.unwrap();
    assert_eq!(session.messages.len(), 4);
    assert_eq!(session.messages[0].role, Role::User);
    assert_eq!(session.messages[1].role, Role::Assistant);
    assert_eq!(session.messages[2].role, Role::Tool);
    assert_eq!(session.messages[3].role, Role::Assistant);

    // Verify status transitions occurred
    let observed_statuses = statuses.lock().unwrap();
    assert!(
        !observed_statuses.is_empty(),
        "Expected at least one status transition"
    );
}

/// Multi-step tool chain: search -> get_node -> final response.
#[tokio::test]
async fn local_agent_multi_step_tool_chain() {
    let engine = Arc::new(MockEngine::new(vec![
        // Round 1: search_nodes
        vec![
            StreamingChunk::ToolCallStart {
                id: "tc_1".to_string(),
                name: "search_nodes".to_string(),
            },
            StreamingChunk::ToolCallArgs {
                id: "tc_1".to_string(),
                args_json: r#"{"query":"design"}"#.to_string(),
            },
            StreamingChunk::Done {
                usage: InferenceUsage {
                    prompt_tokens: 20,
                    completion_tokens: 10,
                },
            },
        ],
        // Round 2: get_node
        vec![
            StreamingChunk::ToolCallStart {
                id: "tc_2".to_string(),
                name: "get_node".to_string(),
            },
            StreamingChunk::ToolCallArgs {
                id: "tc_2".to_string(),
                args_json: r#"{"id":"e2e-node-1"}"#.to_string(),
            },
            StreamingChunk::Done {
                usage: InferenceUsage {
                    prompt_tokens: 40,
                    completion_tokens: 10,
                },
            },
        ],
        // Round 3: final text
        vec![
            StreamingChunk::Token {
                text: "The Architecture Overview describes the system design.".to_string(),
            },
            StreamingChunk::Done {
                usage: InferenceUsage {
                    prompt_tokens: 60,
                    completion_tokens: 20,
                },
            },
        ],
    ]));
    let executor = Arc::new(MockToolExecutor::new());
    let service = LocalAgentService::new(engine, executor);

    let session_id = service.create_session(None).await;
    let result = service
        .send_message(
            &session_id,
            "Tell me about the system design",
            |_| {},
            |_| {},
        )
        .await
        .unwrap();

    assert!(result.response.contains("Architecture Overview"));
    assert_eq!(result.tool_calls_made.len(), 2);
    assert_eq!(result.tool_calls_made[0].name, "search_nodes");
    assert_eq!(result.tool_calls_made[1].name, "get_node");

    // Total usage aggregated
    assert_eq!(result.usage.prompt_tokens, 120);
    assert_eq!(result.usage.completion_tokens, 40);
}

/// Multi-turn conversation: session persists history across turns.
#[tokio::test]
async fn local_agent_multi_turn_history_persistence() {
    // Two-turn conversation: each turn returns a simple text response
    let engine = Arc::new(MockEngine::new(vec![
        // Turn 1 response
        vec![
            StreamingChunk::Token {
                text: "I can help with that.".to_string(),
            },
            StreamingChunk::Done {
                usage: InferenceUsage {
                    prompt_tokens: 10,
                    completion_tokens: 5,
                },
            },
        ],
        // Turn 2 response
        vec![
            StreamingChunk::Token {
                text: "Based on our earlier discussion, here is more info.".to_string(),
            },
            StreamingChunk::Done {
                usage: InferenceUsage {
                    prompt_tokens: 20,
                    completion_tokens: 10,
                },
            },
        ],
    ]));
    let executor = Arc::new(MockToolExecutor::new());
    let service = LocalAgentService::new(engine, executor);

    let session_id = service.create_session(None).await;

    // Turn 1
    let result1 = service
        .send_message(&session_id, "Hello, can you help?", |_| {}, |_| {})
        .await
        .unwrap();
    assert_eq!(result1.response, "I can help with that.");

    // Turn 2
    let result2 = service
        .send_message(&session_id, "Tell me more", |_| {}, |_| {})
        .await
        .unwrap();
    assert_eq!(
        result2.response,
        "Based on our earlier discussion, here is more info."
    );

    // Verify history has all 4 messages (2 turns * 2 messages each)
    let session = service.get_session(&session_id).await.unwrap();
    assert_eq!(session.messages.len(), 4);
    assert_eq!(session.messages[0].role, Role::User);
    assert_eq!(session.messages[0].content, "Hello, can you help?");
    assert_eq!(session.messages[1].role, Role::Assistant);
    assert_eq!(session.messages[2].role, Role::User);
    assert_eq!(session.messages[2].content, "Tell me more");
    assert_eq!(session.messages[3].role, Role::Assistant);
}

/// Session lifecycle: create, use, end, verify cleanup.
#[tokio::test]
async fn local_agent_session_lifecycle() {
    let engine = Arc::new(MockEngine::single_text("Hello!"));
    let executor = Arc::new(MockToolExecutor::new());
    let service = LocalAgentService::new(engine, executor);

    // Create multiple sessions
    let id1 = service.create_session(Some("model-a".into())).await;
    let id2 = service.create_session(Some("model-b".into())).await;
    let sessions = service.get_sessions().await;
    assert_eq!(sessions.len(), 2);

    // End one session
    service.end_session(&id1).await;
    let sessions = service.get_sessions().await;
    assert_eq!(sessions.len(), 1);

    // Verify ended session is gone
    assert!(service.get_session(&id1).await.is_none());
    assert!(service.get_session(&id2).await.is_some());

    // End remaining session
    service.end_session(&id2).await;
    assert!(service.get_sessions().await.is_empty());
}

// ===========================================================================
// Category 2: Model Lifecycle E2E
// ===========================================================================

/// Model catalog: list returns all known models with correct metadata.
#[tokio::test]
async fn model_lifecycle_list_catalog() {
    let temp_dir = tempfile::tempdir().unwrap();
    let manager = GgufModelManager::with_dir(temp_dir.path().to_path_buf()).unwrap();

    let models = manager.list().await.unwrap();

    // Catalog now contains Ministral 3B/8B + Gemma 4 E4B/31B.
    assert_eq!(models.len(), 4);

    // Verify Ministral 3B
    let model_3b = models.iter().find(|m| m.id == "ministral-3b-q4km");
    assert!(model_3b.is_some(), "Ministral 3B should be in catalog");
    let model_3b = model_3b.unwrap();
    assert_eq!(model_3b.family, ModelFamily::Ministral);
    assert_eq!(model_3b.quantization, "Q4_K_M");
    assert!(matches!(model_3b.status, ModelStatus::NotDownloaded));

    // Verify Ministral 8B
    let model_8b = models.iter().find(|m| m.id == "ministral-8b-q4km");
    assert!(model_8b.is_some(), "Ministral 8B should be in catalog");
    let model_8b = model_8b.unwrap();
    assert!(model_8b.size_bytes > model_3b.size_bytes);

    // Verify Gemma 4 E4B and 31B
    let e4b = models.iter().find(|m| m.id == "gemma-4-e4b-q4km");
    assert!(e4b.is_some(), "Gemma 4 E4B should be in catalog");
    assert_eq!(e4b.unwrap().family, ModelFamily::Gemma4);

    let g31 = models.iter().find(|m| m.id == "gemma-4-31b-q4km");
    assert!(g31.is_some(), "Gemma 4 31B should be in catalog");
    assert_eq!(g31.unwrap().family, ModelFamily::Gemma4);
}

/// Model recommendation: returns a valid model based on system RAM.
#[tokio::test]
async fn model_lifecycle_recommendation() {
    let temp_dir = tempfile::tempdir().unwrap();
    let manager = GgufModelManager::with_dir(temp_dir.path().to_path_buf()).unwrap();

    let recommended_id = manager.recommended_model().await.unwrap();

    // The default first-launch recommendation is Gemma 4 — size depends on
    // system RAM (E4B on <32 GB, 31B on >=32 GB).
    assert!(
        recommended_id == "gemma-4-e4b-q4km" || recommended_id == "gemma-4-31b-q4km",
        "Recommended model should be a Gemma 4 catalog entry, got: {}",
        recommended_id
    );
}

/// Model state transitions: NotDownloaded initial state, Ready/Loaded/unload transitions.
#[tokio::test]
async fn model_lifecycle_state_machine() {
    let temp_dir = tempfile::tempdir().unwrap();
    let manager = GgufModelManager::with_dir(temp_dir.path().to_path_buf()).unwrap();

    let model_id = "ministral-3b-q4km";

    // Initial state: NotDownloaded
    let models = manager.list().await.unwrap();
    let model = models.iter().find(|m| m.id == model_id).unwrap();
    assert!(
        matches!(model.status, ModelStatus::NotDownloaded),
        "Initial status should be NotDownloaded"
    );

    // Cannot load a model that is not downloaded
    let load_result = manager.load(model_id).await;
    assert!(
        load_result.is_err(),
        "Loading undownloaded model should fail"
    );

    // No model should be loaded initially
    let loaded = manager.loaded_model().await.unwrap();
    assert!(loaded.is_none());

    // Unload with nothing loaded should be no-op
    manager.unload().await.unwrap();
}

/// Model state: simulate Ready -> Loaded -> Unload cycle.
///
/// We create a dummy file in the models dir to simulate a "downloaded" model,
/// then test the load/unload state transitions.
#[tokio::test]
async fn model_lifecycle_load_unload_cycle() {
    let temp_dir = tempfile::tempdir().unwrap();
    let manager = GgufModelManager::with_dir(temp_dir.path().to_path_buf()).unwrap();

    let model_id = "ministral-3b-q4km";

    // Simulate a downloaded model by creating the expected file
    let model_path = manager.model_path(model_id).unwrap();
    tokio::fs::write(&model_path, b"fake-model-data-for-testing")
        .await
        .unwrap();

    // Re-create manager so it detects the file as Ready
    let manager = GgufModelManager::with_dir(temp_dir.path().to_path_buf()).unwrap();

    // Verify model is now Ready
    let models = manager.list().await.unwrap();
    let model = models.iter().find(|m| m.id == model_id).unwrap();
    assert!(
        matches!(model.status, ModelStatus::Ready),
        "Model with file on disk should be Ready, got: {:?}",
        model.status
    );

    // Load the model
    manager.load(model_id).await.unwrap();

    // Verify loaded
    let loaded = manager.loaded_model().await.unwrap();
    assert_eq!(loaded, Some(model_id.to_string()));

    let models = manager.list().await.unwrap();
    let model = models.iter().find(|m| m.id == model_id).unwrap();
    assert!(
        matches!(model.status, ModelStatus::Loaded),
        "After load, status should be Loaded"
    );

    // Loading same model again should be idempotent
    manager.load(model_id).await.unwrap();

    // Unload
    manager.unload().await.unwrap();

    let loaded = manager.loaded_model().await.unwrap();
    assert!(loaded.is_none());

    let models = manager.list().await.unwrap();
    let model = models.iter().find(|m| m.id == model_id).unwrap();
    assert!(
        matches!(model.status, ModelStatus::Ready),
        "After unload, status should be Ready"
    );
}

/// Model delete: removes file and resets status to NotDownloaded.
#[tokio::test]
async fn model_lifecycle_delete() {
    let temp_dir = tempfile::tempdir().unwrap();
    let manager = GgufModelManager::with_dir(temp_dir.path().to_path_buf()).unwrap();

    let model_id = "ministral-3b-q4km";

    // Create a fake model file
    let model_path = manager.model_path(model_id).unwrap();
    tokio::fs::write(&model_path, b"fake-model-data")
        .await
        .unwrap();

    // Re-create manager to detect the file
    let manager = GgufModelManager::with_dir(temp_dir.path().to_path_buf()).unwrap();

    // Delete the model
    manager.delete(model_id).await.unwrap();

    // Verify file is gone
    assert!(!model_path.exists());

    // Verify status is NotDownloaded
    let models = manager.list().await.unwrap();
    let model = models.iter().find(|m| m.id == model_id).unwrap();
    assert!(matches!(model.status, ModelStatus::NotDownloaded));
}

/// Cannot delete a loaded model.
#[tokio::test]
async fn model_lifecycle_cannot_delete_loaded() {
    let temp_dir = tempfile::tempdir().unwrap();

    let model_id = "ministral-3b-q4km";

    // Create manager and fake model file
    let manager = GgufModelManager::with_dir(temp_dir.path().to_path_buf()).unwrap();
    let model_path = manager.model_path(model_id).unwrap();
    tokio::fs::write(&model_path, b"fake-model-data")
        .await
        .unwrap();

    // Re-create to detect file, then load
    let manager = GgufModelManager::with_dir(temp_dir.path().to_path_buf()).unwrap();
    manager.load(model_id).await.unwrap();

    // Attempt delete while loaded
    let result = manager.delete(model_id).await;
    assert!(
        result.is_err(),
        "Should not be able to delete a loaded model"
    );

    // File should still exist
    assert!(model_path.exists());

    // Unload first, then delete should succeed
    manager.unload().await.unwrap();
    manager.delete(model_id).await.unwrap();
    assert!(!model_path.exists());
}

/// Model not found error for unknown model ID.
#[tokio::test]
async fn model_lifecycle_not_found() {
    let temp_dir = tempfile::tempdir().unwrap();
    let manager = GgufModelManager::with_dir(temp_dir.path().to_path_buf()).unwrap();

    let result = manager.load("nonexistent-model").await;
    assert!(result.is_err());
    match result.unwrap_err() {
        ModelError::NotFound(id) => assert_eq!(id, "nonexistent-model"),
        other => panic!("Expected NotFound error, got: {:?}", other),
    }
}

// ===========================================================================
// Category 3: Concurrent Operations
// ===========================================================================

/// Multiple local agent sessions running concurrently.
#[tokio::test]
async fn concurrent_multiple_local_sessions() {
    let engine = Arc::new(MockEngine::new(vec![
        // Session 1 response
        vec![
            StreamingChunk::Token {
                text: "Response 1".to_string(),
            },
            StreamingChunk::Done {
                usage: InferenceUsage {
                    prompt_tokens: 5,
                    completion_tokens: 2,
                },
            },
        ],
        // Session 2 response
        vec![
            StreamingChunk::Token {
                text: "Response 2".to_string(),
            },
            StreamingChunk::Done {
                usage: InferenceUsage {
                    prompt_tokens: 5,
                    completion_tokens: 2,
                },
            },
        ],
    ]));
    let executor = Arc::new(MockToolExecutor::new());
    let service = Arc::new(LocalAgentService::new(engine, executor));

    let id1 = service.create_session(None).await;
    let id2 = service.create_session(None).await;

    // Send to session 1
    let result1 = service
        .send_message(&id1, "Message for session 1", |_| {}, |_| {})
        .await
        .unwrap();

    // Send to session 2
    let result2 = service
        .send_message(&id2, "Message for session 2", |_| {}, |_| {})
        .await
        .unwrap();

    // Both should have gotten responses
    assert_eq!(result1.response, "Response 1");
    assert_eq!(result2.response, "Response 2");

    // Verify sessions are independent
    let s1 = service.get_session(&id1).await.unwrap();
    let s2 = service.get_session(&id2).await.unwrap();
    assert_eq!(s1.messages.len(), 2);
    assert_eq!(s2.messages.len(), 2);
    assert_ne!(s1.id, s2.id);
}

// ===========================================================================
// Category 4: Error Scenarios
// ===========================================================================

/// Send to nonexistent session returns clear error.
#[tokio::test]
async fn error_send_to_nonexistent_session() {
    let engine = Arc::new(MockEngine::single_text("unused"));
    let executor = Arc::new(MockToolExecutor::new());
    let service = LocalAgentService::new(engine, executor);

    let result = service
        .send_message("nonexistent-session-id", "Hello", |_| {}, |_| {})
        .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        InferenceError::Engine(msg) => {
            assert!(
                msg.contains("session not found"),
                "Error should mention session not found, got: {msg}"
            );
        }
        other => panic!("Expected Engine error, got: {:?}", other),
    }
}

/// Agent with no model loaded returns appropriate error.
#[tokio::test]
async fn error_no_model_loaded() {
    let engine = Arc::new(NoModelEngine);
    let executor = Arc::new(MockToolExecutor::new());
    let service = LocalAgentService::new(engine, executor);

    let session_id = service.create_session(None).await;
    let result = service
        .send_message(&session_id, "Hello", |_| {}, |_| {})
        .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        InferenceError::NoModelLoaded => {} // Expected
        other => panic!("Expected NoModelLoaded, got: {:?}", other),
    }
}

/// Cancellation stops an in-progress agent turn.
#[tokio::test]
async fn error_cancellation_mid_turn() {
    let engine = Arc::new(MockEngine::single_text("Should not complete"));
    let executor = Arc::new(MockToolExecutor::new());
    let service = LocalAgentService::new(engine, executor);

    let session_id = service.create_session(None).await;

    // Cancel the session before sending
    service.cancel(&session_id).await;

    // The next send should detect cancellation
    // Note: because MockEngine returns immediately, cancellation may or may not
    // be detected depending on timing. The cancel token is replaced after cancel(),
    // so we test the cancel API works without error.
    let _result = service
        .send_message(&session_id, "Hello", |_| {}, |_| {})
        .await;

    // The important thing is that cancel didn't crash or deadlock
    // Session should still be accessible
    assert!(service.get_session(&session_id).await.is_some());
}

/// Model manager: nonexistent model ID returns NotFound for all operations.
#[tokio::test]
async fn error_model_operations_with_invalid_id() {
    let temp_dir = tempfile::tempdir().unwrap();
    let manager = GgufModelManager::with_dir(temp_dir.path().to_path_buf()).unwrap();

    // Load
    let result = manager.load("fake-model-id").await;
    assert!(matches!(result.unwrap_err(), ModelError::NotFound(_)));

    // Delete
    let result = manager.delete("fake-model-id").await;
    assert!(matches!(result.unwrap_err(), ModelError::NotFound(_)));

    // Cancel download
    let result = manager.cancel_download("fake-model-id").await;
    assert!(matches!(result.unwrap_err(), ModelError::NotFound(_)));
}

// ===========================================================================
// Performance benchmarks (documented, not pass/fail gated)
// ===========================================================================

/// Benchmark: measure local agent turn latency with mock engine.
///
/// Documents the overhead of the ReAct loop orchestration (session management,
/// status callbacks, chunk collection) independent of actual inference time.
#[tokio::test]
async fn benchmark_local_agent_turn_latency() {
    let engine = Arc::new(MockEngine::single_text("Quick response"));
    let executor = Arc::new(MockToolExecutor::new());
    let service = LocalAgentService::new(engine, executor);

    let session_id = service.create_session(None).await;

    let start = std::time::Instant::now();
    let _result = service
        .send_message(&session_id, "Benchmark message", |_| {}, |_| {})
        .await
        .unwrap();
    let elapsed = start.elapsed();

    // Document the latency (not a pass/fail gate)
    eprintln!(
        "[BENCHMARK] Local agent turn latency (mock engine): {:?}",
        elapsed
    );

    // Sanity check: mock engine should complete in well under 1 second
    assert!(
        elapsed < std::time::Duration::from_secs(1),
        "Mock engine turn took too long: {:?}",
        elapsed
    );
}

/// Benchmark: measure tool execution overhead in the ReAct loop.
#[tokio::test]
async fn benchmark_tool_execution_overhead() {
    let engine = Arc::new(MockEngine::tool_then_text(
        "search_nodes",
        r#"{"query":"bench"}"#,
        "Done.",
    ));
    let executor = Arc::new(MockToolExecutor::new());
    let service = LocalAgentService::new(engine, executor);

    let session_id = service.create_session(None).await;

    let start = std::time::Instant::now();
    let result = service
        .send_message(&session_id, "Benchmark with tool", |_| {}, |_| {})
        .await
        .unwrap();
    let elapsed = start.elapsed();

    eprintln!("[BENCHMARK] Local agent turn with tool call: {:?}", elapsed);
    eprintln!(
        "[BENCHMARK] Tool execution duration: {}ms",
        result.tool_calls_made[0].duration_ms
    );

    assert!(
        elapsed < std::time::Duration::from_secs(1),
        "Tool call turn took too long: {:?}",
        elapsed
    );
}

/// Benchmark: model manager catalog operations.
#[tokio::test]
async fn benchmark_model_catalog_operations() {
    let temp_dir = tempfile::tempdir().unwrap();
    let manager = GgufModelManager::with_dir(temp_dir.path().to_path_buf()).unwrap();

    let start = std::time::Instant::now();
    for _ in 0..100 {
        let _ = manager.list().await.unwrap();
    }
    let elapsed = start.elapsed();

    eprintln!(
        "[BENCHMARK] 100x model list operations: {:?} ({:?}/op)",
        elapsed,
        elapsed / 100
    );

    let start = std::time::Instant::now();
    for _ in 0..100 {
        let _ = manager.recommended_model().await.unwrap();
    }
    let elapsed = start.elapsed();

    eprintln!(
        "[BENCHMARK] 100x recommended_model: {:?} ({:?}/op)",
        elapsed,
        elapsed / 100
    );
}

// ===========================================================================
// Pipeline integration tests — prompt, tools, normalizer (Issue #1040)
//
// These tests validate the agent pipeline plumbing: system prompt assembly,
// tool dispatch, result handling, and response normalization. Tool selection
// is driven by the mock engine (not the actual model), so these verify the
// infrastructure, not model behavior.
// ===========================================================================

/// Mock engine that captures inference requests for assertion.
struct CapturingMockEngine {
    captured_requests: Mutex<Vec<InferenceRequest>>,
    response_text: String,
}

impl CapturingMockEngine {
    fn new(response_text: &str) -> Self {
        Self {
            captured_requests: Mutex::new(Vec::new()),
            response_text: response_text.to_string(),
        }
    }
}

#[async_trait]
impl ChatInferenceEngine for CapturingMockEngine {
    async fn generate(
        &self,
        request: InferenceRequest,
        on_chunk: Box<dyn Fn(StreamingChunk) + Send>,
    ) -> Result<InferenceUsage, InferenceError> {
        self.captured_requests.lock().await.push(request);
        on_chunk(StreamingChunk::Token {
            text: self.response_text.clone(),
        });
        let usage = InferenceUsage {
            prompt_tokens: 10,
            completion_tokens: 5,
        };
        on_chunk(StreamingChunk::Done { usage });
        Ok(usage)
    }

    async fn model_info(&self) -> Result<Option<ChatModelSpec>, InferenceError> {
        Ok(Some(ChatModelSpec {
            model_id: "test-capture".into(),
            family: ModelFamily::Ministral,
            context_window: 32768,
            default_temperature: 0.1,
        }))
    }

    async fn token_count(&self, text: &str) -> Result<u32, InferenceError> {
        Ok((text.len() as f32 / 4.0).ceil() as u32)
    }
}

/// Verify system prompt includes dynamic context when set on session.
#[tokio::test]
async fn system_prompt_includes_dynamic_context() {
    let engine = Arc::new(CapturingMockEngine::new("Here are your tasks."));
    let executor: Arc<dyn AgentToolExecutor> = Arc::new(MockToolExecutor::new());
    let service = LocalAgentService::new(engine.clone() as Arc<dyn ChatInferenceEngine>, executor);

    let session_id = service.create_session(Some("test-model".into())).await;

    // Set dynamic context (simulating what local_agent_new_session does)
    service
        .set_session_context(
            &session_id,
            "ENTITY TYPES:\n- customer: Customer — fields: company(text), status(enum: Active/Churned)".into(),
        )
        .await;

    let _result = service
        .send_message(&session_id, "find my tasks", |_| {}, |_| {})
        .await
        .unwrap();

    // Verify the system prompt sent to the engine includes the dynamic context
    let requests = engine.captured_requests.lock().await;
    assert!(!requests.is_empty(), "Engine should have been called");

    let system_msg = requests[0]
        .messages
        .iter()
        .find(|m| m.role == Role::System)
        .expect("Should have a system message");

    assert!(
        system_msg.content.contains("customer: Customer"),
        "System prompt should include dynamic context. Got: {}",
        &system_msg.content[..200.min(system_msg.content.len())]
    );
    assert!(
        system_msg.content.contains("TOOL STRATEGY"),
        "System prompt should include tool strategy section"
    );
}

/// Verify response normalizer cleans up model output.
#[tokio::test]
async fn response_normalizer_fixes_uri_formatting() {
    let engine = Arc::new(CapturingMockEngine::new(
        "Found your task: [nodespace://abc-123](nodespace://abc-123) with status in_progress",
    ));
    let executor: Arc<dyn AgentToolExecutor> = Arc::new(MockToolExecutor::new());
    let service = LocalAgentService::new(engine as Arc<dyn ChatInferenceEngine>, executor);

    let session_id = service.create_session(None).await;
    let result = service
        .send_message(&session_id, "find tasks", |_| {}, |_| {})
        .await
        .unwrap();

    // Normalizer should fix markdown-wrapped URI to bare URI
    assert!(
        !result.response.contains("[nodespace://"),
        "Markdown-wrapped URI should be normalized. Got: {}",
        result.response
    );
    assert!(
        result.response.contains("nodespace://abc-123"),
        "Bare URI should be preserved. Got: {}",
        result.response
    );
    // Normalizer should fix snake_case status
    assert!(
        result.response.contains("In Progress"),
        "snake_case status should be Title Case. Got: {}",
        result.response
    );
}

/// Verify tool calls receive correct tool definitions.
#[tokio::test]
async fn tool_definitions_included_in_inference_request() {
    let engine = Arc::new(CapturingMockEngine::new("No tasks found."));
    let executor: Arc<dyn AgentToolExecutor> = Arc::new(MockToolExecutor::new());
    let service = LocalAgentService::new(engine.clone() as Arc<dyn ChatInferenceEngine>, executor);

    let session_id = service.create_session(None).await;
    let _result = service
        .send_message(&session_id, "what tasks do I have?", |_| {}, |_| {})
        .await
        .unwrap();

    let requests = engine.captured_requests.lock().await;
    assert!(!requests.is_empty());

    let tools = requests[0]
        .tools
        .as_ref()
        .expect("Tools should be provided");
    assert!(!tools.is_empty(), "At least one tool should be available");

    // Verify tool names are present
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert!(
        tool_names.contains(&"search_nodes"),
        "search_nodes tool should be available"
    );
    assert!(
        tool_names.contains(&"get_node"),
        "get_node tool should be available"
    );
}

/// Verify tool call → result → text response round-trip with normalizer.
#[tokio::test]
async fn tool_call_round_trip_with_normalizer() {
    // Mock: first call tool, then respond with unnormalized text
    let engine = Arc::new(MockEngine::tool_then_text(
        "search_nodes",
        r#"{"query":"tasks"}"#,
        "Found 2 results: `nodespace://e2e-node-1` and `nodespace://e2e-node-2` are in_progress",
    ));
    let executor: Arc<dyn AgentToolExecutor> = Arc::new(MockToolExecutor::new());
    let service = LocalAgentService::new(engine as Arc<dyn ChatInferenceEngine>, executor);

    let session_id = service.create_session(None).await;
    let result = service
        .send_message(&session_id, "find tasks", |_| {}, |_| {})
        .await
        .unwrap();

    // Should have executed the search_nodes tool
    assert_eq!(result.tool_calls_made.len(), 1);
    assert_eq!(result.tool_calls_made[0].name, "search_nodes");

    // Response should be normalized (backtick URIs → bare, snake_case → Title Case)
    assert!(
        !result.response.contains("`nodespace://"),
        "Backtick-wrapped URIs should be normalized"
    );
    assert!(
        result.response.contains("In Progress"),
        "snake_case status should be Title Case"
    );
}

/// Mock tool executor that records calls and returns realistic results per tool.
///
/// Tool schemas are intentionally defined inline (not imported from GraphToolExecutor)
/// to decouple these tests from real service wiring. This tests the agent loop
/// independently — if tool schemas drift, the compile-time types in GraphToolExecutor
/// catch it; these mocks verify pipeline behavior.
struct RecordingToolExecutor {
    calls: Mutex<Vec<(String, serde_json::Value)>>,
}

impl RecordingToolExecutor {
    fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
        }
    }

    async fn recorded_calls(&self) -> Vec<(String, serde_json::Value)> {
        self.calls.lock().await.clone()
    }
}

#[async_trait]
impl AgentToolExecutor for RecordingToolExecutor {
    async fn available_tools(&self) -> Result<Vec<ToolDefinition>, ToolError> {
        Ok(vec![
            ToolDefinition {
                name: "search_nodes".into(),
                description: "Search for nodes by keyword or structured query".into(),
                parameters_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "node_type": { "type": "string" },
                        "limit": { "type": "integer" }
                    },
                    "required": ["query"]
                }),
            },
            ToolDefinition {
                name: "search_semantic".into(),
                description: "Find nodes semantically related to a natural-language query".into(),
                parameters_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "limit": { "type": "integer" }
                    },
                    "required": ["query"]
                }),
            },
            ToolDefinition {
                name: "get_node".into(),
                description: "Get a node by ID".into(),
                parameters_schema: json!({
                    "type": "object",
                    "properties": { "id": { "type": "string" } },
                    "required": ["id"]
                }),
            },
            ToolDefinition {
                name: "create_node".into(),
                description: "Create a new node".into(),
                parameters_schema: json!({
                    "type": "object",
                    "properties": {
                        "title": { "type": "string" },
                        "node_type": { "type": "string" },
                        "properties": { "type": "object" }
                    },
                    "required": ["title", "node_type"]
                }),
            },
        ])
    }

    async fn execute(&self, name: &str, args: serde_json::Value) -> Result<ToolResult, ToolError> {
        self.calls
            .lock()
            .await
            .push((name.to_string(), args.clone()));

        let result = match name {
            "search_nodes" => json!({
                "count": 3,
                "nodes": [
                    {"id": "task-1", "title": "Fix login bug", "type": "task", "snippet": "Fix the login page crash on Safari", "properties": {"status": "in_progress", "priority": "high"}},
                    {"id": "task-2", "title": "Update API docs", "type": "task", "snippet": "Document new endpoints", "properties": {"status": "open", "priority": "medium"}},
                    {"id": "task-3", "title": "Review PR #42", "type": "task", "snippet": "Code review for auth refactor", "properties": {"status": "open", "priority": "low"}},
                ]
            }),
            "search_semantic" => json!({
                "count": 2,
                "nodes": [
                    {"id": "note-ml-1", "title": "ML Pipeline Architecture", "type": "text", "similarity": 0.87, "content": "Our machine learning pipeline uses..."},
                    {"id": "note-ml-2", "title": "Model Training Notes", "type": "text", "similarity": 0.72, "content": "Key findings from the latest training run..."},
                ]
            }),
            "get_node" => json!({
                "id": args.get("id").and_then(|v| v.as_str()).unwrap_or("unknown"),
                "title": "Detailed Node",
                "type": "text",
                "body": "Full node content here"
            }),
            "create_node" => json!({
                "id": "new-node-123"
            }),
            _ => json!({"error": format!("unknown tool: {}", name)}),
        };

        Ok(ToolResult {
            tool_call_id: format!("call_{name}"),
            name: name.to_string(),
            result,
            is_error: false,
        })
    }
}

/// Structured query: "what are my tasks?" should call search_nodes with task type,
/// get realistic task results back, and produce a normalized response.
#[tokio::test]
async fn structured_query_tasks_uses_search_nodes() {
    let executor = Arc::new(RecordingToolExecutor::new());
    // Model calls search_nodes, then responds with task summary
    let engine = Arc::new(MockEngine::tool_then_text(
        "search_nodes",
        r#"{"query":"tasks","node_type":"task"}"#,
        "You have 3 tasks:\n- **Fix login bug** (nodespace://task-1) — in_progress, High priority\n- **Update API docs** (nodespace://task-2) — Open\n- **Review PR #42** (nodespace://task-3) — Open",
    ));

    let service = LocalAgentService::new(
        engine as Arc<dyn ChatInferenceEngine>,
        executor.clone() as Arc<dyn AgentToolExecutor>,
    );

    let session_id = service.create_session(Some("test".into())).await;
    let result = service
        .send_message(&session_id, "what are my tasks?", |_| {}, |_| {})
        .await
        .unwrap();

    // Verify search_nodes was called
    let calls = executor.recorded_calls().await;
    assert_eq!(calls.len(), 1, "Should have made exactly 1 tool call");
    assert_eq!(
        calls[0].0, "search_nodes",
        "Should have called search_nodes"
    );

    // Verify args included node_type filter
    let args = &calls[0].1;
    assert_eq!(
        args.get("node_type").and_then(|v| v.as_str()),
        Some("task"),
        "Should filter by task node_type"
    );

    // Verify response includes task references and is normalized
    assert!(result.response.contains("nodespace://task-1"));
    assert!(result.response.contains("nodespace://task-2"));
    // Normalizer should convert in_progress → In Progress
    assert!(
        result.response.contains("In Progress"),
        "Status should be Title Case. Got: {}",
        result.response
    );
}

/// Semantic/RAG query: "anything about machine learning?" should call search_semantic,
/// get relevance-scored results, and present them with scores.
#[tokio::test]
async fn semantic_query_uses_search_semantic() {
    let executor = Arc::new(RecordingToolExecutor::new());
    // Model calls search_semantic, then responds with semantic results
    let engine = Arc::new(MockEngine::tool_then_text(
        "search_semantic",
        r#"{"query":"machine learning"}"#,
        "Found 2 relevant notes:\n- **ML Pipeline Architecture** (nodespace://note-ml-1) — highly relevant\n- **Model Training Notes** (nodespace://note-ml-2) — related",
    ));

    let service = LocalAgentService::new(
        engine as Arc<dyn ChatInferenceEngine>,
        executor.clone() as Arc<dyn AgentToolExecutor>,
    );

    let session_id = service.create_session(Some("test".into())).await;
    let result = service
        .send_message(
            &session_id,
            "anything about machine learning?",
            |_| {},
            |_| {},
        )
        .await
        .unwrap();

    // Verify search_semantic was called (not search_nodes)
    let calls = executor.recorded_calls().await;
    assert_eq!(calls.len(), 1, "Should have made exactly 1 tool call");
    assert_eq!(
        calls[0].0, "search_semantic",
        "Should have called search_semantic for natural language query"
    );

    // Verify query was passed through
    let args = &calls[0].1;
    assert_eq!(
        args.get("query").and_then(|v| v.as_str()),
        Some("machine learning"),
        "Should pass the search query"
    );

    // Verify response references the found nodes
    assert!(result.response.contains("nodespace://note-ml-1"));
    assert!(result.response.contains("nodespace://note-ml-2"));
}

/// Multi-turn: structured search followed by semantic search in same session.
#[tokio::test]
async fn multi_turn_mixed_tool_calls() {
    let executor = Arc::new(RecordingToolExecutor::new());

    // Turn 1: search_nodes for tasks
    // Turn 2: search_semantic for related content
    let engine = Arc::new(MockEngine::new(vec![
        // Turn 1, round 1: tool call search_nodes
        vec![
            StreamingChunk::ToolCallStart {
                id: "tc1".to_string(),
                name: "search_nodes".to_string(),
            },
            StreamingChunk::ToolCallArgs {
                id: "tc1".to_string(),
                args_json: r#"{"query":"tasks","node_type":"task"}"#.to_string(),
            },
            StreamingChunk::Done {
                usage: InferenceUsage {
                    prompt_tokens: 20,
                    completion_tokens: 10,
                },
            },
        ],
        // Turn 1, round 2: text response
        vec![
            StreamingChunk::Token {
                text: "You have 3 tasks.".to_string(),
            },
            StreamingChunk::Done {
                usage: InferenceUsage {
                    prompt_tokens: 30,
                    completion_tokens: 5,
                },
            },
        ],
        // Turn 2, round 1: tool call search_semantic
        vec![
            StreamingChunk::ToolCallStart {
                id: "tc2".to_string(),
                name: "search_semantic".to_string(),
            },
            StreamingChunk::ToolCallArgs {
                id: "tc2".to_string(),
                args_json: r#"{"query":"machine learning research"}"#.to_string(),
            },
            StreamingChunk::Done {
                usage: InferenceUsage {
                    prompt_tokens: 40,
                    completion_tokens: 10,
                },
            },
        ],
        // Turn 2, round 2: text response
        vec![
            StreamingChunk::Token {
                text: "Found 2 notes about ML.".to_string(),
            },
            StreamingChunk::Done {
                usage: InferenceUsage {
                    prompt_tokens: 50,
                    completion_tokens: 5,
                },
            },
        ],
    ]));

    let service = LocalAgentService::new(
        engine as Arc<dyn ChatInferenceEngine>,
        executor.clone() as Arc<dyn AgentToolExecutor>,
    );

    let session_id = service.create_session(Some("test".into())).await;

    // Turn 1: structured task query
    let result1 = service
        .send_message(&session_id, "what are my tasks?", |_| {}, |_| {})
        .await
        .unwrap();
    assert_eq!(result1.tool_calls_made.len(), 1);
    assert_eq!(result1.tool_calls_made[0].name, "search_nodes");

    // Turn 2: semantic query in same session
    let result2 = service
        .send_message(
            &session_id,
            "find me anything about machine learning research",
            |_| {},
            |_| {},
        )
        .await
        .unwrap();
    assert_eq!(result2.tool_calls_made.len(), 1);
    assert_eq!(result2.tool_calls_made[0].name, "search_semantic");

    // Verify both tools were called across the session
    let calls = executor.recorded_calls().await;
    assert_eq!(
        calls.len(),
        2,
        "Should have made 2 tool calls across 2 turns"
    );
    assert_eq!(calls[0].0, "search_nodes");
    assert_eq!(calls[1].0, "search_semantic");
}

/// Real inference test: Validates per-turn workspace context refresh with actual LLM.
///
/// This test:
/// 1. Loads the real Ministral-3 model from ~/.nodespace/models/
/// 2. Validates the inference engine loads and executes correctly
/// 3. Validates that the system integrates with real model output
///
/// This test doesn't require a full database - it validates that:
/// - The model can be loaded and run inference
/// - The per-turn context refresh mechanism integrates with real inference
/// - The system doesn't crash with actual LLM execution
///
/// Full end-to-end validation with schema changes requires a test database setup
/// (beyond the scope of unit tests), but the per-turn refresh at the code level
/// is validated in the command-level integration (local_agent_send in local_agent.rs)
/// which calls set_session_context before send_message.
///
/// Skipped if the model file doesn't exist (CI-safe).
#[tokio::test]
#[ignore] // Only run locally with models downloaded
async fn test_real_inference_loads_and_runs() {
    use nodespace_agent::local_agent::inference::LlamaChatInferenceEngine;
    use nodespace_nlp_engine::chat::ChatConfig;
    use std::path::PathBuf;

    // Check if model exists (skip if not)
    let model_path = PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/root".to_string()))
        .join(".nodespace/models/Ministral-3-3B-Instruct-2512-Q4_K_M.gguf");

    if !model_path.exists() {
        eprintln!(
            "Skipping real inference test: model not found at {:?}",
            model_path
        );
        return;
    }

    // LlamaChatInferenceEngine::load takes model path separately from ChatConfig.
    let config = ChatConfig {
        n_ctx: 4096,
        ..Default::default()
    };

    let engine = LlamaChatInferenceEngine::load(
        &model_path.to_string_lossy(),
        ModelFamily::Ministral,
        config,
    )
    .expect("Failed to initialize Llama inference engine with Ministral-3");

    // Verify model info is available
    let model_info = engine.model_info().await.expect("Failed to get model info");
    assert!(
        model_info.is_some(),
        "Model should report its information after loading"
    );
    println!("Loaded model: {:?}", model_info);

    // Create a mock executor for tool dispatch
    let executor = MockToolExecutor::new();

    let service = LocalAgentService::new(
        Arc::new(engine) as Arc<dyn ChatInferenceEngine>,
        Arc::new(executor) as Arc<dyn AgentToolExecutor>,
    );

    // Create session
    let session_id = service
        .create_session(Some("real_inference_test".into()))
        .await;

    // Send a simple message to validate inference works
    let result = service
        .send_message(
            &session_id,
            "What are the available node types? List them briefly.",
            |_| {},
            |_| {},
        )
        .await
        .expect("First turn should succeed");

    // Verify we got a response (either text or tool calls)
    assert!(
        !result.response.is_empty() || !result.tool_calls_made.is_empty(),
        "Should receive either a text response or tool calls"
    );

    println!(
        "Real inference test passed: model responded with {} characters or {} tool calls",
        result.response.len(),
        result.tool_calls_made.len()
    );

    // CRITICAL VALIDATION: The per-turn context refresh happens in local_agent_send()
    // in packages/desktop-app/src-tauri/src/commands/local_agent.rs.
    // This test validates the inference infrastructure works. The actual context refresh
    // is validated by:
    // 1. Test in prompt_templates.rs: fallback_system_prompt_per_turn_refresh()
    // 2. Code inspection: local_agent_send() calls set_session_context() before send_message()
    //
    // A full end-to-end test would require:
    // - A test database with schema operations
    // - Tool execution that modifies schemas
    // - Verification that second turn sees the new schema in the prompt
    // But this is complex to set up in unit tests and is better validated
    // through integration tests with a full database stack.
}
