# Collections System

> **Status**: Planned
> **Priority**: High - Essential for document discovery and organization
> **Implementation**: #756 (Backend/MCP), #757 (UI)

## Overview

Collections are **hierarchical labels** for organizing nodes. A collection is a first-class core node type that nodes can belong to via `member_of` edges, without breaking their root node status.

**Key characteristics:**
- **Hierarchical** - Collections form a DAG (directed acyclic graph)
- **Multi-membership** - A node can belong to multiple collections
- **Multi-parent** - A collection can have multiple parent collections
- **Globally unique names** - Only one "Berlin" collection, reachable via multiple paths
- **Path syntax** - Uses `:` delimiter for navigation (e.g., `hr:policy:vacation:Berlin`)

## The Problem Collections Solve

### Root Node Discovery

```
Without collections, list_root_nodes returns:
├── Spec: Login Feature        ← Has properties, queryable
├── Plan: Login Technical      ← Has properties, queryable
├── Task: Implement Auth       ← Has properties, queryable
├── "Meeting notes from..."    ← Just text. No category.
├── "Random idea about..."     ← Just text.
├── "# Project thoughts"       ← Header. No properties.
├── "Research on AI..."        ← Just text.
├── ... (hundreds more unorganized documents)
```

**Structured nodes** (spec, plan, task) have properties for filtering.
**Primitive nodes** (text, header, code-block) have only content.

Semantic search helps find specific content, but **browsing** requires organization.

### Why Not Tags?

Tags are flat and prone to inconsistency:

```
User creates:
- "meeting"
- "meetings"
- "Meeting"
- "mtg"
- "meeting-notes"
```

Tags require management, normalization, and cleanup.

### Why Not Folders (Parent-Child)?

Traditional folders break the root node concept:

```
If folder "Research" contains document "AI Notes":
  - AI Notes is no longer a root node
  - Can only exist in ONE folder
  - Queries for "all root documents" miss it
```

### Collections: Hierarchical Labels

Collections combine the best of folders and tags:

| Feature | Folders | Tags | Collections |
|---------|---------|------|-------------|
| Hierarchical | ✅ | ❌ | ✅ |
| Multi-membership | ❌ | ✅ | ✅ |
| Root nodes preserved | ❌ | ✅ | ✅ |
| Multi-parent structure | ❌ | N/A | ✅ (DAG) |
| Visual browsing | ✅ | ❌ | ✅ |

## Mental Model: DAG Structure

Collections form a **DAG (Directed Acyclic Graph)**, not a tree. One collection can have multiple parents:

```
Collections (DAG):

hr ─────────────────┐
  policy ───────────┤
    vacation ───────┼──► Berlin (one collection, multiple paths)
engineering ────────┤
  offices ──────────┘

Document X is member of: Berlin
Document X appears in:
  - hr:policy:vacation:Berlin
  - engineering:offices:Berlin
```

**The path is navigation, not identity.** Both paths resolve to the same "Berlin" collection (same UUID).

### Collection vs Parent-Child

```
Parent-child (has_child):   Node is INSIDE parent, not a root
Member-of (member_of):      Node BELONGS TO collection, stays root
```

| Relationship | Edge Type | Effect on Node |
|--------------|-----------|----------------|
| Parent-child | `has_child` | Node is INSIDE parent, not a root |
| Collection membership | `member_of` | Node BELONGS TO collection, stays root |

## Path Syntax

**Delimiter:** `:` (colon)

**Valid paths:**
```
hr:policy:vacation:Berlin
engineering:docs
Berlin
Human Resources (HR):Policy:Vacation
```

**Invalid paths:**
```
hr::policy      ← empty segment
:hr:policy      ← leading colon
hr:policy:      ← trailing colon
```

**Path resolution:**
1. Parse segments by `:`
2. For each segment, find or create collection
3. Case-insensitive lookup, preserve original case on create
4. Globally unique names (reuse existing, don't create duplicates)
5. Auto-create missing segments in path

**Max depth:** 10 levels

## Technical Specification

### CollectionNode

Collections are a **core node type** with minimal schema - no spoke fields, uses hub `content` only:

```rust
SchemaNode {
    id: "collection".to_string(),
    content: "Collection".to_string(),
    is_core: true,
    schema_version: 1,
    description: "Hierarchical label for organizing nodes".to_string(),
    fields: vec![],  // No spoke fields - just uses hub content
    relationships: vec![],  // member_of is native, not defined here
}
```

The collection's **name** is stored in the hub's `content` field.

### Edge Tables

**`member_of`** - Native edge (like `has_child`, `mentions`):

```sql
DEFINE TABLE member_of SCHEMAFULL;
DEFINE FIELD in ON member_of TYPE record<node>;
DEFINE FIELD out ON member_of TYPE record<node>;  -- collection
DEFINE FIELD created_at ON member_of TYPE datetime DEFAULT time::now();

DEFINE INDEX member_of_in ON member_of FIELDS in;
DEFINE INDEX member_of_out ON member_of FIELDS out;
```

**Collection hierarchy** - Uses `member_of` edges between collection nodes (child `member_of` parent), forming a DAG structure that supports multi-parent relationships.

### Cycle Validation

Collections form a DAG - cycles are not allowed. When setting a parent:

```sql
-- Check if new_parent is a descendant of collection (would create cycle)
SELECT * FROM node:$new_parent<-member_of*<-node WHERE id = $collection_id;
-- If result is not empty → reject
```

### Node Response Extension

Nodes include their collection memberships:

```javascript
{
  id: "xyz",
  nodeType: "text",
  content: "Vacation Policy",
  mentions: [...],
  mentionedBy: [...],
  memberOf: ["col-id-1", "col-id-2"]  // Collection IDs
}
```

## MCP Tool Integration

Collections are integrated into existing MCP tools rather than creating many new discovery tools.

### Extended Tools

**`create_node`** - Add optional `collection` param:
```javascript
create_node({
  node_type: "text",
  content: "Vacation Policy",
  collection: "hr:policy:vacation:Berlin"  // Auto-creates path
})
```

**`update_node`** - Add collection management:
```javascript
update_node({
  node_id: "xyz",
  add_to_collections: ["hr:policy", "engineering:docs"],
  remove_from_collections: ["archive:2024"]
})
```

**`query_nodes`** - Add collection filter:
```javascript
query_nodes({
  node_type: "task",
  collection: "engineering:projects:alpha",
  collection_recursive: true  // Include child collections
})
```

**`create_nodes_from_markdown`** - Add collection param:
```javascript
create_nodes_from_markdown({
  markdown_content: "# Project Spec\n\n...",
  collection: "engineering:projects:alpha:specs"
})
```

**`search_semantic`** - Add scoped search:
```javascript
search_semantic({
  query: "vacation policy",
  collection: "hr:policy",
  collection_recursive: true
})
```

### New Discovery Tool

**`get_node_collections`** - Get collections a node belongs to:
```javascript
get_node_collections({
  node_id: "xyz",
  format: "tree" | "flat"  // Default: "flat"
})

// flat: [{ id, content }, ...]
// tree: [{ id, content, children: [...] }, ...]  // Children downward only
```

## SurrealDB Query Patterns

All collection operations use **single SurrealDB calls** with graph traversal:

```sql
-- Get all members of a collection
SELECT * FROM node WHERE ->member_of->node = $collection_id;

-- Get all members recursively (collection + descendants)
SELECT * FROM node WHERE ->member_of->node IN (
  SELECT id FROM node WHERE node_type = 'collection'
  AND id = $collection_id OR <-member_of*<-node CONTAINS $collection_id
);

-- Get all collections a node belongs to
SELECT ->member_of->node.* FROM node:$id
WHERE ->member_of->node.node_type = 'collection';

-- Get collection ancestry (for path building)
SELECT ->member_of->node.* FROM node:$id WHERE node_type = 'collection';

-- Cycle detection
SELECT * FROM node:$new_parent<-member_of*<-node WHERE id = $collection_id;
```

## Usage Examples

### Team Document Organization

```
Collections:
├── Engineering
│   ├── Architecture
│   │   └── Decisions
│   ├── Runbooks
│   └── Onboarding
├── Product
│   ├── Specs
│   └── Research
└── HR
    └── Policies
        └── Vacation
            └── Berlin  ← Also reachable via engineering:offices:Berlin

Document memberships:
- "Architecture Decision: Use SurrealDB"
  └── member_of: engineering:architecture:decisions
  └── member_of: product:research  ← Cross-team relevance
```

### Creating Collections via Path

```javascript
// First call - creates: hr, policy, vacation, Berlin
create_node({
  node_type: "text",
  content: "Doc A",
  collection: "hr:policy:vacation:Berlin"
})

// Second call - reuses Berlin, adds new parent relationship
create_node({
  node_type: "text",
  content: "Doc B",
  collection: "engineering:offices:Berlin"
})
// "Berlin" now has TWO parents: vacation AND offices
```

## Edge Cases

| Case | Behavior |
|------|----------|
| Duplicate collection name | Reuse existing (globally unique) |
| Delete collection with members | Remove `member_of` edges, keep member nodes |
| Collection as member of collection | Not allowed - collections have parents (hierarchy), not membership |
| Empty collection | Allowed (placeholder) |
| Rename collection | Update `content`, uniqueness check applies |
| Case sensitivity | Case-insensitive lookup, preserve original case |
| Cycle in hierarchy | Rejected with error |
| Depth > 10 levels | Rejected with error |

## Future Enhancements

### Smart Collections

Auto-populated based on queries (not in initial implementation):

```javascript
{
  node_type: "smart-collection",
  query: {
    node_type: "task",
    filters: [{ field: "status", equals: "in_progress" }]
  }
}
```

### Collection Templates

Pre-defined structures for common use cases (not in initial implementation).

## Related Documentation

- #756 - Backend implementation issue
- #757 - UI implementation issue
- [How to Add New Node Type](../development/how-to-add-new-node-type.md)
- [SurrealDB Schema Design](../data/surrealdb-schema-design.md)
