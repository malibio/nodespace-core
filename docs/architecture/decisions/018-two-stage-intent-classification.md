# ADR-018: Two-Stage Intent Classification Pipeline

## Status
**Accepted** - January 2025

## Context

NodeSpace requires an efficient natural language interface that balances speed, accuracy, and user control. After analysis of intent classification approaches, we need to optimize the pipeline for clarity validation before expensive LLM operations.

### Key Requirements

1. **Clarity Validation**: Ensure prompts are clear and unambiguous before processing
2. **Cost Efficiency**: Minimize expensive LLM calls for ambiguous/multi-intent requests
3. **User Agency**: Give users control to clarify/simplify complex requests
4. **Fast Feedback**: Quick response for invalid/unclear prompts (<50ms)
5. **Future Flexibility**: Support fine-tuning when patterns are well-understood

### Problem Statement

Without a gatekeeper, ambiguous or multi-intent prompts reach the LLM stage, resulting in:
- Wasted compute on unclear requests
- Poor user experience from incorrect interpretations
- Difficulty in function calling when multiple intents detected
- No opportunity for users to refine their requests

## Decision

### Two-Stage Pipeline Architecture

**Stage 1: ML Gatekeeper (Clarity Validator)**
- **Purpose**: Validate prompt clarity and detect multi-intent before LLM processing
- **Technology**: Candle + DistilBERT (base model initially)
- **Speed**: <50ms classification
- **Output**: Clear/NeedsConfirmation/NeedsClarification/MultiIntent

**Stage 2: LLM Function Calling (Tool Selector)**
- **Purpose**: Decide which API/tool to call for validated prompts
- **Technology**: llama.cpp-rs + Gemma 3 4B-QAT (base model → LoRA fine-tuned)
- **Input**: Only clear, single-intent prompts from Stage 1
- **Output**: Function call with parameters

**Stage 3: Query Execution (Implementation Detail)**
- **Purpose**: Execute validated queries efficiently
- **Technology**: Text-to-SQL generation or direct function execution
- **Model**: Gemma 3 4B-QAT (same as Stage 2)
- **Output**: Database results or action confirmation

### Pipeline Flow

```
User Prompt
    ↓
┌─────────────────────────────────────┐
│ Stage 1: ML Gatekeeper              │
│ (DistilBERT - Candle)               │
│ - Confidence scoring                │
│ - Multi-intent detection            │
└─────────────────┬───────────────────┘
                  │
    ┌─────────────┼─────────────┐
    │             │             │
    ▼             ▼             ▼
┌───────┐   ┌──────────┐   ┌────────┐
│ Clear │   │ Medium   │   │ Low/   │
│ >0.85 │   │ 0.65-0.85│   │ Multi  │
└───┬───┘   └────┬─────┘   └───┬────┘
    │            │             │
    │      Ask user to    Ask user to
    │      confirm        simplify/
    │                     break down
    │            │             │
    └────────────┴─────────────┘
                 │
                 ▼ (only if clear)
┌─────────────────────────────────────┐
│ Stage 2: LLM Function Caller        │
│ (Gemma 3 4B-QAT - llama.cpp-rs)     │
│ - Tool/API selection                │
│ - Parameter extraction              │
└─────────────────┬───────────────────┘
                  │
                  ▼
┌─────────────────────────────────────┐
│ Stage 3: Query Execution            │
│ - Text-to-SQL generation            │
│ - Database operations (Turso)       │
│ - Or direct function execution      │
└─────────────────┬───────────────────┘
                  │
                  ▼
            Results/Action
```

### Base Model First, Fine-Tuning Later

**Phase 1: Base Models** (Initial Implementation)
- Start with **base DistilBERT** and **base Gemma 3**
- No fine-tuning initially
- Gather real-world usage data
- Understand actual user patterns and edge cases

**Phase 2: Fine-Tuning** (Future - When Patterns Clear)
- Fine-tune DistilBERT on NodeSpace-specific clarity patterns
- Fine-tune Gemma 3 with LoRA for function calling
- Use collected data from Phase 1
- A/B test before deployment

**Rationale**:
- Premature fine-tuning wastes effort on wrong patterns
- Base models provide acceptable performance initially
- Real usage data produces better training sets than synthetic data
- Delayed fine-tuning allows architecture validation first

## Intent Categories (Initial Set)

Seven core intent categories for Stage 1 classification:

1. **CREATE_SCHEMA** - Create new entity types, schemas
2. **RETRIEVE_DATA** - Query, search, retrieve operations
3. **UPDATE_RECORD** - Modify existing data
4. **DELETE_DATA** - Remove data
5. **AGGREGATE** - Analytics, calculations, summaries
6. **RAG_SEARCH** - Semantic/document search
7. **CREATE_WORKFLOW** - Workflow/automation creation

Plus meta-categories:
- **UNCLEAR** - Ambiguous prompt, low confidence
- **MULTI_INTENT** - Multiple intents detected

## Consequences

### Positive

✅ **Reduced LLM Costs**: Only clear prompts reach expensive LLM stage
✅ **Better UX**: Users get immediate feedback on unclear prompts
✅ **Higher Accuracy**: LLM processes only validated, clear requests
✅ **User Agency**: Users can refine requests before execution
✅ **Data Collection**: Logging enables future fine-tuning
✅ **Future Flexibility**: Can fine-tune when patterns are proven

### Negative

❌ **Two-hop Latency**: Clear prompts take 50ms + LLM time (vs direct LLM)
❌ **Additional Complexity**: Two models to maintain vs one
❌ **Potential Over-filtering**: Some valid prompts may be marked unclear

### Mitigations

- **Latency**: 50ms ML classification is negligible vs 2s LLM call
- **Complexity**: Candle patterns already established (embedding service)
- **Over-filtering**: Medium confidence tier allows user confirmation

## Implementation

### Stage 1: ML Gatekeeper (Issue #109)

```rust
pub struct IntentClassifier {
    model: DistilBERTModel,  // Base model initially
    cache: DashMap<String, ClassificationResult>,
    config: ClassifierConfig,
}

pub enum ClassificationResult {
    Clear {
        intent: IntentCategory,
        confidence: f32,  // >0.85
    },
    NeedsConfirmation {
        intent: IntentCategory,
        confidence: f32,  // 0.65-0.85
        alternatives: Vec<IntentCategory>,
    },
    NeedsClarification {
        confidence: f32,  // <0.65
    },
    MultiIntent {
        intents: Vec<IntentCategory>,
    },
}
```

### Stage 2: LLM Function Caller (Separate Issue)

```rust
pub struct FunctionCaller {
    llm: LlamaModel,  // Gemma 3 4B-QAT base → LoRA fine-tuned
    function_registry: FunctionRegistry,
}

impl FunctionCaller {
    pub async fn call_from_prompt(
        &self,
        prompt: &str,
        validated_intent: IntentCategory  // From Stage 1
    ) -> Result<FunctionCall> {
        // LLM decides which function to call
        // Uses validated intent as context
    }
}
```

### Stage 3: Query Execution Strategy

**Model Choice: Gemma 3 4B-QAT**
- **Parameters**: 4B (quantization-aware trained)
- **RAM**: 2.7GB (3x reduction via QAT)
- **Context**: 128k tokens (vs 32k for alternatives)
- **Capabilities**: Text-to-SQL, function calling, multi-step reasoning, multimodal

**Why Gemma 3 4B-QAT:**
- ✅ **Memory Efficient**: 2.7GB RAM (runs on any modern machine)
- ✅ **Large Context**: 128k tokens = full schema + examples + entity definitions
- ✅ **Multi-Purpose**: Handles SQL generation AND workflow automation (Phase 2)
- ✅ **Future-Proof**: Multimodal (text + images), 140+ languages
- ✅ **Strong Reasoning**: Excellent for workflow orchestration

**Alternatives Considered:**
- **SQLCoder 7B**: 7GB RAM, SQL-specialized but no workflow/function calling support
- **Qwen 2.5 Coder 7B**: 7GB RAM, 32k context, excellent code but no multimodal
- **Gemma 3 12B**: 12GB RAM, maximum quality but overkill for NodeSpace schema

**Decision Rationale:**
NodeSpace's schema (simple nodes table + JSON properties) doesn't require SQLCoder's specialization. Gemma 3 4B's "good enough" SQL generation + excellent reasoning makes it ideal for both current (SQL) and future (workflows) phases.

#### Text-to-SQL Execution Flow

```rust
// Complete pipeline
async fn process_natural_language_query(
    query: &str,
    intent_classifier: &IntentClassifier,
    gemma_model: &GemmaModel,
    turso: &TursoClient
) -> Result<Vec<Node>> {
    // Stage 1: Intent validation
    let classification = intent_classifier.classify(query).await?;

    if !classification.is_clear() {
        return Err(Error::NeedsClarification(classification));
    }

    // Stage 2: Determine if SQL query or direct function call
    let function_call = gemma_model.select_function(
        query,
        classification.intent
    ).await?;

    // Stage 3: Execute based on function type
    match function_call {
        FunctionCall::SqlQuery => {
            // Generate SQL from natural language
            let sql = gemma_model.generate_sql(
                query,
                schema_context,  // Fits in 128k context!
                classification
            ).await?;

            // Validate SQL (prevent injection)
            validate_sql(&sql)?;

            // Execute on Turso
            turso.query(&sql).await
        }
        FunctionCall::SemanticSearch { query, filters } => {
            // Hybrid vector + SQL search
            execute_hybrid_search(query, filters).await
        }
        FunctionCall::CreateWorkflow { description } => {
            // Future: Workflow generation
            gemma_model.generate_workflow(description).await
        }
    }
}
```

#### Why Text-to-SQL Instead of Custom DSL

**Alternative Considered: Composable Filter DSL**

We considered building a MongoDB/GraphQL-style filter language:
```rust
// Custom DSL approach (NOT chosen)
enum Filter {
    Eq { field: String, value: Value },
    Gt { field: String, value: Value },
    And { filters: Vec<Filter> },
    Or { filters: Vec<Filter> },
    // ... many more primitives
}
```

**Why We Chose Text-to-SQL:**
1. ✅ **Leverage Turso's Full Power**: SQL handles joins, aggregations, JSON queries natively
2. ✅ **LLM Does Translation**: No custom DSL to build/maintain
3. ✅ **128k Context**: Include full schema docs + examples + entity definitions
4. ✅ **Debuggable**: Log and inspect generated SQL
5. ✅ **User-Friendly**: Advanced users can view/edit SQL (future feature)
6. ✅ **No Reinvention**: SQL is a mature, proven query language

**Example Generated Queries:**

```sql
-- "Meetings from last week where we discussed budget"
SELECT * FROM nodes
WHERE json_extract(properties, '$.entity_type') = 'Meeting'
  AND date >= '2025-09-30'
  AND date <= '2025-10-06'
  AND (
    content LIKE '%budget%'
    OR json_extract(properties, '$.budget_discussed') = true
  );

-- "High priority tasks due this week"
SELECT * FROM nodes
WHERE node_type = 'task'
  AND json_extract(properties, '$.priority') = 'high'
  AND json_extract(properties, '$.due_date') >= '2025-10-07'
  AND json_extract(properties, '$.due_date') <= '2025-10-13'
ORDER BY json_extract(properties, '$.due_date') ASC;
```

#### Hybrid Vector + SQL Search

For semantic queries, combine vector similarity with SQL filters:

```rust
async fn semantic_search_with_filters(
    query: &str,
    sql_filters: Option<&str>
) -> Result<Vec<Node>> {
    // Generate query embedding
    let query_embedding = nlp_engine.generate_embedding(query).await?;

    // Hybrid search: vector similarity + SQL filters
    let sql = format!(
        "SELECT id, content,
                vec_distance_cosine(embedding_vector, ?) as similarity
         FROM nodes
         WHERE similarity > 0.7
           {additional_filters}
         ORDER BY similarity DESC
         LIMIT 20",
        additional_filters = sql_filters
            .map(|f| format!("AND {}", f))
            .unwrap_or_default()
    );

    turso.query(&sql, params![query_embedding]).await
}
```

**Primary Use Case**: Topic node discovery
**Example**: "Find notes about AI safety" → Vector search on topic embeddings

## Integration with Existing Architecture

### Candle Ecosystem (Consistency)
- Uses same Candle + ONNX patterns as embedding service (#108)
- Shares Metal GPU acceleration infrastructure
- Consistent feature flag patterns
- Unified DashMap caching approach

### LLM Infrastructure (Existing)
- Builds on ADR-008 (llama.cpp-rs + Gemma 3)
- Reuses existing model loading infrastructure
- Consistent with LoRA adapter management strategy
- Aligns with function registry pattern

### AI-Native Hybrid Approach (Philosophy)
- Maintains user agency through clarification prompts
- Progressive trust through confidence scoring
- Natural language as primary interface
- Structured confirmation before high-stakes actions

## Alternatives Considered

### Alternative 1: Direct LLM for Everything
- **Rejected**: Wastes compute on unclear prompts, no early feedback
- **Trade-off**: Simpler architecture but worse UX and higher costs

### Alternative 2: Fine-Tuned Models from Start
- **Rejected**: Premature optimization without understanding real usage
- **Trade-off**: Potentially higher accuracy but risks training on wrong patterns

### Alternative 3: Rule-Based Clarity Validation
- **Rejected**: Brittle, doesn't generalize to natural language variety
- **Trade-off**: Faster but poor coverage of edge cases

## References

- [BERT Implementation Guide](../components/bert-implementation-guide.md)
- [Hybrid Intent Classification](../components/hybrid-intent-classification.md)
- [ADR-008: NLP Engine Architecture](./nlp-engine-architecture.md)
- [AI Agent Architecture](../ai-agents/hybrid-llm-agent-architecture.md)
- [Model Selection Guide](../nlp/model-selection.md)
- Issue #109: ML Intent Classification Implementation
- Issue #108: Vector Embedding Service (Candle patterns reference)

## Revision History

- **2025-01-07**: Initial version - Two-stage pipeline with base models first
- **2025-10-08**: Added Stage 3 (Query Execution), specified Gemma 3 4B-QAT, documented Text-to-SQL approach
