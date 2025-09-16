# Hybrid LLM Agent Architecture

## Executive Summary

NodeSpace implements a **hybrid agent architecture** that seamlessly combines local LoRA-adapted models with cloud LLM providers. This approach enables cost-effective local inference for common tasks while leveraging powerful cloud models for complex operations, all through a unified agent interface.

## Architecture Overview

### Multi-Provider LLM Strategy

```rust
pub trait LlmProvider {
    async fn generate(&mut self, prompt: &str, config: &GenerationConfig) -> Result<String>;
    fn model_info(&self) -> ModelInfo;
    fn cost_per_token(&self) -> Option<f64>;
    fn supports_streaming(&self) -> bool;
}

// Local provider with LoRA adapters
pub struct LocalLoRAProvider {
    base_model: Arc<LlamaModel>,
    adapter_manager: AdapterManager,
    active_adapter: Option<String>,
}

// Cloud provider wrapper
pub struct CloudLlmProvider {
    client: Box<dyn CloudClient>, // OpenAI, Anthropic, etc.
    model: String,
    rate_limiter: RateLimiter,
}
```

### Intelligent Provider Selection

```rust
pub struct HybridAgentSystem {
    local_provider: LocalLoRAProvider,
    cloud_providers: HashMap<String, CloudLlmProvider>,
    routing_strategy: RoutingStrategy,
    cost_tracker: CostTracker,
}

#[derive(Debug, Clone)]
pub enum RoutingStrategy {
    LocalFirst,           // Try local, fallback to cloud
    CostOptimized,        // Route based on cost analysis
    QualityOptimized,     // Route based on expected quality
    Hybrid(HybridConfig), // Smart routing with multiple factors
}

impl HybridAgentSystem {
    pub async fn process_request(&mut self, request: &AgentRequest) -> Result<AgentResponse> {
        let routing_decision = self.decide_routing(request).await?;
        
        match routing_decision.provider {
            ProviderChoice::Local(adapter) => {
                self.process_with_local_adapter(request, &adapter).await
            },
            ProviderChoice::Cloud(provider_name) => {
                self.process_with_cloud_provider(request, &provider_name).await
            },
        }
    }
    
    async fn decide_routing(&self, request: &AgentRequest) -> Result<RoutingDecision> {
        let factors = RoutingFactors {
            complexity: self.assess_complexity(request),
            cost_budget: request.cost_budget,
            latency_requirement: request.max_latency,
            quality_requirement: request.min_quality,
            privacy_level: request.privacy_level,
        };
        
        self.routing_strategy.decide(factors).await
    }
}
```

## Local LoRA Adapter System

### Efficient Adapter Management

```rust
pub struct AdapterManager {
    base_model: Arc<LlamaModel>,
    loaded_adapters: HashMap<String, AdapterHandle>,
    adapter_cache: LruCache<String, AdapterData>,
    switch_queue: VecDeque<AdapterSwitchRequest>,
}

impl AdapterManager {
    pub async fn switch_adapter(&mut self, adapter_name: &str) -> Result<()> {
        if !self.is_adapter_loaded(adapter_name) {
            // Unload LRU adapter if at capacity
            if self.loaded_adapters.len() >= self.max_concurrent_adapters {
                self.unload_lru_adapter().await?;
            }
            
            // Load new adapter
            self.load_adapter(adapter_name).await?;
        }
        
        self.set_active_adapter(adapter_name);
        Ok(())
    }
    
    pub async fn preload_adapters(&mut self, adapters: &[String]) -> Result<()> {
        // Parallel loading of multiple adapters for faster switching
        let futures: Vec<_> = adapters.iter()
            .map(|name| self.load_adapter_async(name))
            .collect();
            
        futures::future::try_join_all(futures).await?;
        Ok(())
    }
}
```

### Specialized Agent Adapters

```rust
pub struct CreatorAgentAdapters {
    pub intent_classifier: String,     // Quick intent recognition
    pub workflow_builder: String,      // Workflow creation and modification  
    pub content_generator: String,     // Content creation assistance
    pub knowledge_organizer: String,   // Information structuring
    pub ambiguity_resolver: String,    // Clarification and disambiguation
}

impl CreatorAgentAdapters {
    pub fn default_setup() -> Self {
        Self {
            intent_classifier: "intent-classification-v1".to_string(),
            workflow_builder: "workflow-builder-v1".to_string(), 
            content_generator: "content-generation-v1".to_string(),
            knowledge_organizer: "knowledge-org-v1".to_string(),
            ambiguity_resolver: "ambiguity-aware-v1".to_string(),
        }
    }
}
```

## Cloud Provider Integration

### Multi-Provider Support

```rust
pub enum CloudProvider {
    OpenAI {
        client: OpenAIClient,
        models: Vec<String>,
        pricing: PricingTier,
    },
    Anthropic {
        client: AnthropicClient, 
        models: Vec<String>,
        pricing: PricingTier,
    },
    Google {
        client: GeminiClient,
        models: Vec<String>, 
        pricing: PricingTier,
    },
}

pub struct CloudLlmProvider {
    provider: CloudProvider,
    rate_limiter: RateLimiter,
    retry_policy: RetryPolicy,
    cost_tracker: Arc<Mutex<CostTracker>>,
}

impl LlmProvider for CloudLlmProvider {
    async fn generate(&mut self, prompt: &str, config: &GenerationConfig) -> Result<String> {
        // Rate limiting
        self.rate_limiter.acquire().await?;
        
        // Cost estimation
        let estimated_cost = self.estimate_cost(prompt, config)?;
        if estimated_cost > config.max_cost {
            return Err(Error::CostLimitExceeded(estimated_cost));
        }
        
        // Generate with retry logic
        let response = self.retry_policy.execute(|| {
            self.provider.generate(prompt, config)
        }).await?;
        
        // Track actual cost
        let actual_cost = self.calculate_actual_cost(&response)?;
        self.cost_tracker.lock().await.record_cost(actual_cost);
        
        Ok(response.content)
    }
}
```

### Smart Fallback Strategy

```rust
pub struct FallbackStrategy {
    primary: Box<dyn LlmProvider>,
    fallback_chain: Vec<Box<dyn LlmProvider>>,
    failure_threshold: u32,
    circuit_breaker: CircuitBreaker,
}

impl FallbackStrategy {
    pub async fn generate_with_fallback(&mut self, prompt: &str, config: &GenerationConfig) -> Result<String> {
        // Try primary provider
        match self.primary.generate(prompt, config).await {
            Ok(response) => {
                self.circuit_breaker.record_success();
                return Ok(response);
            },
            Err(e) => {
                self.circuit_breaker.record_failure();
                log::warn!("Primary provider failed: {}", e);
            }
        }
        
        // Try fallback providers in order
        for fallback in &mut self.fallback_chain {
            if let Ok(response) = fallback.generate(prompt, config).await {
                log::info!("Fallback provider succeeded");
                return Ok(response);
            }
        }
        
        Err(Error::AllProvidersFailed)
    }
}
```

## Agent Routing Intelligence

### Request Classification

```rust
#[derive(Debug, Clone)]
pub struct RequestClassification {
    pub complexity: ComplexityLevel,
    pub domain: DomainType,
    pub urgency: UrgencyLevel,
    pub privacy_sensitivity: PrivacyLevel,
    pub estimated_tokens: u32,
}

#[derive(Debug, Clone)]
pub enum ComplexityLevel {
    Simple,    // Template-based, local LoRA can handle
    Moderate,  // Requires reasoning, local 12B might work
    Complex,   // Multi-step reasoning, cloud model recommended
    Expert,    // Requires latest/largest models
}

impl HybridAgentSystem {
    pub async fn classify_request(&self, request: &AgentRequest) -> RequestClassification {
        // Use lightweight local model for classification
        let classification_prompt = format!(
            "Classify this request: {}\n\nOutput JSON with complexity, domain, urgency, privacy_sensitivity",
            request.content
        );
        
        let response = self.local_provider
            .generate_with_adapter(&classification_prompt, "intent-classifier")
            .await?;
            
        serde_json::from_str(&response)?
    }
    
    pub fn should_use_local(&self, classification: &RequestClassification, config: &RoutingConfig) -> bool {
        match (&classification.complexity, &classification.privacy_sensitivity) {
            (ComplexityLevel::Simple, _) => true,
            (ComplexityLevel::Moderate, _) if config.prefer_local => true,
            (_, PrivacyLevel::High) => true, // Always keep sensitive data local
            (ComplexityLevel::Complex, _) if config.cloud_budget_available() => false,
            _ => config.default_to_local,
        }
    }
}
```

### Cost-Aware Routing

```rust
pub struct CostOptimizer {
    local_cost_per_token: f64,        // Amortized local inference cost
    cloud_costs: HashMap<String, f64>, // Per-token costs by provider
    budget_tracker: BudgetTracker,
    performance_history: PerformanceTracker,
}

impl CostOptimizer {
    pub fn optimize_routing(&self, request: &AgentRequest, classification: &RequestClassification) -> RoutingDecision {
        let local_cost = self.estimate_local_cost(classification);
        let cloud_options = self.estimate_cloud_costs(classification);
        
        // Factor in quality expectations
        let quality_weights = self.calculate_quality_weights(request);
        
        // Consider current budget constraints
        let budget_constraints = self.budget_tracker.current_constraints();
        
        // Historical performance analysis
        let success_rates = self.performance_history.get_success_rates(classification);
        
        self.select_optimal_provider(local_cost, cloud_options, quality_weights, budget_constraints, success_rates)
    }
    
    pub fn estimate_local_cost(&self, classification: &RequestClassification) -> f64 {
        // Factor in: hardware amortization, electricity, inference time
        let base_cost = self.local_cost_per_token * classification.estimated_tokens as f64;
        let adapter_switching_cost = if self.requires_adapter_switch() { 0.001 } else { 0.0 };
        base_cost + adapter_switching_cost
    }
}
```

## Implementation Phases

### Phase 1: Local LoRA Foundation

```rust
// Core local agent with adapter management
pub struct Phase1LocalAgent {
    adapter_manager: AdapterManager,
    specialized_agents: HashMap<String, SpecializedAgent>,
}

impl Phase1LocalAgent {
    pub async fn initialize() -> Result<Self> {
        let mut adapter_manager = AdapterManager::new("./models/gemma-3-12b-it-q4_k_m.gguf").await?;
        
        // Load specialized adapters
        adapter_manager.load_adapter("intent-classification").await?;
        adapter_manager.load_adapter("workflow-builder").await?;
        adapter_manager.load_adapter("content-generation").await?;
        
        let specialized_agents = HashMap::from([
            ("intent".to_string(), SpecializedAgent::new("intent-classification")),
            ("workflow".to_string(), SpecializedAgent::new("workflow-builder")),
            ("content".to_string(), SpecializedAgent::new("content-generation")),
        ]);
        
        Ok(Self { adapter_manager, specialized_agents })
    }
}
```

### Phase 2: Hybrid Architecture

```rust
// Add cloud provider integration
pub struct Phase2HybridAgent {
    local_agent: Phase1LocalAgent,
    cloud_providers: HashMap<String, CloudLlmProvider>,
    router: IntelligentRouter,
    cost_optimizer: CostOptimizer,
}

impl Phase2HybridAgent {
    pub async fn process_request(&mut self, request: &AgentRequest) -> Result<AgentResponse> {
        let classification = self.classify_request(request).await?;
        let routing_decision = self.router.decide_routing(&classification, &request.preferences).await?;
        
        match routing_decision.provider {
            ProviderChoice::Local(adapter) => {
                self.local_agent.process_with_adapter(request, &adapter).await
            },
            ProviderChoice::Cloud(provider) => {
                self.cloud_providers[&provider].process_request(request).await
            },
        }
    }
}
```

### Phase 3: Advanced Intelligence

```rust
// Learning and optimization capabilities
pub struct Phase3IntelligentAgent {
    hybrid_agent: Phase2HybridAgent,
    learning_system: LearningSystem,
    performance_optimizer: PerformanceOptimizer,
    user_preference_tracker: UserPreferenceTracker,
}

impl Phase3IntelligentAgent {
    pub async fn adaptive_processing(&mut self, request: &AgentRequest) -> Result<AgentResponse> {
        // Learn from historical performance
        let performance_insights = self.learning_system.analyze_similar_requests(request).await?;
        
        // Adapt routing based on user preferences
        let user_preferences = self.user_preference_tracker.get_preferences(&request.user_id);
        
        // Optimize for user's actual success patterns
        let optimized_strategy = self.performance_optimizer.optimize_for_user(
            request, 
            &performance_insights,
            &user_preferences
        ).await?;
        
        self.hybrid_agent.process_with_strategy(request, &optimized_strategy).await
    }
}
```

## Configuration Management

### Tiered Configuration

```rust
#[derive(Debug, Clone)]
pub struct AgentConfiguration {
    pub local_config: LocalConfig,
    pub cloud_config: CloudConfig,
    pub routing_config: RoutingConfig,
    pub cost_config: CostConfig,
}

#[derive(Debug, Clone)]  
pub struct LocalConfig {
    pub base_model_path: PathBuf,
    pub adapter_directory: PathBuf,
    pub max_concurrent_adapters: usize,
    pub gpu_layers: u32,
    pub context_size: u32,
}

#[derive(Debug, Clone)]
pub struct CloudConfig {
    pub enabled_providers: Vec<CloudProvider>,
    pub api_keys: HashMap<String, String>,
    pub rate_limits: HashMap<String, u32>,
    pub timeout_settings: TimeoutConfig,
}

#[derive(Debug, Clone)]
pub struct RoutingConfig {
    pub default_strategy: RoutingStrategy,
    pub complexity_thresholds: ComplexityThresholds,
    pub privacy_routing_rules: PrivacyRules,
    pub cost_optimization_enabled: bool,
}
```

### User-Configurable Preferences

```rust
#[derive(Debug, Clone)]
pub struct UserPreferences {
    pub privacy_preference: PrivacyLevel,
    pub cost_sensitivity: CostSensitivity,
    pub quality_vs_speed: QualitySpeedPreference,
    pub provider_preferences: Vec<ProviderPreference>,
}

impl UserPreferences {
    pub fn creator_defaults() -> Self {
        Self {
            privacy_preference: PrivacyLevel::High,  // Keep creative work private
            cost_sensitivity: CostSensitivity::Medium, // Balance cost and quality
            quality_vs_speed: QualitySpeedPreference::Balanced,
            provider_preferences: vec![
                ProviderPreference::Local,  // Prefer local for privacy
                ProviderPreference::CloudFallback, // Use cloud when needed
            ],
        }
    }
    
    pub fn enterprise_defaults() -> Self {
        Self {
            privacy_preference: PrivacyLevel::Maximum,
            cost_sensitivity: CostSensitivity::Low, // Quality over cost
            quality_vs_speed: QualitySpeedPreference::Quality,
            provider_preferences: vec![
                ProviderPreference::CloudPrimary, // Use best available models
                ProviderPreference::LocalFallback,
            ],
        }
    }
}
```

## Benefits of Hybrid Architecture

### For Free Tier Users
- **Local LoRA adapters** handle 80%+ of requests
- **Cost-effective operation** with specialized local models
- **Complete privacy** for sensitive creative work
- **Offline capability** for core functionality

### For Premium Users  
- **Best-of-both-worlds** approach
- **Intelligent cloud routing** for complex tasks
- **Cost optimization** across providers
- **Quality guarantees** with fallback strategies

### For Enterprise Users
- **Maximum flexibility** in provider selection
- **Advanced cost controls** and budgeting
- **Custom adapter training** and deployment
- **Compliance-aware routing** for regulated industries

This hybrid architecture positions NodeSpace as a uniquely flexible AI platform that can scale from individual creators to enterprise teams while maintaining optimal cost, quality, and privacy characteristics.