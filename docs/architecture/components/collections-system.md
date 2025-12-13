# Collections System

> **Status**: Planned - Logical organization for nodes
> **Priority**: High - Essential for document discovery and team organization
> **Dependencies**: Edge/Relationship System

## Overview

Collections provide **logical grouping** for nodes without breaking the root node concept. Unlike traditional folders (hierarchical, single-membership), collections are:

- **Hierarchical** - Nested organization like `hr/policy/vacation/berlin`
- **Multi-membership** - A node can belong to multiple collections
- **Non-parental** - Nodes remain "root nodes" for querying purposes

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

### Collections: The Middle Ground

```
Collections provide:
├── Hierarchical organization (like folders)
├── Multi-membership (like tags)
├── Root nodes stay root nodes (unlike folders)
└── Team-friendly shared structures
```

## Data Model

### Collections as Nodes with Edge Relationships

```
Collection hierarchy (parent-child edges):

collection:hr
  └── child_of → collection:policy
                   └── child_of → collection:vacation
                                    └── child_of → collection:berlin

Document membership (member_of edges):

node:vacation-rules (remains a root node - no parent)
  └── member_of → collection:berlin
  └── member_of → collection:germany-docs   ← Multiple memberships

node:onboarding-guide (remains a root node)
  └── member_of → collection:hr
  └── member_of → collection:new-hires
```

### Key Distinction

```
Parent-child (hierarchy):     Node is INSIDE parent, not a root
Member-of (collection):       Node BELONGS TO collection, stays root
```

## Schema Definition

### Collection Node Schema

```javascript
{
  node_type: "collection",
  description: "Logical grouping for organizing nodes",
  fields: [
    {
      name: "name",
      type: "text",
      required: true,
      indexed: true,
      description: "Collection display name"
    },
    {
      name: "slug",
      type: "text",
      indexed: true,
      description: "URL-friendly identifier (auto-generated from name)"
    },
    {
      name: "description",
      type: "text",
      description: "What this collection contains"
    },
    {
      name: "icon",
      type: "text",
      description: "Emoji or icon identifier for UI"
    },
    {
      name: "color",
      type: "text",
      description: "Color for UI differentiation"
    },
    {
      name: "is_system",
      type: "boolean",
      default: false,
      description: "System collections cannot be deleted by users"
    },
    {
      name: "visibility",
      type: "enum",
      coreValues: [
        { value: "private", label: "Private" },
        { value: "team", label: "Team" },
        { value: "public", label: "Public" }
      ],
      default: "private",
      description: "Who can see this collection"
    }
  ],
  relationships: [
    {
      name: "parent_collection",
      target: "collection",
      type: "has_one",
      description: "Parent collection for hierarchy"
    },
    {
      name: "child_collections",
      target: "collection",
      type: "has_many",
      description: "Nested sub-collections"
    },
    {
      name: "members",
      target: "*",  // Any node type
      type: "has_many",
      edge_type: "member_of",
      description: "Nodes belonging to this collection"
    }
  ]
}
```

### Edge Types

```javascript
// Collection hierarchy
{
  edge_type: "child_of",
  from: "collection",
  to: "collection",
  properties: {
    order: "number"  // For ordering child collections
  }
}

// Node membership
{
  edge_type: "member_of",
  from: "*",  // Any node type
  to: "collection",
  properties: {
    added_at: "datetime",
    added_by: "text"  // User who added it
  }
}
```

## Query Patterns

### SurrealDB Queries

```sql
-- All nodes in a specific collection
SELECT * FROM node WHERE ->member_of->collection:berlin;

-- All nodes in collection and its children (recursive)
SELECT * FROM node WHERE ->member_of->(
  SELECT * FROM collection WHERE <-child_of*<-collection:hr
);

-- All collections a node belongs to
SELECT ->member_of->collection.* FROM node:vacation-rules;

-- Collection path (breadcrumb)
SELECT <-child_of<-collection.* FROM collection:berlin;
-- Returns: [collection:vacation, collection:policy, collection:hr]

-- Root collections (no parent)
SELECT * FROM collection WHERE NOT ->child_of->collection;

-- Search within a collection
SELECT * FROM node
  WHERE ->member_of->collection:research
  AND content @@ 'machine learning';
```

### MCP Tool Extensions

```javascript
// New tool: list_collections
{
  name: "list_collections",
  description: "List all collections, optionally filtered by parent",
  parameters: {
    parent_id: "string (optional) - Parent collection ID, null for root",
    include_counts: "boolean - Include member count"
  }
}

// New tool: get_collection_members
{
  name: "get_collection_members",
  description: "Get all nodes in a collection",
  parameters: {
    collection_id: "string - Collection ID",
    recursive: "boolean - Include members of child collections",
    node_type: "string (optional) - Filter by node type"
  }
}

// New tool: add_to_collection
{
  name: "add_to_collection",
  description: "Add a node to a collection",
  parameters: {
    node_id: "string - Node to add",
    collection_id: "string - Target collection"
  }
}

// New tool: remove_from_collection
{
  name: "remove_from_collection",
  description: "Remove a node from a collection",
  parameters: {
    node_id: "string - Node to remove",
    collection_id: "string - Collection to remove from"
  }
}

// Extended query_nodes with collection filter
{
  name: "query_nodes",
  parameters: {
    // ... existing parameters
    collection_id: "string (optional) - Filter to nodes in this collection",
    collection_recursive: "boolean - Include child collection members"
  }
}
```

## Usage Examples

### Team Document Organization

```
Collections:
├── 📁 Engineering
│   ├── 📁 Architecture
│   │   ├── 📁 Decisions (ADRs)
│   │   └── 📁 Diagrams
│   ├── 📁 Runbooks
│   └── 📁 Onboarding
├── 📁 Product
│   ├── 📁 Specs
│   ├── 📁 Research
│   └── 📁 Roadmaps
└── 📁 HR
    ├── 📁 Policies
    │   ├── 📁 Vacation
    │   │   ├── 📁 Berlin
    │   │   └── 📁 NYC
    │   └── 📁 Remote Work
    └── 📁 Benefits

Document memberships:
- "Architecture Decision: Use SurrealDB"
  └── member_of: Engineering/Architecture/Decisions
  └── member_of: Product/Research  ← Cross-team relevance

- "Vacation Policy Germany"
  └── member_of: HR/Policies/Vacation/Berlin
  └── member_of: Engineering/Onboarding  ← Relevant for new hires
```

### Project-Based Organization

```
Collections:
├── 📁 Projects
│   ├── 📁 Project Alpha
│   │   ├── 📁 Specs
│   │   ├── 📁 Plans
│   │   ├── 📁 Tasks
│   │   └── 📁 Docs
│   └── 📁 Project Beta
│       └── ...
└── 📁 Archive
    └── 📁 2024
        └── 📁 Project Gamma (completed)
```

### Workflow Integration

Workflows can automatically organize output:

```javascript
// Trigger action
{
  type: "add_to_collection",
  target: "$created_node",
  collection: "projects/$project_name/specs"
}
```

When a spec is created for "Project Alpha", it's automatically added to `Projects/Project Alpha/Specs`.

## UI Considerations

### Collection Browser

```
┌─────────────────────────────────────────────────────────┐
│ Collections                                    [+ New]  │
├─────────────────────────────────────────────────────────┤
│ 📁 Engineering (42)                               ▶     │
│ 📁 Product (28)                                   ▶     │
│ 📁 HR (15)                                        ▶     │
│ 📁 Projects (67)                                  ▼     │
│   ├── 📁 Project Alpha (23)                      ▶     │
│   ├── 📁 Project Beta (18)                       ▶     │
│   └── 📁 Project Gamma (26)                      ▶     │
│ 📁 Archive (156)                                  ▶     │
└─────────────────────────────────────────────────────────┘
```

### Node Membership Indicator

```
┌─────────────────────────────────────────────────────────┐
│ Vacation Policy Germany                                 │
│                                                         │
│ Collections: HR/Policies/Vacation/Berlin                │
│              Engineering/Onboarding                     │
│              [+ Add to collection]                      │
├─────────────────────────────────────────────────────────┤
│ Content...                                              │
└─────────────────────────────────────────────────────────┘
```

### Drag-and-Drop

- Drag node to collection = add membership
- Drag node out of collection view = remove membership (with confirmation)
- Drag collection to collection = nest (create child_of edge)

## Comparison: Collections vs Alternatives

| Feature | Folders | Tags | Collections |
|---------|---------|------|-------------|
| Hierarchical | ✅ | ❌ | ✅ |
| Multi-membership | ❌ | ✅ | ✅ |
| Root nodes preserved | ❌ | ✅ | ✅ |
| Nested organization | ✅ | ❌ | ✅ |
| Team-friendly | ✅ | ⚠️ (needs management) | ✅ |
| Query by containment | ✅ | ✅ | ✅ |
| Visual browsing | ✅ | ❌ | ✅ |

## Future Enhancements

### Smart Collections

Auto-populated based on queries:

```javascript
{
  node_type: "smart-collection",
  query: {
    node_type: "task",
    property_filters: [
      { path: "$.status", equals: "in_progress" },
      { path: "$.assignee", equals: "$current_user" }
    ]
  },
  refresh: "on_access"  // or "realtime", "hourly"
}
```

### Collection Templates

Pre-defined structures for common use cases:

```javascript
{
  name: "Software Project Template",
  structure: [
    { name: "Specs", icon: "📋" },
    { name: "Plans", icon: "📐" },
    { name: "Tasks", icon: "✅" },
    { name: "Docs", icon: "📄" },
    { name: "Archive", icon: "📦" }
  ]
}
```

### Collection Sharing

```javascript
{
  name: "Shared with Marketing",
  visibility: "team",
  shared_with: ["team:marketing"],
  permissions: "read"  // or "read-write"
}
```

### Collection Sync

For teams using external tools:

```javascript
{
  sync_adapter: "google-drive",
  folder_id: "...",
  direction: "bidirectional",
  conflict_resolution: "nodespace-wins"
}
```

## Implementation Notes

### Edge Table

All relationships (including collection membership) use unified edge table:

```sql
DEFINE TABLE edge SCHEMAFULL;
DEFINE FIELD from ON edge TYPE record;
DEFINE FIELD to ON edge TYPE record;
DEFINE FIELD edge_type ON edge TYPE string;
DEFINE FIELD properties ON edge TYPE object;
DEFINE FIELD created_at ON edge TYPE datetime DEFAULT time::now();

DEFINE INDEX edge_from ON edge FIELDS from;
DEFINE INDEX edge_to ON edge FIELDS to;
DEFINE INDEX edge_type ON edge FIELDS edge_type;
```

### Query Performance

For recursive collection queries, consider:

1. **Materialized paths** - Store full path as string for fast prefix matching
2. **Caching** - Cache collection hierarchy (changes infrequently)
3. **Limit depth** - Reasonable max depth (e.g., 10 levels)

---

## Summary

Collections provide the **organizational layer** that makes NodeSpace usable at scale:

- **Hierarchical** like folders for intuitive browsing
- **Multi-membership** like tags for flexible categorization
- **Non-parental** so root nodes stay queryable
- **Team-friendly** for shared organizational structures

Combined with semantic search and workflow automation, collections complete the document discovery story.
