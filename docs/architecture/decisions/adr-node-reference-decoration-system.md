# ADR-005: Node Reference Decoration System

## Status
Proposed

## Context

NodeSpace requires a universal system for referencing any type of node (users, tasks, documents, dates, custom entities) from within content. The system must support:

1. **Universal Interface**: Single consistent method to reference any node type
2. **Rich Visual Decoration**: Each node type should control how its references appear
3. **Content Integration**: Seamless integration with the existing text editor and ContentProcessor
4. **Extensibility**: Easy to add new node types without breaking existing references
5. **Discoverability**: Users should be able to explore available nodes through autocomplete

### Previous Approaches Considered

**Option 1: Wikilink Syntax (`[[node-name]]`)**
- Pros: Familiar from wikis, simple syntax
- Cons: Limited to text-based references, no rich decoration, difficult to distinguish node types

**Option 2: Complex Node Reference Interface**
- Pros: Rich metadata, type-specific handling
- Cons: Complex data structures, over-engineered for simple use cases

**Option 3: Typed Reference Syntax (`@user:john.doe`, `@task:deploy`)**
- Pros: Clear type distinction, rich possibilities
- Cons: Complex syntax, requires type prefixes, harder to type

## Decision

We will implement a **Node Reference Decoration System** with the following architecture:

### 1. Universal `@` Trigger Interface
- **`@` keystroke** triggers autocomplete modal for all node types
- **Single interface** for referencing users, tasks, documents, dates, and custom entities
- **Search and filter** existing nodes by content/name as user types

### 2. URI-Based Storage Format
- Store references as **standard markdown links**: `[display-text](nodespace://uuid)`
- Use **`nodespace://` URI scheme** to identify node references vs regular web links
- Extract **UUID from URI** for node lookup and resolution

### 3. Content-Driven Decoration System
- **BaseNode** provides default decoration using `content` property
- **Derived node types** override `decorateReference()` method to control appearance
- **No artificial display names** - each node type interprets its `content` as appropriate

### 4. Extensible Rendering Architecture
```typescript
// Base pattern - all nodes inherit this behavior
class BaseNode {
  id: string;
  content: string;  // Raw content - interpretation up to derived types
  
  decorateReference(): string {
    return `<a href="nodespace://${this.id}" class="ns-noderef">${this.content}</a>`;
  }
}

// Derived nodes control their own reference appearance
class TaskNode extends BaseNode {
  decorateReference(): string {
    const checkbox = this.isCompleted() ? '✅' : '☐';
    return `<a href="nodespace://${this.id}" class="ns-noderef ns-task">
      ${checkbox} ${this.content}
    </a>`;
  }
}

class UserNode extends BaseNode {
  decorateReference(): string {
    return `<a href="nodespace://${this.id}" class="ns-noderef ns-user">
      👤 ${this.content}
    </a>`;
  }
}
```

### 5. ContentProcessor Integration
- **Extend existing link detection** to specially handle `nodespace://` URIs
- **Node lookup service** resolves UUIDs to node instances
- **Decoration rendering** calls `node.decorateReference()` for custom appearance
- **Graceful fallback** for missing or deleted nodes

## Rationale

### Why This Approach?

**1. Leverages Web Standards**
- Uses standard markdown link syntax: `[text](url)`
- Compatible with any markdown renderer
- Future-proof with URI scheme extensibility

**2. Maximum Flexibility**
- Each node type has complete control over its reference appearance
- Content-driven approach respects node semantics
- No predetermined constraints on decoration

**3. Simple Foundation, Rich Extensions**
- BaseNode provides sensible default (content as link text)
- Derived types can add checkboxes, status indicators, avatars, etc.
- Easy to add new node types without architectural changes

**4. Familiar User Experience**
- `@` trigger pattern familiar from social media (Discord, Slack, Twitter)
- Autocomplete provides discoverability
- Standard link behavior for navigation

**5. Future-Ready Architecture**
- URI scheme supports query parameters: `nodespace://uuid?view=compact`
- Decoration system supports interactive elements
- Plugin architecture for custom node types

### Content-Driven Philosophy

Rather than artificial "display names," each node type uses its actual `content`:

```typescript
// Examples of content interpretation
const taskNode = new TaskNode();
taskNode.content = "Deploy app to production";
// Renders as: ☐ Deploy app to production

const userNode = new UserNode(); 
userNode.content = "John Doe";
// Renders as: 👤 John Doe

const dateNode = new DateNode();
dateNode.content = "2024-01-15";
// Renders as: 📅 January 15, 2024
```

## Consequences

### Positive Consequences

**1. Extensible Architecture**
- New node types can be added without modifying core system
- Each type controls its own visual representation
- Plugin system naturally supports custom decorations

**2. Familiar User Experience**
- `@` trigger universally understood
- Standard markdown storage format
- Consistent across all node types

**3. Technical Simplicity**
- Builds on existing ContentProcessor link detection
- Minimal interface complexity
- Standard web technologies (URIs, CSS, HTML)

**4. Future-Proof Design**
- URI scheme allows for rich extensions
- Decoration system supports interactive elements
- Ready for collaboration features (user status, real-time updates)

### Negative Consequences

**1. Node Lookup Dependency**
- Requires node registry/lookup service for decoration
- Performance considerations for large numbers of references
- Need graceful handling of missing nodes

**2. Decoration Complexity**
- Each node type must implement decoration logic
- Potential for inconsistent visual design across types
- CSS complexity for rich decorations

**3. Learning Curve**
- Different from traditional `[[wikilink]]` approach
- Developers must understand decoration patterns
- Type-specific decoration requirements

### Neutral Consequences

**1. Storage Format Change**
- Moving from potential `[[syntax]]` to markdown links
- Requires migration if existing wikilinks present
- Benefits outweigh migration costs

**2. Rendering Performance**
- Node lookups required during rendering
- Caching strategies needed for frequently referenced nodes
- Trade-off between rich decoration and performance

## Implementation Plan

### Phase 1: Foundation (2-3 hours)
1. **ContentProcessor Enhancement**: Detect `nodespace://` URIs in links
2. **BaseNode Decoration**: Implement default `decorateReference()` method
3. **Node Lookup Service**: Simple UUID → node resolution
4. **Basic CSS Framework**: `.ns-noderef` styling foundation

### Phase 2: Autocomplete Integration (1-2 days)
1. **`@` Keystroke Detection**: Trigger autocomplete in contenteditable
2. **Node Search Interface**: Filter and display available nodes
3. **Link Insertion**: Insert `[content](nodespace://uuid)` on selection
4. **Modal Positioning**: Cursor-relative autocomplete placement

### Phase 3: Rich Decorations (2-3 days)
1. **Task Node Decorations**: Checkboxes, status indicators, due dates
2. **User Node Decorations**: Avatars, online status, role indicators
3. **Date Node Decorations**: Calendar icons, day/time formatting
4. **Document Node Decorations**: File type icons, preview thumbnails

### Phase 4: Advanced Features (1 week)
1. **Interactive Decorations**: Click handlers, status toggles
2. **Real-time Updates**: Dynamic status changes (user online/offline)
3. **Context-Aware Rendering**: Different decoration based on context
4. **Performance Optimization**: Caching, lazy loading, viewport-based rendering

## Related Decisions

- **ADR-001**: Always-editing mode enables seamless `@` trigger integration
- **ADR-002**: Component composition supports decoration extensibility  
- **Text Editor Architecture Refactor**: ContentProcessor provides foundation for link processing

## Future Considerations

### Potential Extensions

**1. Rich Query Parameters**
```
nodespace://task-uuid?view=compact&showDueDate=true
nodespace://user-uuid?view=avatar-only
```

**2. Interactive Decorations**
```typescript
// Task nodes with clickable checkboxes
decorateReference(): string {
  return `<button onclick="toggleTask('${this.id}')" class="ns-task-ref">
    ${this.checkbox} ${this.content}
  </button>`;
}
```

**3. Contextual Decorations**
- Different appearance based on where reference appears
- Adaptive decoration based on available space
- Theme-aware styling for different document contexts

### Migration Strategy

If existing `[[wikilink]]` content exists:
1. **Detection Phase**: Scan content for `[[pattern]]` syntax
2. **Resolution Phase**: Attempt to resolve to existing nodes
3. **Conversion Phase**: Convert to `[text](nodespace://uuid)` format
4. **Validation Phase**: Verify all references resolve correctly

---

**Date**: January 2025  
**Author**: Development Team  
**Reviewers**: Architecture Team  
**Next Review**: After Phase 2 implementation