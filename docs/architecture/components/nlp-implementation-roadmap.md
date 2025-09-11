# NLP Implementation Roadmap for NodeSpace AI-Native Features

## Overview

This document outlines the implementation roadmap for NodeSpace's natural language processing capabilities, following our AI-Native Hybrid Approach. The roadmap prioritizes a complete function calling foundation with preview features that demonstrate the full vision.

## MVP Scope and Timeline (3-4 months)

### Philosophy: Complete Phase 1 + Simple Phase 2/3 Preview

Rather than building incomplete features across multiple phases, we focus on:
- **Complete Phase 1**: Rock-solid function calling that works 95% of the time
- **Simple Phase 2/3**: Preview features that give users a taste of advanced capabilities

This approach provides immediate value while showcasing future potential.

## Phase 1: Function Calling Foundation (Complete Implementation)

**Timeline: Weeks 1-8**  
**Goal: 95%+ accuracy on basic CRUD operations**

### Week 1-2: Function Registry Infrastructure
```rust
// Core infrastructure setup
#[api_function("Creates a new text node with content")]
pub async fn create_text_node(content: String, parent_id: Option<String>) -> Result<String, Error>

#[api_function("Updates a field on any entity")]  
pub async fn update_entity_field(entity_id: String, field: String, value: Value) -> Result<bool, Error>

#[api_function("Queries entities with filters")]
pub async fn query_entities(entity_type: String, filters: QueryFilters) -> Result<Vec<Entity>, Error>
```

**Deliverables:**
- [ ] Function registry macro system
- [ ] Automated training data generation pipeline
- [ ] Build system integration (cargo build triggers training data update)

### Week 3-4: Training Data Generation
```rust
// Auto-generate thousands of training examples
fn generate_training_variations(function: &FunctionMetadata) -> Vec<TrainingExample> {
    match function.name.as_str() {
        "create_text_node" => vec![
            ("Create a project called NodeSpace", "create_text_node(\"NodeSpace\", null)"),
            ("Make a new document with title Research Notes", "create_text_node(\"Research Notes\", null)"),
            ("Add a text node for Meeting Minutes under project-123", "create_text_node(\"Meeting Minutes\", \"project-123\")"),
            // Generate 50+ variations per function
        ],
        // ... other functions
    }
}
```

**Deliverables:**
- [ ] Training data generator for all core functions
- [ ] Quality validation pipeline
- [ ] JSONL export for MLX training

### Week 5-6: Model Fine-tuning
```bash
# MLX-based fine-tuning pipeline
mlx_lm.lora \
  --model google/gemma-3-4b-it \
  --train \
  --data function_calls.jsonl \
  --batch-size 4 \
  --iters 300 \
  --learning-rate 1e-4 \
  --adapter-path nodespace-function-calling \
  --save-every 50
```

**Deliverables:**
- [ ] Fine-tuned Gemma 3 4B model for NodeSpace functions
- [ ] Model evaluation pipeline with accuracy metrics
- [ ] GGUF quantization for production deployment

### Week 7-8: Function Execution Engine
```rust
pub struct FunctionExecutor {
    registry: FunctionRegistry,
    model: LlamaModel,
}

impl FunctionExecutor {
    pub async fn execute_natural_language(&self, input: &str) -> Result<ExecutionResult, Error> {
        // 1. Generate function call from natural language
        let function_call = self.model.generate_function_call(input).await?;
        
        // 2. Parse and validate function call
        let parsed_call = self.parse_function_call(&function_call)?;
        
        // 3. Execute with confirmation interface
        Ok(ExecutionResult {
            function_call: parsed_call,
            preview: self.generate_preview(&parsed_call).await?,
            confidence: self.calculate_confidence(&function_call),
            requires_confirmation: self.assess_risk(&parsed_call),
        })
    }
}
```

**Deliverables:**
- [ ] Function parsing and validation system
- [ ] Execution engine with error handling
- [ ] Confidence scoring system

**Success Criteria for Phase 1:**
- ✅ 95%+ accuracy on basic function calls
- ✅ <2 second response time
- ✅ Graceful error handling for invalid inputs
- ✅ Comprehensive test coverage

## Phase 2 Preview: Smart Entity Resolution (Simple Implementation)

**Timeline: Weeks 9-10**  
**Goal: Demonstrate intelligent disambiguation**

### @ Mention System
```typescript
// Simple but effective disambiguation
User types: "@john"
System shows:
┌─────────────────────────┐
│ @ Mentions              │
├─────────────────────────┤
│ 👤 John Smith          │
│    Engineering • Active│
│                         │
│ 👤 John Doe             │
│    Marketing • Active   │
│                         │
│ 👤 Jonathan Wilson      │
│    Sales • Inactive     │
└─────────────────────────┘
```

### Basic Entity Resolution
```rust
pub async fn resolve_entity_reference(reference: &str, context: &Context) -> EntityResolutionResult {
    let candidates = search_entities(reference).await?;
    
    match candidates.len() {
        0 => EntityResolutionResult::NotFound { suggestion: generate_creation_suggestion(reference) },
        1 => EntityResolutionResult::Resolved { entity: candidates[0] },
        _ => EntityResolutionResult::Ambiguous { 
            candidates,
            clarification_prompt: generate_clarification_prompt(&candidates)
        }
    }
}
```

**Deliverables:**
- [ ] @ mention dropdown component
- [ ] Basic entity search and ranking
- [ ] Simple disambiguation dialogs

**Success Criteria:**
- ✅ Familiar @ mention UX (like Slack/Discord)
- ✅ <100ms search response time
- ✅ Smart ranking (recent collaborators first)

## Phase 3 Preview: Workflow Templates (Simple Implementation)

**Timeline: Weeks 11-12**  
**Goal: Show guided multi-step operations**

### Pre-built Templates
```rust
// Simple workflow template system
pub struct WorkflowTemplate {
    name: String,
    description: String,
    steps: Vec<WorkflowStep>,
}

pub struct WorkflowStep {
    prompt: String,              // "What's the employee's name?"
    function_template: String,   // "create_entity('employee', {'name': '{}'})"
    validation: Option<Validator>,
}

// Example: New Employee Onboarding
let onboarding_template = WorkflowTemplate {
    name: "new_employee_onboarding",
    description: "Set up a new employee with standard configuration",
    steps: vec![
        WorkflowStep {
            prompt: "What's the employee's name?",
            function_template: "create_entity('employee', {'name': '{}'})",
            validation: Some(Validator::RequiredString),
        },
        WorkflowStep {
            prompt: "What's their role?", 
            function_template: "update_entity_field('{}', 'role', '{}')",
            validation: Some(Validator::ValidRole),
        },
        WorkflowStep {
            prompt: "What's their starting salary?",
            function_template: "update_entity_field('{}', 'salary', {})",
            validation: Some(Validator::PositiveNumber),
        },
    ],
};
```

### Visual Workflow Cards
```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│ 1. Create Entity│ -> │ 2. Set Role     │ -> │ 3. Set Salary   │
│                 │    │                 │    │                 │
│ ✅ Completed    │    │ 🔄 In Progress  │    │ ⏸️  Waiting     │
│ John Smith      │    │ Senior Dev      │    │                 │
└─────────────────┘    └─────────────────┘    └─────────────────┘
```

**Deliverables:**
- [ ] Template definition system
- [ ] Visual workflow card UI
- [ ] Step-by-step execution engine

**Success Criteria:**
- ✅ 3-5 common workflow templates working
- ✅ Clear visual progress indication
- ✅ Ability to modify steps before execution

## Confirmation Interface Implementation

**Throughout all phases**

### Cursor-Style Diff Preview
```typescript
interface ConfirmationDialog {
  operation: {
    function: string;
    parameters: Record<string, any>;
  };
  preview: {
    entity_changes: EntityChange[];
    estimated_time: string;
    reversible: boolean;
  };
  trust_metrics: {
    confidence: number;
    similar_operations: number;
    user_success_rate: number;
  };
  actions: {
    cancel: () => void;
    accept: () => void;
    accept_all: () => void;  // Only show after trust built
  };
}
```

### Progressive Trust System
```rust
pub struct TrustManager {
    pub fn should_auto_execute(&self, operation: &Operation, user: &User) -> bool {
        let risk = self.assess_risk(operation);
        let trust = self.get_user_trust_metrics(user, &operation.function_name);
        
        match risk {
            RiskLevel::Low => trust.success_rate > 0.90 && trust.operations_count > 20,
            RiskLevel::Medium => trust.success_rate > 0.95 && trust.operations_count > 50,
            RiskLevel::High => false, // Always confirm
            RiskLevel::Critical => false, // Always confirm
        }
    }
}
```

## Technical Implementation Details

### Model Integration
```rust
// Integration with existing llama.cpp setup
pub struct NodeSpaceNLP {
    model: LlamaModel,
    context: LlamaContext,
    function_registry: FunctionRegistry,
}

impl NodeSpaceNLP {
    pub async fn process_natural_language(&mut self, input: &str) -> Result<FunctionCall, Error> {
        let prompt = self.build_function_calling_prompt(input);
        let response = self.generate_with_confidence(&prompt).await?;
        let function_call = self.parse_function_call(&response.text)?;
        
        Ok(FunctionCall {
            function: function_call,
            confidence: response.confidence,
            raw_response: response.text,
        })
    }
    
    fn build_function_calling_prompt(&self, input: &str) -> String {
        let functions = self.function_registry.get_function_signatures();
        format!(r#"
Available functions:
{}

Convert this natural language request to a function call:
"{}"

Function call:
"#, functions, input)
    }
}
```

### CI/CD Integration
```yaml
# Automated model retraining pipeline
name: Nightly Model Training
on:
  schedule:
    - cron: '0 2 * * *'
  push:
    paths: ['src/**/*.rs']

jobs:
  train:
    runs-on: macos-latest
    steps:
      - name: Generate training data
        run: cargo run --bin generate-training-data
      
      - name: Fine-tune model
        run: |
          mlx_lm.lora \
            --model google/gemma-3-4b-it \
            --data training_data/function_calls.jsonl \
            --adapter-path models/nodespace-$(date +%Y%m%d)
      
      - name: Test model accuracy
        run: python scripts/test_model_accuracy.py --min-accuracy 0.95
      
      - name: Deploy if tests pass
        run: ./scripts/deploy-model.sh
```

## Success Metrics and KPIs

### Technical Metrics
- **Function Call Accuracy**: >95% for common operations
- **Response Time**: <2 seconds for typical queries
- **Model Size**: <4GB for efficient local deployment
- **Training Pipeline Speed**: <30 minutes from API change to model update

### User Experience Metrics
- **Task Completion Rate**: >80% of natural language requests successfully executed
- **User Satisfaction**: >4.5/5 rating for AI assistance
- **Confirmation Rate**: <20% of operations require user modification
- **Progressive Trust**: >50% of users enable auto-execution for routine operations

### Business Metrics
- **Feature Adoption**: >60% of active users use NL interface daily
- **Productivity Gain**: 30% reduction in time for common tasks
- **User Retention**: AI features contribute to 15% higher retention
- **Support Reduction**: 25% fewer support tickets related to UI complexity

## Risk Mitigation Strategies

### Technical Risks
- **Model accuracy degradation**: Continuous monitoring with automatic rollback
- **Performance issues**: Optimize inference with quantization and caching
- **Training data quality**: Automated validation and human review process

### User Experience Risks
- **Over-reliance on AI**: Always provide manual fallbacks
- **Trust issues**: Transparent confidence scores and clear previews
- **Complexity creep**: Focus on reliability over intelligence

### Business Risks
- **Feature scope creep**: Strict adherence to MVP boundaries
- **Resource allocation**: Time-boxed phases with clear success criteria
- **Market timing**: Ship working MVP even if not perfect

## Future Evolution (Post-MVP)

### Advanced Entity Resolution
- Cross-conversation context understanding
- Smart entity creation suggestions
- Relationship-aware disambiguation

### Complex Workflow Automation
- Multi-step operation chaining
- Conditional logic and branching
- Integration with external APIs

### Advanced AI Capabilities
- Multi-turn conversation memory
- Predictive operation suggestions  
- Context-aware help and guidance

---

This roadmap balances ambitious AI-native vision with pragmatic implementation constraints, ensuring we deliver working functionality that users will actually adopt and trust.