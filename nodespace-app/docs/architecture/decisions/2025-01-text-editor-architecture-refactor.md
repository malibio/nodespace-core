# ADR-003: Text Editor Architecture Enhancement

**Date**: January 2025 (Updated August 2025)  
**Status**: Approved (Revised)  
**Participants**: Development Team, Senior Architect, Frontend Expert

## Context

Our current text editor architecture has fundamental issues that prevent reliable functionality and limit scalability:

### Technical Problems Identified
1. **Reactivity Anti-Patterns**: Direct prop mutation in Svelte 5 components breaking parent-child binding
2. **Mixed Responsibilities**: Components handling both UI logic and business logic
3. **Performance Issues**: Deep tree traversal on every update, no virtualization
4. **Maintenance Challenges**: Scattered logic across multiple components
5. **Extension Limitations**: Difficult to add AI integration, collaborative editing, backlinking, or plugins

### Specific Bugs
- **Backspace Node Combination**: Nodes disappear instead of combining content
- **Content Updates**: UI not reflecting data model changes
- **Memory Leaks**: Improper event cleanup in complex component trees

### Expert Analysis & Research
- **Frontend Expert**: Identified Svelte 5 reactivity violations as root cause
- **Senior Architect**: Confirmed need for service-oriented architecture with clear separation of concerns
- **Logseq Research**: Analyzed dual-representation patterns for hybrid markdown editing
- **ProseMirror Evaluation**: Determined ProseMirror cannot handle NodeSpace's unique indented indicator requirements

### Key Finding: NodeSpace's Unique Value Proposition
Research revealed that **NodeSpace's custom contenteditable approach with indented node indicators is architecturally superior** to ProseMirror-based solutions. Most block editors (Notion, Craft) use left-aligned gutters because ProseMirror's architecture cannot handle true hierarchical visual indicators.

## Decision

We will implement an **enhanced contenteditable architecture** that preserves NodeSpace's unique hierarchical design while applying proven patterns from Logseq:

### Enhanced ContentEditable Architecture

```
┌─────────────────────────────────────────┐
│                Services                 │
│  - ContentProcessor (Logseq patterns)  │
│  - NodeManager (Enhanced)              │
│  - BacklinkService (New)               │
│  - DecorationService (New)             │
│  - EventBus                            │
│  - AIService (future)                  │
└─────────────────────────────────────────┘
                    ↕ Events
┌─────────────────────────────────────────┐
│             State Management            │
│  - documentStore (Dual-representation) │
│  - selectionStore                      │
│  - backlinkStore (New)                 │
│  - Derived reactive state              │
└─────────────────────────────────────────┘
                    ↕ Props/Events
┌─────────────────────────────────────────┐
│          UI Components (Enhanced)       │
│  - BaseNode.svelte (Preserve hierarchy)│
│  - TextNode.svelte (Enhanced markdown) │
│  - BacklinkRenderer.svelte (New)       │
│  - DecorationOverlay.svelte (New)      │
└─────────────────────────────────────────┘
```

### Key Architectural Principles

1. **Preserve Unique Hierarchical Design**
   - Keep indented node indicators that follow content depth
   - Maintain NodeSpace's superior visual hierarchy
   - Enhance existing contenteditable foundation

2. **Dual-Representation Pattern (from Logseq)**
   - Source representation: Raw markdown with backlinks
   - Rendered representation: Parsed AST for display
   - Seamless switching between edit/view contexts

3. **Service-Oriented Business Logic**
   - ContentProcessor handles markdown ↔ AST conversion
   - BacklinkService manages `[[wikilinks]]` and decorations
   - NodeManager centralizes hierarchy operations with proven patterns from `nodespace-core-logic`
   - EventBus enables loose coupling

4. **Enhanced Reactive State Management**
   - Centralized stores with Svelte 5 `$state`
   - Bidirectional link graph management
   - Real-time decoration updates
   - Performance-optimized reactivity patterns

5. **Database Persistence Compatibility**
   - Maintain compatibility with existing `nodespace-core-logic` ordering strategy
   - Support single-pointer sibling navigation (`before_sibling`) with option to optimize
   - Preserve root hierarchy optimization for O(1) query performance
   - Type-segmented metadata preservation from existing NodeOperations

6. **Extensibility for Future Features**
   - Plugin architecture foundation
   - AI integration points
   - Collaborative editing readiness
   - Rich decoration system

## Implementation Strategy

### Phase 1: Enhanced Service Layer (3 weeks)
- **ContentProcessor**: Implement Logseq-style dual-representation (markdown ↔ AST)
- **NodeManager**: Centralize node operations with proper reactivity (fixes backspace bug)
  - Migrate hierarchy management from `nodespace-core-logic`
  - Implement single-pointer sibling ordering (`before_sibling` only initially)
  - Add content extraction utilities and type-segmented metadata
- **BacklinkService**: Basic `[[wikilink]]` parsing and resolution
- **EventBus**: Type-safe event system for service communication

### Phase 2: Backlinking Foundation (2 weeks)
- **Bidirectional Link Graph**: Build link index and resolution system
- **Real-time Link Detection**: Parse `[[links]]` as user types
- **Basic Decorations**: Simple link highlighting and hover states
- **Link Navigation**: Click-to-navigate functionality

### Phase 3: Rich Decorations & Context (3 weeks)
- **Node Type Decorations**: Task status, document previews, metadata
- **Contextual Information**: Hover overlays with node-specific data
- **Advanced Link Types**: `((block-refs))`, `@mentions`, `![[embeds]]`
- **Multi-Level Embeddings**: Individual, contextual, and hierarchical embeddings from `nodespace-core-types`
- **Performance Optimization**: Viewport-based processing, debounced updates

### Phase 4: AI Integration Preparation (2 weeks)
- **Service Extension Points**: Interfaces for AI content enhancement
- **Smart Suggestions**: Architecture for auto-linking and completions
- **Collaboration Readiness**: Event-driven updates for real-time sync
- **Plugin Foundation**: Extensible decoration and service systems

## Consequences

### Positive
- ✅ **Preserves Unique Value**: Maintains NodeSpace's superior indented node indicators
- ✅ **Fixes Current Bugs**: Solves backspace combination and reactivity issues
- ✅ **Enables Backlinking**: Adds sophisticated knowledge management capabilities
- ✅ **Future-Proof Architecture**: Clear extension points for AI, collaboration, plugins
- ✅ **Performance Optimized**: Leverages proven patterns from Logseq research
- ✅ **Better Testing**: Service-oriented design with pure functions
- ✅ **Reduced Complexity**: Builds on existing foundation rather than replacing it

### Negative
- ❌ **Development Time**: 10 weeks of enhanced development work
- ❌ **Migration Complexity**: Adding new services while preserving existing functionality
- ❌ **Learning Curve**: Team needs to understand dual-representation patterns

### Migration Risks and Mitigation
- **Risk**: Breaking existing hierarchical display
- **Mitigation**: Preserve all existing UI components, enhance incrementally
- **Risk**: Performance regression with backlinking
- **Mitigation**: Implement viewport-based processing and caching strategies
- **Risk**: Complexity of dual-representation
- **Mitigation**: Follow proven Logseq patterns with comprehensive testing

## Alternative Approaches Considered

### 1. ProseMirror Migration
**Rejected**: Cannot handle NodeSpace's unique indented indicator requirements. Research confirmed ProseMirror's architectural limitations for hierarchical visual design.

### 2. Quick Fixes Only
**Rejected**: Would not solve underlying architectural problems or enable backlinking roadmap

### 3. Complete Rewrite with Different Framework
**Rejected**: Would lose NodeSpace's unique value proposition and working hierarchical system

### 4. Gradual Component Updates Without Service Layer
**Rejected**: Mixed architecture would persist, making backlinking integration complex

## Success Metrics

### Technical
- [ ] Zero reactivity bugs in text editing
- [ ] Backspace node combination works reliably
- [ ] Hierarchical node indicators preserved and enhanced
- [ ] `[[Wikilinks]]` parse and resolve in real-time
- [ ] Rich decorations display contextual node information
- [ ] 1000+ node documents with backlinks load in < 2 seconds
- [ ] Keystroke latency < 16ms (60fps) with live link detection

### Architectural
- [ ] Clear separation between services, stores, and UI components
- [ ] Dual-representation pattern implemented correctly
- [ ] Bidirectional link graph maintains consistency
- [ ] 100% test coverage for core services
- [ ] Easy to add new decoration types and link formats
- [ ] Plugin system architectural foundation

### User Experience
- [ ] Seamless backlinking with contextual information
- [ ] Intuitive node type decorations (task status, previews, etc.)
- [ ] Preserved hierarchical editing experience
- [ ] Fast, responsive link navigation
- [ ] Rich hover states and contextual previews

### Developer Experience
- [ ] Comprehensive TypeScript types for all services
- [ ] Clear debugging capabilities for link resolution
- [ ] Excellent error recovery and validation
- [ ] Maintainable service-oriented architecture

## Implementation Tracking

### GitHub Issues (To Be Updated)
- **Epic**: #69 Text Editor Architecture Enhancement (Updated)
- **Phase 1.1**: Enhanced ContentProcessor with Dual-Representation
- **Phase 1.2**: NodeManager Service with Improved Reactivity
- **Phase 1.3**: Migrate Core Logic Components from `nodespace-core-logic`
- **Phase 1.4**: BacklinkService Implementation
- **Phase 1.5**: EventBus System
- **Phase 2.1**: Bidirectional Link Graph
- **Phase 2.2**: Real-time Link Detection
- **Phase 3.1**: Rich Decoration System
- **Phase 3.2**: Contextual Node Information

### Components to Migrate from Existing Codebase
From `nodespace-core-logic` and `nodespace-core-types`:
- **Hierarchy Management**: Complete hierarchy computation system with caching
- **Node Operations**: Unified upsert with type-segmented metadata preservation  
- **Content Extraction**: Smart content parsing utilities with fallbacks
- **Multi-Level Embeddings**: Individual, contextual, and hierarchical embedding support
- **Performance Optimizations**: Root hierarchy optimization, caching strategies
- **Sibling Navigation**: Single-pointer approach (`before_sibling`) with upgrade path

### Documentation
- **Enhanced Implementation Plan**: `docs/architecture/development/text-editor-architecture-refactor.md`
- **Backlinking Architecture**: `docs/architecture/features/backlinking-system.md`
- **Decoration System**: `docs/architecture/features/decoration-system.md`
- **Troubleshooting**: `docs/troubleshooting/backspace-node-combination-issue.md`

## Future Enhancements Enabled

### Backlinking & Knowledge Management
- Advanced link types: `((block-refs))`, `@mentions`, `![[embeds]]`
- Graph visualization and navigation
- Smart link suggestions and auto-completion
- Link context and relationship analysis

### AI Integration
- Content enhancement with link-aware context
- Smart backlinking suggestions
- Document summarization with link preservation
- Intelligent node relationship detection

### Collaborative Editing
- Real-time link synchronization
- Collaborative graph building
- Shared node decorations and annotations
- Multi-user link resolution

### Plugin System
- Custom decoration types
- External link integrations (APIs, databases)
- Extensible node type definitions
- Third-party service connections

## Timeline

- **Start Date**: August 2025
- **Phase 1 Complete**: Week 3 (Enhanced Service Layer)
- **Phase 2 Complete**: Week 5 (Backlinking Foundation)
- **Phase 3 Complete**: Week 8 (Rich Decorations & Context)
- **Phase 4 Complete**: Week 10 (AI Integration Preparation)

## Approval

- ✅ **Technical Review**: Senior Architect approval
- ✅ **Frontend Review**: Frontend Expert approval  
- ✅ **Logseq Research**: Comprehensive analysis of proven patterns
- ✅ **ProseMirror Evaluation**: Technical assessment confirming contenteditable superiority
- ✅ **Architecture Review**: Preserves NodeSpace's unique value while enabling backlinking
- ✅ **Project Priority**: High priority for text editor reliability and knowledge management features

---

**Decision Rationale**: Research confirmed that NodeSpace's custom contenteditable approach with indented indicators is architecturally superior to ProseMirror-based solutions for hierarchical editing. By enhancing this foundation with Logseq's proven dual-representation patterns, we can solve current technical issues while building sophisticated backlinking capabilities that maintain NodeSpace's unique visual design.

**Next Steps**: Update GitHub issues to reflect enhanced contenteditable approach, then begin Phase 1 implementation with enhanced service layer.