# Hybrid BERT + LLM Intent Classification System

## Overview

NodeSpace implements a sophisticated hybrid intent classification system that combines fast BERT-based classification with LLM-powered multi-intent decomposition. This approach provides **1-5ms classification for 70-80% of requests** while leveraging fine-tuned LLM models for complex multi-step scenarios that require human confirmation.

The system addresses the core challenges identified in NodeSpace's NLP evaluation:
- ❌ **Fine-tuned models**: Excellent intent classification (100% accuracy) but poor confidence scoring and ambiguity detection (28.6% accuracy) 
- ❌ **Base models**: Good overall performance but inconsistent confidence calibration
- ✅ **Hybrid solution**: BERT provides reliable confidence + multi-intent detection, LLMs handle complex decomposition

## Architecture Philosophy

### Three-Tier Classification Strategy

The hybrid system uses a progressive routing approach that balances speed, accuracy, and user control:

```
User Input
    ↓
┌─────────────────────┐
│ Phase 1: BERT Quick │ → 70-80% of requests (1-5ms)
│ Classification      │   High confidence, single intent
└─────────────────────┘
    ↓ Medium confidence
┌─────────────────────┐
│ Phase 2: LLM        │ → 15-20% of requests (50-200ms)  
│ Verification        │   Verify BERT suggestion
└─────────────────────┘
    ↓ Low confidence/Multi-intent
┌─────────────────────┐
│ Phase 3: LLM Full   │ → 5-10% of requests (200-1000ms)
│ Decomposition       │   Multi-step workflow analysis
└─────────────────────┘
```

### Integration with NodeSpace AI-Native Hybrid Approach

This intent classification system seamlessly integrates with NodeSpace's established **AI-Native Hybrid Approach** principles:

- **Natural Language as Primary Interface**: Users express intent conversationally
- **Structured Confirmation Before Action**: Multi-intent requests show execution plans for approval  
- **Progressive Trust and Automation**: BERT builds trust through consistent performance
- **Risk Classification**: Intent classification informs operation risk assessment

## Technical Architecture

### BERT Quick Classifier Design

Following NodeSpace's established patterns (similar to `EmbeddingGenerator` in `nodespace-nlp-engine`):

```rust
//! BERT-based intent classification using candle-rs
//! Follows the same pattern as embedding.rs for consistency

use dashmap::DashMap;
use std::sync::Arc;
use serde::{Serialize, Deserialize};

#[cfg(feature = "bert-classification")]
use candle_core::{Device, DType, Tensor};
use candle_transformers::models::bert::BertModel;
use tokenizers::Tokenizer;

/// Configuration for BERT intent classifier
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentClassifierConfig {
    pub model_name: String,
    pub model_path: Option<std::path::PathBuf>,
    pub max_sequence_length: usize,
    pub confidence_threshold_high: f32,  // 0.85 - execute directly
    pub confidence_threshold_low: f32,   // 0.65 - verify with LLM
    pub intent_categories: Vec<String>,  // NodeSpace's 7 intents
}

/// Quick classification decision
#[derive(Debug, Clone)]
pub enum ClassificationDecision {
    Execute { 
        intent: String, 
        confidence: f32,
        is_multi_intent: bool,
    },
    Verify { 
        intent: String, 
        confidence: f32,
        alternative_intents: Vec<String>,
    },
    LLMRequired { 
        reason: String,
        detected_intents: Vec<String>,
    },
}

/// BERT-based intent classifier (similar to EmbeddingGenerator pattern)
pub struct IntentClassifier {
    config: IntentClassifierConfig,
    #[cfg(feature = "bert-classification")]
    model: Option<BertClassificationModel>,
    device: Device,
    cache: Arc<DashMap<String, ClassificationResult>>,
    initialized: bool,
}
```

### Multi-Task BERT Architecture

The BERT model uses multi-task learning to simultaneously:

1. **Single Intent Classification**: Softmax over 7 NodeSpace intent categories
2. **Multi-Intent Detection**: Sigmoid outputs to detect multiple simultaneous intents
3. **Confidence Regression**: Direct confidence prediction calibrated to real performance

```rust
#[cfg(feature = "bert-classification")]
struct BertClassificationModel {
    bert: BertModel,
    // Single intent classification head
    classifier: candle_nn::Linear,           // 768 → 7 (NodeSpace intents)  
    // Multi-intent detection head
    multi_intent_detector: candle_nn::Linear, // 768 → 7 (binary per intent)
    // Confidence regression head
    confidence_regressor: candle_nn::Linear,  // 768 → 1 (confidence score)
}
```

### NodeSpace Intent Categories

The system uses NodeSpace's proven 7-category taxonomy from the NLP evaluation:

```rust
const NODESPACE_INTENTS: &[&str] = &[
    "CREATE_SCHEMA",    // Database/entity structure creation
    "RETRIEVE_DATA",    // Data querying and retrieval
    "UPDATE_RECORD",    // Record modification operations
    "DELETE_DATA",      // Data removal operations  
    "AGGREGATE",        // Mathematical analysis and calculations
    "RAG_SEARCH",       // Knowledge base and document queries
    "CREATE_WORKFLOW",  // Automation and process setup
];
```

## Integration with Existing NodeSpace Architecture

### Enhanced IntentClassificationResponse

Building on the existing `IntentClassificationResponse` structure:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentClassificationResponse {
    // Existing fields
    pub intent: String,
    pub ambiguity_level: String, 
    pub confidence: f64,
    pub alternative_intents: Vec<String>,
    pub clarification_needed: Option<String>,
    
    // New hybrid approach fields
    pub classification_method: ClassificationMethod,
    pub is_multi_intent: bool,
    pub execution_plan: Option<Vec<ExecutionStep>>,
    pub bert_confidence: Option<f64>,
    pub processing_time_breakdown: ProcessingTimes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClassificationMethod {
    BertDirect,      // High confidence BERT classification
    BertLlmVerify,   // BERT + LLM verification
    LlmFull,         // Full LLM processing
    LlmFallback,     // BERT unavailable, LLM only
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStep {
    pub step_number: u32,
    pub intent: String,
    pub description: String,
    pub parameters: HashMap<String, String>,
    pub depends_on: Vec<u32>,
    pub risk_level: RiskLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]  
pub struct ProcessingTimes {
    pub bert_time_ms: u64,
    pub llm_time_ms: u64,
    pub total_time_ms: u64,
}
```

### Tauri Command Integration

Enhanced `classify_intent` command that integrates with existing NodeSpace architecture:

```rust
#[tauri::command]
async fn classify_intent(
    request: IntentClassificationRequest, 
    state: State<'_, AppState>
) -> Result<IntentClassificationResult, String> {
    let start_time = Instant::now();
    
    println!("\n=== HYBRID INTENT CLASSIFICATION ===");
    println!("📝 Input: {}", request.input);
    
    // Phase 1: BERT Quick Classification
    let bert_decision = {
        let classifier_guard = state.intent_classifier.read()
            .map_err(|e| format!("Failed to acquire classifier lock: {}", e))?;
        
        if let Some(classifier) = classifier_guard.as_ref() {
            Some(classifier.classify(&request.input).await
                .map_err(|e| format!("BERT classification failed: {}", e))?)
        } else {
            None
        }
    };
    
    match bert_decision {
        Some(ClassificationDecision::Execute { intent, confidence, .. }) => {
            println!("🚀 BERT Direct: {} (confidence: {:.2})", intent, confidence);
            
            return create_direct_response(request, intent, confidence, start_time);
        },
        
        Some(ClassificationDecision::Verify { intent, confidence, alternatives }) => {
            println!("🔍 BERT Verify: {} (confidence: {:.2})", intent, confidence);
            
            return verify_with_llm(request, intent, confidence, alternatives, start_time, &state).await;
        },
        
        Some(ClassificationDecision::LLMRequired { reason, detected_intents }) => {
            println!("🧠 BERT → LLM: {} (detected: {:?})", reason, detected_intents);
            
            return full_llm_classification(request, Some(detected_intents), start_time, &state).await;
        },
        
        None => {
            println!("⚠️ BERT not available, using existing LLM");
            return full_llm_classification(request, None, start_time, &state).await;
        }
    }
}
```

## Multi-Intent Decomposition

### Multi-Step Workflow Analysis

When BERT detects multiple intents or LLM identifies complex requests, the system provides structured execution plans:

```rust
async fn decompose_multi_intent(
    input: &str,
    detected_intents: Vec<String>
) -> Result<IntentClassificationResponse, String> {
    
    let decomposition_prompt = format!(
        "This request involves multiple steps. Break it down into an execution plan:
        
        User request: \"{}\"
        Detected intents: {:?}
        
        Please provide a JSON execution plan:
        {{
            \"is_multi_intent\": true,
            \"primary_intent\": \"MOST_IMPORTANT_INTENT\",
            \"execution_steps\": [
                {{
                    \"step_number\": 1,
                    \"intent\": \"RETRIEVE_DATA\",
                    \"description\": \"Get customer engagement data\",
                    \"parameters\": {{\"entity_type\": \"customers\", \"filter\": \"engagement_data\"}},
                    \"depends_on\": [],
                    \"risk_level\": \"low\"
                }},
                {{
                    \"step_number\": 2,
                    \"intent\": \"CREATE_WORKFLOW\",
                    \"description\": \"Create re-engagement campaign for at-risk customers\",
                    \"parameters\": {{\"workflow_type\": \"campaign\", \"target\": \"at_risk_customers\"}},
                    \"depends_on\": [1],
                    \"risk_level\": \"medium\"
                }}
            ],
            \"requires_confirmation\": true,
            \"clarification\": \"I'll analyze user engagement data first, then create a re-engagement campaign for at-risk customers. Should I proceed with step 1?\"
        }}",
        input, detected_intents
    );
    
    let response = generate_with_llm(decomposition_prompt).await?;
    let execution_plan: ExecutionPlan = parse_execution_plan_response(&response)?;
    
    // Convert to IntentClassificationResponse
    Ok(IntentClassificationResponse {
        intent: "MULTI_INTENT_WORKFLOW".to_string(),
        ambiguity_level: "COMPLEX".to_string(),
        confidence: 0.85,
        alternative_intents: detected_intents,
        clarification_needed: execution_plan.clarification,
        classification_method: ClassificationMethod::LlmFull,
        is_multi_intent: true,
        execution_plan: Some(execution_plan.execution_steps),
        bert_confidence: None,
        processing_time_breakdown: ProcessingTimes {
            bert_time_ms: 0,
            llm_time_ms: generate_time_ms,
            total_time_ms: total_time_ms,
        },
    })
}
```

### Common Multi-Intent Patterns

The system recognizes and handles common multi-step patterns:

**Research + Create Pattern:**
```
"Research authentication best practices and create implementation tasks"
→ Step 1: RAG_SEARCH ("authentication best practices")  
→ Step 2: CREATE_WORKFLOW ("implementation tasks based on research")
```

**Update + Notify Pattern:**
```
"Update order status to shipped and send customer notification"  
→ Step 1: UPDATE_RECORD ("order status = shipped")
→ Step 2: CREATE_WORKFLOW ("customer notification workflow")
```

**Analyze + Action Pattern:**
```
"Analyze sales performance and create improvement recommendations"
→ Step 1: AGGREGATE ("sales performance analysis")  
→ Step 2: CREATE_WORKFLOW ("improvement recommendations workflow")
```

## Training Data Strategy

### Converting Existing NodeSpace Training Data

The system leverages NodeSpace's existing comprehensive training data:

```rust
// Convert NodeSpace's existing JSONL training data to BERT format
pub fn convert_nodespace_training_data() -> Result<Vec<BertTrainingExample>, Box<dyn std::error::Error>> {
    let jsonl_files = [
        "final_balanced_training_data.jsonl",      // 299 examples
        "validation_data.jsonl",                   // Validation set
    ];
    
    let mut training_examples = Vec::new();
    
    for file in jsonl_files {
        for line in std::fs::read_to_string(file)?.lines() {
            let item: serde_json::Value = serde_json::from_str(line)?;
            
            if let Some(text) = item["text"].as_str() {
                if let Some((user_input, expected_response)) = extract_training_pair(text) {
                    let example = BertTrainingExample {
                        text: user_input,
                        intent_label: expected_response.intent,
                        confidence_score: expected_response.confidence,
                        is_multi_intent: detect_multi_intent_training(&user_input),
                        ambiguity_level: expected_response.ambiguity_level,
                    };
                    training_examples.push(example);
                }
            }
        }
    }
    
    println!("Converted {} training examples for BERT", training_examples.len());
    Ok(training_examples)
}
```

### Multi-Task Training Strategy

The BERT model trains on three simultaneous tasks:

1. **Intent Classification**: Cross-entropy loss on NodeSpace's 7 categories
2. **Multi-Intent Detection**: Binary cross-entropy for multi-label classification
3. **Confidence Regression**: MSE loss against calibrated confidence scores

```python
# Multi-task loss function
def compute_loss(outputs, labels):
    intent_loss = F.cross_entropy(outputs['single_intent'], labels['intent'])
    
    multi_loss = F.binary_cross_entropy_with_logits(
        outputs['multi_intent'], labels['multi_intent']
    )
    
    conf_loss = F.mse_loss(outputs['confidence'], labels['confidence'])
    
    # Weighted combination
    return intent_loss + 0.5 * multi_loss + 0.3 * conf_loss
```

## Performance Characteristics

### Speed Benchmarks

Based on NodeSpace's evaluation methodology and target performance:

| Classification Method | Latency | Accuracy | Use Cases |
|----------------------|---------|----------|-----------|
| BERT Direct | 1-5ms | 90-95% | Simple, clear requests |  
| BERT + LLM Verify | 50-200ms | 95-98% | Medium confidence cases |
| LLM Full Decomposition | 200-1000ms | 95-99% | Multi-intent, complex requests |
| LLM Fallback | 200-2000ms | 93-97% | BERT unavailable |

### Memory Usage

- **BERT Model**: ~400MB (DistilBERT) vs ~8GB (Gemma 3 12B)
- **Classification Cache**: ~4KB per 1000 cached classifications  
- **Total Overhead**: <500MB additional memory usage

### Accuracy Improvements

Projected improvements over current fine-tuned models:

| Metric | Current Fine-Tuned | Hybrid System | Improvement |
|--------|-------------------|---------------|-------------|
| Intent Accuracy | 100% | 95-99% | Maintained high accuracy |
| Confidence Calibration | Poor (0.5 fixed) | Excellent | Real confidence scores |
| Ambiguity Detection | 28.6% | 80-90% | 3x improvement |
| Multi-Intent Detection | Not supported | 85-95% | New capability |

## Integration with NodeSpace Trust Model

### Progressive Trust Building

The hybrid system integrates with NodeSpace's trust model for progressive automation:

```rust
pub struct HybridTrustMetrics {
    // BERT-specific trust metrics
    pub bert_accuracy_by_confidence: HashMap<ConfidenceRange, f32>,
    pub bert_auto_execution_threshold: f32,
    
    // LLM verification trust metrics  
    pub llm_verification_accuracy: f32,
    pub verification_override_rate: f32,
    
    // Multi-intent workflow trust
    pub workflow_completion_rate: f32,
    pub user_plan_modification_rate: f32,
}

impl HybridTrustMetrics {
    pub fn should_auto_execute_bert(&self, confidence: f32) -> bool {
        let confidence_range = self.get_confidence_range(confidence);
        let historical_accuracy = self.bert_accuracy_by_confidence
            .get(&confidence_range)
            .unwrap_or(&0.0);
        
        // Auto-execute BERT results if historically accurate
        *historical_accuracy >= 0.95 && confidence >= self.bert_auto_execution_threshold
    }
    
    pub fn should_skip_llm_verification(&self, bert_confidence: f32) -> bool {
        // Skip verification if BERT is highly trusted at this confidence level
        self.bert_accuracy_by_confidence
            .get(&self.get_confidence_range(bert_confidence))
            .map_or(false, |accuracy| *accuracy >= 0.98)
    }
}
```

### Risk Assessment Integration

The hybrid system enhances NodeSpace's risk classification:

```rust
pub fn assess_hybrid_execution_risk(
    classification: &IntentClassificationResponse,
    function_metadata: &FunctionMetadata,
    trust_metrics: &HybridTrustMetrics
) -> ExecutionDecision {
    
    match classification.classification_method {
        ClassificationMethod::BertDirect => {
            if trust_metrics.should_auto_execute_bert(classification.confidence as f32) {
                ExecutionDecision::AutoExecute
            } else {
                ExecutionDecision::RequireConfirmation
            }
        },
        
        ClassificationMethod::BertLlmVerify => {
            // Verification increases confidence, but still check function risk
            let function_risk = assess_function_risk(function_metadata);
            if matches!(function_risk, RiskLevel::Low) {
                ExecutionDecision::AutoExecute
            } else {
                ExecutionDecision::RequireConfirmation
            }
        },
        
        ClassificationMethod::LlmFull => {
            // Complex operations always require confirmation
            if classification.is_multi_intent {
                ExecutionDecision::RequireWorkflowApproval
            } else {
                ExecutionDecision::RequireConfirmation  
            }
        },
    }
}
```

## Future Enhancement Roadmap

### Phase 1: Core Implementation (Months 1-2)
- ✅ **BERT model training** on NodeSpace's existing dataset
- ✅ **Multi-task architecture** (intent + confidence + multi-intent)
- ✅ **Integration with existing Tauri commands**
- ✅ **Basic caching and performance optimization**

### Phase 2: Advanced Features (Months 3-4)  
- 🔄 **Context-aware classification** using conversation history
- 🔄 **Dynamic confidence thresholds** based on user trust metrics
- 🔄 **Advanced multi-intent patterns** and workflow templates
- 🔄 **Performance optimization** and model compression

### Phase 3: Intelligence Layer (Months 5-6)
- 🔮 **Predictive intent suggestion** based on user patterns  
- 🔮 **Cross-conversation context** for improved accuracy
- 🔮 **Active learning** from user corrections and feedback
- 🔮 **Domain adaptation** for specific NodeSpace use cases

## Implementation Checklist

### Dependencies and Setup
- [ ] Add Candle and tokenizer dependencies to `Cargo.toml`
- [ ] Create `intent_classifier.rs` module following NLP engine patterns
- [ ] Set up BERT model downloading and caching infrastructure
- [ ] Configure feature flags for optional BERT functionality

### Core Functionality  
- [ ] Implement `IntentClassifier` struct with initialization patterns
- [ ] Create multi-task BERT architecture with classification heads
- [ ] Build confidence calculation and multi-intent detection logic
- [ ] Integrate with existing `classify_intent` Tauri command

### Training and Evaluation
- [ ] Convert NodeSpace's JSONL training data to BERT format
- [ ] Set up multi-task training pipeline with balanced datasets
- [ ] Create evaluation framework compatible with existing test suite
- [ ] Benchmark performance against current fine-tuned models

### Integration and Testing
- [ ] Update `AppState` to include `IntentClassifier` instance
- [ ] Modify response structures for hybrid classification results
- [ ] Implement progressive trust metrics for BERT performance
- [ ] Create comprehensive integration tests

### Documentation and Deployment  
- [ ] Update architecture documentation with hybrid approach
- [ ] Create training data generation and model update procedures
- [ ] Document performance characteristics and tuning guidelines
- [ ] Plan deployment strategy for model updates and versioning

## Conclusion

The Hybrid BERT + LLM Intent Classification System represents a strategic evolution of NodeSpace's AI capabilities. By combining fast BERT classification with sophisticated LLM decomposition, the system achieves the best of both worlds:

- **⚡ Performance**: 70-80% of requests processed in 1-5ms
- **🎯 Accuracy**: Maintains high intent classification while adding reliable confidence scoring  
- **🔧 Capability**: Multi-intent detection and workflow decomposition for complex requests
- **🛡️ Safety**: Integrates with NodeSpace's trust model and risk assessment framework
- **📈 Scalability**: Reduces LLM load while improving user experience

This approach aligns perfectly with NodeSpace's AI-Native Hybrid philosophy: leveraging AI capabilities while maintaining human agency and progressive trust building. The system provides immediate productivity gains while establishing the foundation for more advanced AI-powered workflows in future NodeSpace releases.

---

*This hybrid approach acknowledges that different types of natural language operations require different processing strategies, optimizing for the common case while gracefully handling complex scenarios through intelligent routing and user confirmation workflows.*