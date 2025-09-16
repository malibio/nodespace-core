# NodeSpace Agent Implementation Guide

## Getting Started

This guide walks through implementing the NodeSpace hybrid agent architecture, from basic local LoRA adapters to advanced cloud integration.

## Prerequisites

### Development Environment

```bash
# Required tools
brew install rust llvm
cargo install --git https://github.com/rustformers/llama-cpp-rs

# Python environment for training (your existing setup)
conda create -n nodespace-training python=3.11
conda activate nodespace-training
pip install mlx-lm torch transformers

# Model storage
mkdir -p ./models/{base,adapters}
```

### Base Model Setup

```rust
// Cargo.toml
[dependencies]
llama-cpp-rs = "0.2"
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
anyhow = "1.0"
tracing = "0.1"
futures = "0.3"

// src/models/base_model.rs
use llama_cpp_rs::{LlamaModel, LlamaContext, LlamaParams};
use std::path::Path;

pub struct BaseModelManager {
    models: HashMap<String, Arc<LlamaModel>>,
}

impl BaseModelManager {
    pub async fn initialize() -> Result<Self> {
        let mut models = HashMap::new();
        
        // Load base models
        if Path::new("./models/base/gemma-3-4b-it-q4_k_m.gguf").exists() {
            let model = Arc::new(LlamaModel::load_from_file(
                "./models/base/gemma-3-4b-it-q4_k_m.gguf",
                LlamaParams::default().with_n_ctx(4096)
            )?);
            models.insert("gemma-3-4b".to_string(), model);
        }
        
        if Path::new("./models/base/gemma-3-12b-it-q4_k_m.gguf").exists() {
            let model = Arc::new(LlamaModel::load_from_file(
                "./models/base/gemma-3-12b-it-q4_k_m.gguf", 
                LlamaParams::default().with_n_ctx(8192)
            )?);
            models.insert("gemma-3-12b".to_string(), model);
        }
        
        Ok(Self { models })
    }
}
```

## Phase 1: Basic Local Agent

### Core Agent Interface

```rust
// src/agents/mod.rs
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRequest {
    pub content: String,
    pub context: AgentContext,
    pub preferences: UserPreferences,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    pub content: String,
    pub confidence: f64,
    pub reasoning: Option<String>,
    pub suggested_actions: Vec<SuggestedAction>,
}

#[async_trait]
pub trait Agent: Send + Sync {
    async fn process(&mut self, request: &AgentRequest) -> Result<AgentResponse>;
    fn name(&self) -> &str;
    fn capabilities(&self) -> Vec<Capability>;
}
```

### Local LoRA Agent Implementation

```rust
// src/agents/local_agent.rs
use crate::models::BaseModelManager;
use llama_cpp_rs::{LlamaContext, LlamaModel};

pub struct LocalLoRAAgent {
    name: String,
    base_model: Arc<LlamaModel>,
    current_adapter: Option<String>,
    context: Option<LlamaContext>,
}

impl LocalLoRAAgent {
    pub async fn new(
        name: String,
        base_model_name: &str,
        model_manager: &BaseModelManager
    ) -> Result<Self> {
        let base_model = model_manager.get_model(base_model_name)
            .ok_or(Error::ModelNotFound(base_model_name.to_string()))?
            .clone();
            
        Ok(Self {
            name,
            base_model,
            current_adapter: None,
            context: None,
        })
    }
    
    pub async fn load_adapter(&mut self, adapter_name: &str) -> Result<()> {
        // Unload current adapter if exists
        if let Some(current) = &self.current_adapter {
            self.base_model.unload_adapter(current)?;
        }
        
        // Load new adapter
        let adapter_path = format!("./models/adapters/{}.gguf", adapter_name);
        self.base_model.load_adapter(&adapter_path, adapter_name)?;
        
        // Create new context with adapter
        self.context = Some(self.base_model.new_context()?);
        self.current_adapter = Some(adapter_name.to_string());
        
        tracing::info!("Loaded adapter: {}", adapter_name);
        Ok(())
    }
}

#[async_trait]
impl Agent for LocalLoRAAgent {
    async fn process(&mut self, request: &AgentRequest) -> Result<AgentResponse> {
        let context = self.context.as_mut()
            .ok_or(Error::NoAdapterLoaded)?;
        
        // Build prompt based on agent type and request
        let prompt = self.build_prompt(request)?;
        
        // Generate response
        context.eval_string(&prompt)?;
        let response_text = context.generate_text(512)?;
        
        // Parse structured response
        let response = self.parse_response(&response_text)?;
        
        Ok(response)
    }
    
    fn name(&self) -> &str {
        &self.name
    }
    
    fn capabilities(&self) -> Vec<Capability> {
        vec![
            Capability::TextGeneration,
            Capability::TaskClassification,
        ]
    }
}
```

### Intent Classification Agent

```rust
// src/agents/intent_classifier.rs
pub struct IntentClassifierAgent {
    local_agent: LocalLoRAAgent,
}

impl IntentClassifierAgent {
    pub async fn new(model_manager: &BaseModelManager) -> Result<Self> {
        let mut local_agent = LocalLoRAAgent::new(
            "intent-classifier".to_string(),
            "gemma-3-4b", // Fast model for quick classification
            model_manager
        ).await?;
        
        // Load intent classification adapter
        local_agent.load_adapter("intent-classification-v1").await?;
        
        Ok(Self { local_agent })
    }
    
    fn build_prompt(&self, request: &AgentRequest) -> String {
        format!(r#"
Classify this user request into one of these categories:

Categories:
- CREATE_WORKFLOW: User wants to create a new workflow
- MODIFY_WORKFLOW: User wants to modify existing workflow  
- GENERATE_CONTENT: User wants to create content (blog, script, etc.)
- ORGANIZE_KNOWLEDGE: User wants to organize information
- ASK_QUESTION: User has a question about their data
- UNCLEAR: Request is ambiguous and needs clarification

Request: "{}"

Output only the category name."#, request.content)
    }
}

#[async_trait]
impl Agent for IntentClassifierAgent {
    async fn process(&mut self, request: &AgentRequest) -> Result<AgentResponse> {
        let prompt = self.build_prompt(request);
        let context = self.local_agent.context.as_mut().unwrap();
        
        context.eval_string(&prompt)?;
        let response = context.generate_text(50)?; // Short response
        
        let intent = response.trim().to_uppercase();
        let confidence = self.calculate_confidence(&intent)?;
        
        Ok(AgentResponse {
            content: intent,
            confidence,
            reasoning: Some(format!("Classified based on keywords and structure")),
            suggested_actions: self.suggest_next_actions(&intent),
        })
    }
    
    fn name(&self) -> &str {
        "intent-classifier"
    }
    
    fn capabilities(&self) -> Vec<Capability> {
        vec![Capability::IntentClassification]
    }
}
```

### Workflow Builder Agent

```rust
// src/agents/workflow_builder.rs
pub struct WorkflowBuilderAgent {
    local_agent: LocalLoRAAgent,
    template_library: WorkflowTemplateLibrary,
}

impl WorkflowBuilderAgent {
    pub async fn new(model_manager: &BaseModelManager) -> Result<Self> {
        let mut local_agent = LocalLoRAAgent::new(
            "workflow-builder".to_string(),
            "gemma-3-12b", // Larger model for complex reasoning
            model_manager
        ).await?;
        
        // Load workflow building adapter
        local_agent.load_adapter("workflow-builder-v1").await?;
        
        let template_library = WorkflowTemplateLibrary::load_defaults().await?;
        
        Ok(Self { local_agent, template_library })
    }
    
    fn build_prompt(&self, request: &AgentRequest) -> String {
        let similar_templates = self.template_library
            .find_similar_templates(&request.content);
        
        format!(r#"
Create a workflow based on this request: "{}"

Available templates:
{}

Create a detailed workflow with:
1. Clear phases/steps
2. Responsible parties
3. Deliverables for each phase
4. Success criteria

Output as JSON with this structure:
{{
  "name": "workflow name",
  "phases": [
    {{
      "name": "phase name",
      "tasks": ["task1", "task2"],
      "deliverables": ["deliverable1"],
      "duration_estimate": "2 days"
    }}
  ]
}}"#, 
            request.content,
            similar_templates.iter()
                .map(|t| format!("- {}: {}", t.name, t.description))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}
```

## Phase 2: Multi-Agent Coordination

### Agent Manager

```rust
// src/agents/manager.rs
pub struct AgentManager {
    agents: HashMap<String, Box<dyn Agent>>,
    intent_classifier: IntentClassifierAgent,
    model_manager: Arc<BaseModelManager>,
}

impl AgentManager {
    pub async fn initialize() -> Result<Self> {
        let model_manager = Arc::new(BaseModelManager::initialize().await?);
        
        // Initialize agents
        let intent_classifier = IntentClassifierAgent::new(&model_manager).await?;
        let workflow_builder = WorkflowBuilderAgent::new(&model_manager).await?;
        let content_generator = ContentGeneratorAgent::new(&model_manager).await?;
        
        let mut agents: HashMap<String, Box<dyn Agent>> = HashMap::new();
        agents.insert("workflow-builder".to_string(), Box::new(workflow_builder));
        agents.insert("content-generator".to_string(), Box::new(content_generator));
        
        Ok(Self {
            agents,
            intent_classifier,
            model_manager,
        })
    }
    
    pub async fn process_request(&mut self, request: &AgentRequest) -> Result<AgentResponse> {
        // 1. Classify intent
        let intent_response = self.intent_classifier.process(request).await?;
        let intent = intent_response.content.as_str();
        
        // 2. Route to appropriate agent
        let agent_name = self.select_agent_for_intent(intent)?;
        
        // 3. Process with selected agent
        if let Some(agent) = self.agents.get_mut(agent_name) {
            agent.process(request).await
        } else {
            Err(Error::AgentNotFound(agent_name.to_string()))
        }
    }
    
    fn select_agent_for_intent(&self, intent: &str) -> Result<&str> {
        match intent {
            "CREATE_WORKFLOW" | "MODIFY_WORKFLOW" => Ok("workflow-builder"),
            "GENERATE_CONTENT" => Ok("content-generator"),
            "ORGANIZE_KNOWLEDGE" => Ok("knowledge-organizer"),
            _ => Err(Error::UnknownIntent(intent.to_string())),
        }
    }
}
```

### Conversation Flow Manager

```rust
// src/agents/conversation_flow.rs
pub struct ConversationFlowManager {
    agent_manager: AgentManager,
    conversation_history: Vec<ConversationTurn>,
    current_context: ConversationContext,
}

impl ConversationFlowManager {
    pub async fn handle_user_input(&mut self, input: String) -> Result<ConversationResponse> {
        // Build request with conversation context
        let request = AgentRequest {
            content: input.clone(),
            context: self.build_agent_context(),
            preferences: self.current_context.user_preferences.clone(),
        };
        
        // Process request
        let agent_response = self.agent_manager.process_request(&request).await?;
        
        // Update conversation history
        self.conversation_history.push(ConversationTurn {
            user_input: input,
            agent_response: agent_response.clone(),
            timestamp: Utc::now(),
        });
        
        // Check if follow-up questions needed
        let follow_up_questions = self.generate_follow_up_questions(&agent_response)?;
        
        Ok(ConversationResponse {
            main_response: agent_response,
            follow_up_questions,
            suggested_actions: self.suggest_next_actions(),
        })
    }
    
    fn build_agent_context(&self) -> AgentContext {
        AgentContext {
            conversation_history: self.conversation_history.clone(),
            user_profile: self.current_context.user_profile.clone(),
            current_projects: self.current_context.current_projects.clone(),
        }
    }
}
```

## Phase 3: Cloud Integration

### Cloud Provider Interface

```rust
// src/providers/cloud_provider.rs
#[async_trait]
pub trait CloudProvider: Send + Sync {
    async fn generate(
        &mut self,
        prompt: &str,
        config: &GenerationConfig
    ) -> Result<CloudResponse>;
    
    fn model_info(&self) -> ModelInfo;
    fn cost_per_token(&self) -> f64;
    fn rate_limits(&self) -> RateLimits;
}

// OpenAI implementation
pub struct OpenAIProvider {
    client: OpenAIClient,
    model: String,
    rate_limiter: RateLimiter,
}

impl OpenAIProvider {
    pub fn new(api_key: String, model: String) -> Self {
        let client = OpenAIClient::new(api_key);
        let rate_limiter = RateLimiter::new(60, Duration::from_secs(60)); // 60 requests per minute
        
        Self { client, model, rate_limiter }
    }
}

#[async_trait]
impl CloudProvider for OpenAIProvider {
    async fn generate(&mut self, prompt: &str, config: &GenerationConfig) -> Result<CloudResponse> {
        // Rate limiting
        self.rate_limiter.acquire().await?;
        
        // Prepare request
        let request = ChatCompletionRequest {
            model: self.model.clone(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            max_tokens: Some(config.max_tokens),
            temperature: Some(config.temperature),
        };
        
        // Make API call
        let response = self.client.chat_completion(request).await?;
        
        Ok(CloudResponse {
            content: response.choices[0].message.content.clone(),
            model: self.model.clone(),
            tokens_used: response.usage.total_tokens,
            cost: self.calculate_cost(response.usage.total_tokens),
        })
    }
}
```

### Hybrid Routing System

```rust
// src/routing/hybrid_router.rs
pub struct HybridRouter {
    local_agents: AgentManager,
    cloud_providers: HashMap<String, Box<dyn CloudProvider>>,
    routing_strategy: RoutingStrategy,
    cost_tracker: CostTracker,
}

impl HybridRouter {
    pub async fn route_request(&mut self, request: &AgentRequest) -> Result<AgentResponse> {
        // Classify request complexity and requirements
        let routing_analysis = self.analyze_routing_requirements(request).await?;
        
        match self.routing_strategy.decide(&routing_analysis) {
            RoutingDecision::Local(agent_name) => {
                tracing::info!("Routing to local agent: {}", agent_name);
                self.local_agents.process_with_agent(request, &agent_name).await
            },
            RoutingDecision::Cloud(provider_name) => {
                tracing::info!("Routing to cloud provider: {}", provider_name);
                self.process_with_cloud_provider(request, &provider_name).await
            },
            RoutingDecision::Hybrid { local_first, cloud_fallback } => {
                tracing::info!("Trying hybrid approach: {} -> {}", local_first, cloud_fallback);
                
                // Try local first
                match self.local_agents.process_with_agent(request, &local_first).await {
                    Ok(response) if response.confidence > 0.8 => Ok(response),
                    _ => {
                        // Fallback to cloud
                        self.process_with_cloud_provider(request, &cloud_fallback).await
                    }
                }
            }
        }
    }
    
    async fn analyze_routing_requirements(&self, request: &AgentRequest) -> Result<RoutingAnalysis> {
        // Use fast local classifier to analyze request
        let complexity = self.assess_complexity(request).await?;
        let privacy_sensitivity = self.assess_privacy_sensitivity(request);
        let cost_constraints = self.cost_tracker.current_constraints();
        
        Ok(RoutingAnalysis {
            complexity,
            privacy_sensitivity,
            cost_constraints,
            estimated_tokens: self.estimate_token_usage(request),
        })
    }
}
```

## Testing and Validation

### Unit Tests

```rust
// tests/agent_tests.rs
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_intent_classification() {
        let model_manager = BaseModelManager::initialize().await.unwrap();
        let mut agent = IntentClassifierAgent::new(&model_manager).await.unwrap();
        
        let request = AgentRequest {
            content: "I want to create a workflow for my video production process".to_string(),
            context: AgentContext::default(),
            preferences: UserPreferences::default(),
        };
        
        let response = agent.process(&request).await.unwrap();
        assert_eq!(response.content, "CREATE_WORKFLOW");
        assert!(response.confidence > 0.7);
    }
    
    #[tokio::test]
    async fn test_workflow_builder() {
        let model_manager = BaseModelManager::initialize().await.unwrap();
        let mut agent = WorkflowBuilderAgent::new(&model_manager).await.unwrap();
        
        let request = AgentRequest {
            content: "Create a workflow for producing YouTube videos".to_string(),
            context: AgentContext::default(),
            preferences: UserPreferences::default(),
        };
        
        let response = agent.process(&request).await.unwrap();
        
        // Parse workflow JSON
        let workflow: WorkflowDefinition = serde_json::from_str(&response.content).unwrap();
        assert!(!workflow.phases.is_empty());
        assert!(workflow.name.contains("YouTube") || workflow.name.contains("video"));
    }
}
```

### Integration Tests

```rust
// tests/integration_tests.rs
#[tokio::test]
async fn test_end_to_end_workflow_creation() {
    let mut conversation_flow = ConversationFlowManager::initialize().await.unwrap();
    
    // User wants to create a workflow
    let response1 = conversation_flow
        .handle_user_input("I need help creating a workflow for my design agency".to_string())
        .await
        .unwrap();
    
    assert!(response1.main_response.content.contains("workflow"));
    assert!(!response1.follow_up_questions.is_empty());
    
    // User provides more details
    let response2 = conversation_flow
        .handle_user_input("We do brand identity work for startups".to_string())
        .await
        .unwrap();
    
    // Should generate a specific workflow
    let workflow: WorkflowDefinition = serde_json::from_str(&response2.main_response.content).unwrap();
    assert!(workflow.phases.len() >= 3); // Discovery, Design, Delivery phases
}
```

## Deployment Configuration

### Development Setup

```toml
# config/development.toml
[models]
base_directory = "./models"
adapters_directory = "./models/adapters"

[agents]
intent_classifier_adapter = "intent-classification-v1"
workflow_builder_adapter = "workflow-builder-v1"
content_generator_adapter = "content-generation-v1"

[routing]
strategy = "local_only"
enable_cloud_fallback = false

[performance]
max_concurrent_adapters = 2
adapter_switch_timeout = "5s"
```

### Production Setup

```toml
# config/production.toml
[models]
base_directory = "/opt/nodespace/models"
adapters_directory = "/opt/nodespace/models/adapters"

[cloud_providers.openai]
enabled = true
api_key_env = "OPENAI_API_KEY"
models = ["gpt-4o", "gpt-4o-mini"]
rate_limit = 60

[cloud_providers.anthropic]
enabled = true
api_key_env = "ANTHROPIC_API_KEY"
models = ["claude-3-5-sonnet-20241022"]

[routing]
strategy = "hybrid"
complexity_threshold = 0.8
privacy_threshold = "medium"
cost_optimization = true

[performance]
max_concurrent_adapters = 4
preload_common_adapters = true
enable_adapter_caching = true
```

This implementation guide provides a solid foundation for building the NodeSpace agent system, starting with local LoRA adapters and expanding to include cloud integration and advanced routing capabilities.

<function_calls>
<invoke name="TodoWrite">
<parameter name="todos">[{"content": "Create agent architecture documentation with LoRA and cloud LLM support", "status": "completed", "activeForm": "Creating agent architecture documentation with LoRA and cloud LLM support"}, {"content": "Document multi-model strategy and adapter management", "status": "completed", "activeForm": "Documenting multi-model strategy and adapter management"}, {"content": "Create implementation guide for agent development", "status": "completed", "activeForm": "Creating implementation guide for agent development"}]