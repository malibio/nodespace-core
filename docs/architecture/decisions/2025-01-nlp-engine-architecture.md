# ADR-008: NLP Engine Architecture for Natural Language API Operations

## Status
**Accepted** - September 2025

## Context

NodeSpace aims to be an "AI-Native Knowledge Management System" where users can perform operations through natural language. After extensive analysis and experimentation with intent classification systems, we needed to determine the optimal architecture for scaling natural language to API function mapping across the entire NodeSpace system.

### Key Requirements

1. **Local-First**: All AI processing must work offline with embedded models
2. **Scalable Function Mapping**: Support for exposing many API functions via natural language
3. **Fast Iteration**: Ability to add/modify API functions without extensive retraining cycles
4. **Trust & Control**: Users must maintain agency over high-stakes operations
5. **Desktop-Native**: Integration with Tauri/Svelte/Rust architecture

### Analysis Conducted

- Evaluated current llama.cpp approach vs framework alternatives (LangChain, Rig.rs, Candle-Core)
- Analyzed schema context impact on model confidence levels
- Assessed complexity of pure prompt engineering vs structured approaches
- Considered agentic AI vs human-in-the-loop hybrid models

## Decision

### Core NLP Engine Architecture

**Technology Stack:**
- **Model Runtime**: llama.cpp-rs (via llama-cpp-2 crate)
- **Base Models**: Gemma 3 12B / Gemma 3 4B (Q4_K_M quantization)
- **Fine-tuning Framework**: MLX (Metal acceleration on macOS)
- **Training Data**: Auto-generated from function registry
- **Deployment**: GGUF format for efficient local inference

**Approach**: Structured Function Registry + Fine-tuning (NOT complex prompt engineering)

### Function Registry Pattern

Instead of complex intent classification and manual prompt engineering, we use automated function discovery:

```rust
// Automatic function registration
#[api_function("Creates a new text node with content")]
pub async fn create_text_node(content: String, parent_id: Option<String>) -> Result<String, Error> {
    // Implementation
}

#[api_function("Updates a specific field on an entity")]
pub async fn update_entity_field(entity_id: String, field: String, value: serde_json::Value) -> Result<bool, Error> {
    // Implementation
}
```

**Benefits:**
- Eliminates manual prompt engineering for each function
- Automatically generates training data from API signatures
- Keeps training data in sync with API changes
- Enables systematic fine-tuning rather than ad-hoc prompts

### Automated Training Pipeline

```
API Functions → Function Registry → Training Data Generation → MLX Fine-tuning → Model Deployment
```

**Training Data Generation:**
- Compile-time discovery of `#[api_function]` attributes
- Automatic generation of natural language variations
- Continuous integration with nightly model retraining
- Quality gates with accuracy thresholds (>95%)

### Hybrid AI-Native Approach

**Philosophy**: "AI-Native, not AI-Only"

Rather than pure agentic automation, we use a hybrid approach that maintains user control:

1. **Natural Language Input**: Users express intent in conversational form
2. **AI Function Translation**: Models convert to structured function calls
3. **Confirmation Interface**: Users review and approve operations
4. **Progressive Trust**: Gradually enable auto-execution for proven patterns

## Alternatives Considered

### Framework Approaches (Rejected)

**LangChain/Rig.rs/Candle-Core**: Would provide built-in function calling but:
- Requires cloud dependencies (contradicts local-first vision)
- Vendor lock-in and usage-based pricing
- Complex integration with existing Tauri architecture
- Less control over fine-tuning and model behavior

**Assessment**: Framework benefits don't outweigh architectural constraints

### Pure Agentic Automation (Rejected)

**Full AI Agent**: Complete automation without human confirmation:
- Users not ready to delegate high-stakes operations (financial, data modification)
- Trust gap too large for production knowledge management
- Error amplification risk too high
- Compliance/control requirements in enterprise environments

**Assessment**: Market reality shows users want AI assistance, not replacement

### Complex Intent Classification (Replaced)

**Current Approach**: 7-category intent classification with extensive prompt engineering:
- High maintenance overhead for each new function
- Brittle to API changes
- Requires manual training data creation
- Doesn't scale to many functions

**Assessment**: Function registry approach is more systematic and maintainable

## Implementation Strategy

### MVP Scope (3-4 months)

**Complete Phase 1: Function Calling Foundation**
- Reliable CRUD operations via natural language
- 95%+ accuracy on basic function patterns
- Automated training pipeline

**Simple Phase 2 & 3: Preview Features**
- Basic entity resolution with clarification dialogs
- Pre-built workflow templates
- Confirmation interfaces (Cursor-style diffs)

### UI/UX Integration

**@ Mention System**: 
- Users can explicitly reference entities (`@john_smith`) 
- Reduces AI ambiguity while maintaining natural language flow
- Familiar pattern from Slack/Discord

**Confirmation Forms**:
- Show structured operations before execution
- Build user trust and understanding
- Progressive automation ("accept all" after confidence builds)

**Visual + Natural Language Workflows**:
- Visual cards for workflow structure
- Natural language for step descriptions
- Inspectable and modifiable by users

## Success Metrics

### Technical Metrics
- **Function Call Accuracy**: >95% for common operations
- **Training Pipeline Speed**: <30 minutes from API change to model update
- **Model Performance**: <2 second response time for typical queries

### User Experience Metrics
- **Task Completion Rate**: >80% of natural language requests successfully executed
- **User Trust**: >90% of operations approved in confirmation interface
- **Adoption**: Progressive increase in auto-execution for routine operations

## Risks and Mitigations

### Risk: Local Model Limitations
**Mitigation**: Hybrid UI approach provides fallbacks and user control

### Risk: Training Data Quality
**Mitigation**: Automated generation ensures consistency; continuous monitoring of accuracy

### Risk: Entity Resolution Complexity
**Mitigation**: @ mention system and smart search for explicit disambiguation

### Risk: Scope Creep
**Mitigation**: Focus on reliable function calling first; resist adding "intelligence" until foundation is solid

## Future Considerations

### Model Evolution
- Monitor Gemma 4+ releases for improved reasoning capabilities
- Evaluate quantization techniques (Q5_K_M, Q6_K) as hardware improves
- Consider multi-model approaches for specialized tasks

### Advanced Features (Post-MVP)
- Cross-function operation chaining
- Context-aware entity resolution
- Complex multi-step workflow automation
- Integration with external APIs (with user approval)

## References

- [System Overview](../core/system-overview.md)
- [AI Integration Specification](../components/ai-integration.md)
- [Confidence and Ambiguity Detection System](../components/confidence-ambiguity-system.md)
- [AI-Native Hybrid Approach](../components/ai-native-hybrid-approach.md) 
- [NLP Implementation Roadmap](../components/nlp-implementation-roadmap.md)
- NodeSpace Intent Classification Experiment Results (Sep 2025)
- Schema Context Impact Testing Results (Sep 2025)

---

**Decision Made By**: Architecture Team  
**Date**: September 2025  
**Review Date**: December 2025 (Post-MVP Assessment)