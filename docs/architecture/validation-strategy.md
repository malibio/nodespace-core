# Validation Strategy Architecture

**Status**: Active
**Last Updated**: 2025-10-12
**Related Issues**: #231

## Overview

NodeSpace implements a **layered validation architecture** that separates user experience concerns from data integrity requirements. This document defines the validation contract between frontend and backend layers.

## Architectural Principles

### 1. Separation of Concerns

```
┌─────────────────────────────────────────────────┐
│              FRONTEND LAYER                     │
│  Responsibility: User Experience                │
│  • Optimistic UI updates                        │
│  • Placeholder node management                  │
│  • Client-side validation (immediate feedback)  │
│  • Memory-only temporary states                 │
└─────────────────────────────────────────────────┘
                      ↓
              Frontend → Backend
           (Persistence Boundary)
                      ↓
┌─────────────────────────────────────────────────┐
│              BACKEND LAYER                      │
│  Responsibility: Data Integrity                 │
│  • Authoritative validation                     │
│  • Database constraint enforcement              │
│  • Persistence guarantees                       │
│  • Cross-service consistency                    │
└─────────────────────────────────────────────────┘
```

### 2. Clear Contract

**Frontend Contract**:
- MAY hold invalid nodes in memory for UX optimization
- MAY perform optimistic updates before validation
- MUST NOT persist invalid data to backend
- MUST handle backend validation failures gracefully

**Backend Contract**:
- MUST validate all data at persistence boundary
- MUST be the authoritative source of validation truth
- MUST enforce database integrity constraints
- MUST return clear error messages for validation failures

## Implementation Details

### Frontend Validation (Reactive)

**Location**: `packages/desktop-app/src/lib/services/`

**Purpose**: Immediate user feedback, UX optimization

**Example**: Empty Placeholder Nodes

```typescript
// Frontend creates placeholder when user presses Enter
const placeholder = {
  id: generateId(),
  content: "",  // Empty content - valid in frontend
  nodeType: "text",
  // ... other fields
};

// Store in memory-only reactive state
reactiveNodeService.createPlaceholder(placeholder);

// Only send to backend AFTER user adds content
if (placeholder.content.trim()) {
  await backend.createNode(placeholder);
}
```

**Characteristics**:
- **Fast**: No network round-trip
- **Optimistic**: Assumes success, handles failures later
- **UX-First**: Prioritizes responsiveness over strict validation
- **Temporary**: Can hold invalid states briefly

### Backend Validation (Authoritative)

**Location**: `packages/core/src/behaviors/mod.rs`, `packages/core/src/services/`

**Purpose**: Data integrity, database consistency

**Example**: Empty Content Rejection

```rust
fn validate(&self, node: &Node) -> Result<(), NodeValidationError> {
    // Backend validates data integrity: empty nodes are rejected
    if is_empty_or_whitespace(&node.content) {
        return Err(NodeValidationError::MissingField(
            "Text nodes must have content".to_string(),
        ));
    }
    Ok(())
}
```

**Characteristics**:
- **Strict**: Enforces all business rules and constraints
- **Consistent**: Same validation for all clients (web, desktop, API)
- **Durable**: Validated data is guaranteed correct in database
- **Unicode-Aware**: Handles edge cases (zero-width spaces, etc.)

## Validation Layers

### Layer 1: Type-Level Validation (Rust Type System)

**When**: Compile-time
**Where**: Rust type definitions
**Example**: Required fields, enums

```rust
pub struct Node {
    pub id: String,              // Required by type system
    pub node_type: String,       // Required by type system
    pub content: String,         // Required by type system
    // ...
}
```

**Guarantees**: Fields exist, types are correct

### Layer 2: Behavior Validation (Runtime)

**When**: Before database persistence
**Where**: `NodeBehavior::validate()` trait implementations
**Example**: Empty content, date format, task status

```rust
impl NodeBehavior for TextNodeBehavior {
    fn validate(&self, node: &Node) -> Result<(), NodeValidationError> {
        // Content validation with Unicode handling
        if is_empty_or_whitespace(&node.content) {
            return Err(/* ... */);
        }
        Ok(())
    }
}
```

**Guarantees**: Business rules enforced, semantic correctness

### Layer 3: Database Constraints

**When**: Database insertion/update
**Where**: SQLite schema, foreign keys
**Example**: UNIQUE constraints, FOREIGN KEY relationships

```sql
CREATE TABLE nodes (
    id TEXT PRIMARY KEY,
    node_type TEXT NOT NULL,
    content TEXT NOT NULL,
    parent_id TEXT REFERENCES nodes(id) ON DELETE CASCADE,
    -- ...
);
```

**Guarantees**: Referential integrity, uniqueness

## Unicode Validation

### Problem

Standard whitespace checking (`trim().is_empty()`) misses Unicode edge cases:
- Zero-width spaces (U+200B, U+200C, U+200D)
- Zero-width no-break space / BOM (U+FEFF)

### Solution

Custom validation helper that explicitly checks for invisible characters:

```rust
fn is_empty_or_whitespace(content: &str) -> bool {
    content.chars().all(|c| {
        c.is_whitespace()           // Standard whitespace (space, tab, \n, U+00A0, U+2028, etc.)
            || c == '\u{200B}'      // Zero-width space
            || c == '\u{200C}'      // Zero-width non-joiner
            || c == '\u{200D}'      // Zero-width joiner
            || c == '\u{FEFF}'      // Zero-width no-break space (BOM)
    })
}
```

**Rationale**: Rust's `char::is_whitespace()` follows Unicode's "White_Space" property, which excludes zero-width characters (they're meant for text shaping). For content validation, we treat them as empty.

## Validation Flow Examples

### Example 1: Valid Node Creation

```
1. User presses Enter
   → Frontend creates placeholder { content: "" } in memory

2. User types "Hello"
   → Frontend updates placeholder { content: "Hello" }

3. Frontend sends to backend
   → Backend validates: is_empty_or_whitespace("Hello") = false ✅

4. Backend persists to database
   → Node saved successfully
```

### Example 2: Invalid Node Rejection

```
1. User presses Enter
   → Frontend creates placeholder { content: "" } in memory

2. User types zero-width space (U+200B)
   → Frontend updates placeholder { content: "\u{200B}" }

3. Frontend sends to backend
   → Backend validates: is_empty_or_whitespace("\u{200B}") = true ❌

4. Backend returns validation error
   → Frontend shows error message
   → Placeholder remains in memory (can be edited)
```

### Example 3: Rapid Editing (No Network Spam)

```
1. User types "H" → Frontend debounces
2. User types "e" → Frontend debounces
3. User types "l" → Frontend debounces
4. User types "l" → Frontend debounces
5. User types "o" → Frontend debounces

After 300ms idle:
   → Frontend sends single request { content: "Hello" }
   → Backend validates once
   → Single database write
```

## Edge Cases & Error Handling

### Case 1: Network Failure During Persistence

**Scenario**: Backend validation succeeds, but database write fails

**Handling**:
```typescript
try {
  const nodeId = await backend.createNode(nodeData);
  // Update frontend state with persisted ID
  reactiveNodeService.markPersisted(nodeId);
} catch (error) {
  if (error instanceof ValidationError) {
    // Show validation error to user
    showError("Invalid content: " + error.message);
  } else if (error instanceof NetworkError) {
    // Queue for retry
    persistenceQueue.enqueue(nodeData);
  }
}
```

### Case 2: Stale Frontend State

**Scenario**: Another client deletes node while user is editing

**Handling**:
```typescript
// Backend returns 404
try {
  await backend.updateNode(nodeId, changes);
} catch (error) {
  if (error instanceof NotFoundError) {
    // Remove from frontend state
    reactiveNodeService.removeNode(nodeId);
    showError("Node was deleted by another user");
  }
}
```

### Case 3: Concurrent Validation Changes

**Scenario**: Backend validation rules change (deployment)

**Handling**:
- Backend returns new validation errors
- Frontend handles gracefully (generic error handling)
- User re-edits content to meet new requirements
- **No breaking change**: Frontend doesn't couple to specific validation rules

## Testing Strategy

### Frontend Tests (UX Validation)

**Location**: `packages/desktop-app/src/tests/`

**Focus**: User interaction flows, optimistic updates

```typescript
test('creates placeholder on Enter key', async () => {
  await userEvent.keyboard('{Enter}');

  // Placeholder should exist in memory
  const placeholder = reactiveNodeService.getNode(newNodeId);
  expect(placeholder.content).toBe('');

  // Should NOT be in backend
  const persisted = await backend.getNode(newNodeId);
  expect(persisted).toBeNull();
});
```

### Backend Tests (Data Integrity)

**Location**: `packages/core/src/behaviors/mod.rs` (unit tests)

**Focus**: Validation logic, edge cases

```rust
#[test]
fn test_empty_node_rejection() {
    let behavior = TextNodeBehavior;
    let empty_node = Node::new("text".to_string(), "".to_string(), None, json!({}));

    assert!(behavior.validate(&empty_node).is_err());
}

#[test]
fn test_unicode_whitespace_rejection() {
    let behavior = TextNodeBehavior;
    let zwsp_node = Node::new("text".to_string(), "\u{200B}".to_string(), None, json!({}));

    assert!(behavior.validate(&zwsp_node).is_err());
}
```

### Integration Tests

**Location**: `packages/desktop-app/src/tests/integration/`

**Focus**: Frontend-backend contract validation

```typescript
test('backend rejects empty content', async () => {
  const emptyNode = TestNodeBuilder.text('').build();

  // Backend should reject
  await expect(backend.createNode(emptyNode)).rejects.toThrow();

  // Verify node doesn't exist in database
  const fetched = await backend.getNode(emptyNode.id);
  expect(fetched).toBeNull();
});
```

## Best Practices

### ✅ DO

- **Validate at the boundary**: Always validate before persistence
- **Fail fast**: Return clear error messages immediately
- **Handle gracefully**: Frontend should handle all validation errors
- **Document contracts**: Keep validation rules documented and stable
- **Test edge cases**: Unicode, concurrency, network failures

### ❌ DON'T

- **Don't trust client validation**: Always validate on backend
- **Don't leak implementation**: Error messages shouldn't expose internals
- **Don't couple layers**: Frontend shouldn't depend on specific backend validation
- **Don't skip validation**: Even for "trusted" sources
- **Don't use validation for authorization**: Separate concerns

## Migration Strategy

When changing validation rules:

1. **Additive Changes** (Safe):
   ```rust
   // Adding a new optional constraint
   if node.properties.contains_key("priority") {
       // New validation logic
   }
   ```

2. **Restrictive Changes** (Breaking):
   ```rust
   // Adding a new required constraint
   // 1. Deploy with lenient validation (warn only)
   // 2. Monitor logs for violations
   // 3. Fix violating data
   // 4. Deploy strict validation (error)
   ```

3. **Communication**:
   - Document change in CHANGELOG
   - Update API documentation
   - Notify frontend team
   - Consider feature flags for gradual rollout

## Future Enhancements

### Considered but Deferred

1. **Client-Side Validation Library**
   - **Why Deferred**: Adds coupling between frontend/backend
   - **Alternative**: Backend returns structured error messages that frontend can interpret

2. **Validation Schema Language**
   - **Why Deferred**: Adds complexity without clear benefit
   - **Alternative**: Type-safe Rust validation + good documentation

3. **Progressive Validation**
   - **Why Deferred**: Current approach is simple and works
   - **Alternative**: May revisit if validation becomes performance bottleneck

## References

- Issue #231: Fix Database Persistence Operations
- `/packages/core/src/behaviors/mod.rs`: Behavior validation implementations
- `/packages/desktop-app/src/lib/services/reactive-node-service.svelte.ts`: Frontend service layer
- [Unicode Standard Annex #44](https://www.unicode.org/reports/tr44/): Unicode Character Database

## Change Log

| Date | Change | Issue |
|------|--------|-------|
| 2025-10-12 | Initial documentation | #231 |
| 2025-10-12 | Added Unicode validation section | #231 |
