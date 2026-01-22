# Service Layer Architecture

## Overview

NodeSpace uses a layered architecture where the **service layer** orchestrates business logic, validation, and event emission, while the **data layer** (SurrealStore) handles pure persistence.

This document defines the patterns for when to create domain-specific services vs. using generic NodeService operations.

## Core Principle: "Everything is a Node"

All entities in NodeSpace (tasks, collections, schemas, people, projects) are stored as **nodes** with a `node_type` field. This enables:

- Unified CRUD operations via `NodeService`
- Consistent event emission
- Schema-driven validation and extensibility
- Graph relationships between any node types

## Architecture Layers

```
┌─────────────────────────────────────────────────────────────────┐
│                        MCP Handlers / Tauri Commands            │
│  (API layer - translates external requests to service calls)    │
└─────────────────────────────────────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────┐
│                        NodeService                               │
│  - Generic CRUD for all node types                              │
│  - ALL event emission (NodeCreated, NodeUpdated, etc.)          │
│  - ALL relationship event emission (RelationshipCreated, etc.)  │
│  - Delegates to NodeBehaviorRegistry for validation             │
└─────────────────────────────────────────────────────────────────┘
        │                                           │
        │ validates via                             │ orchestrates via
        ▼                                           ▼
┌───────────────────────────┐           ┌───────────────────────────┐
│   NodeBehaviorRegistry    │           │   Domain Services         │
│  ┌─────────┐ ┌──────────┐ │           │   (thin orchestration)    │
│  │  Task   │ │Collection│ │           │                           │
│  │Behavior │ │ Behavior │ │           │  CollectionService        │
│  └─────────┘ └──────────┘ │           │  - Path resolution        │
│  ┌─────────┐ ┌──────────┐ │           │  - Multi-node operations  │
│  │ Person  │ │ Project  │ │           │  - Delegates CRUD/events  │
│  │Behavior │ │ Behavior │ │           │    to NodeService         │
│  └─────────┘ └──────────┘ │           │                           │
└───────────────────────────┘           └───────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────┐
│                        SurrealStore                              │
│  - Pure data persistence (CRUD operations)                       │
│  - NO event emission                                             │
│  - NO business logic                                             │
│  - NO client_id awareness                                        │
└─────────────────────────────────────────────────────────────────┘
```

## NodeService: The Single Source of Truth

`NodeService` is the central service for all node and relationship operations:

### Responsibilities

1. **Generic CRUD** - Create, read, update, delete for all node types
2. **Event Emission** - ALL domain events originate from NodeService:
   - `NodeCreated`, `NodeUpdated`, `NodeDeleted`
   - `RelationshipCreated`, `RelationshipUpdated`, `RelationshipDeleted`
3. **Behavior Dispatch** - Routes to appropriate `NodeBehavior` for validation
4. **Relationship Management** - `create_relationship()`, `delete_relationship()` for all relationship types

### Key Methods

```rust
// Node CRUD (emits NodeCreated/Updated/Deleted events)
pub async fn create_node(&self, params: CreateNodeParams) -> Result<String>;
pub async fn update_node(&self, id: &str, update: NodeUpdate) -> Result<Node>;
pub async fn delete_node(&self, id: &str) -> Result<()>;

// Relationship CRUD (emits RelationshipCreated/Deleted events)
pub async fn create_relationship(&self, source_id: &str, rel_type: &str, target_id: &str) -> Result<()>;
pub async fn delete_relationship(&self, source_id: &str, rel_type: &str, target_id: &str) -> Result<()>;
```

### Built-in Relationship Types

| Relationship | Description | Direction |
|--------------|-------------|-----------|
| `has_child` | Parent-child hierarchy | parent → child |
| `member_of` | Collection membership | node → collection |
| `mentions` | Node references | source → target |
| Custom types | Schema-defined relationships | varies |

## NodeBehavior: Type-Specific Validation

`NodeBehavior` implementations provide compile-time type safety for core node types:

### Responsibilities

1. **Validation** - Type-specific validation rules
2. **Defaults** - Default property values for new nodes
3. **Capabilities** - Whether type supports children, markdown, etc.

### When to Create a Behavior

Create a `NodeBehavior` when:
- The node type has **core properties** that UI depends on
- There are **invariants** that must always hold (e.g., date format)
- **Performance-critical** validation is needed

### Existing Behaviors

- `TextNodeBehavior` - Text content validation
- `TaskNodeBehavior` - Task status, priority validation
- `DateNodeBehavior` - Date ID format (YYYY-MM-DD)
- `CollectionNodeBehavior` - Collection-specific rules
- `SchemaNodeBehavior` - Schema structure validation

## Domain Services: When to Create One

Domain services are **thin orchestration layers** that coordinate multiple operations but **delegate all CRUD and events to NodeService**.

### Decision Tree

```
New Node Type (e.g., Person, Project, Invoice)
    │
    ├─► Does it have ONLY data validation needs?
    │       YES ──► Just create XxxBehavior
    │               Use generic NodeService CRUD
    │               NO domain service needed
    │
    ├─► Does it have special syntax/parsing?
    │       YES ──► Create utility functions (stateless)
    │               Can be in a module, not a service
    │
    └─► Does it orchestrate multiple nodes/relationships?
            YES ──► Create XxxService (thin wrapper)
                    Service validates, then delegates to NodeService
                    NO event emission in domain service
```

### CollectionService Example

`CollectionService` exists because collections have:

1. **Path syntax** - `hr:policy:vacation` requires parsing
2. **Multi-node orchestration** - Creating collections along a path
3. **Relationship orchestration** - `member_of` edges between collections

```rust
impl CollectionService {
    /// Creates a new CollectionService with both store (reads) and NodeService (writes)
    pub fn new(store: &Arc<SurrealStore>, node_service: &NodeService) -> Self {
        Self { store, node_service }
    }

    /// Resolves path, creates collections, establishes member_of relationships
    pub async fn resolve_path(&self, path: &str) -> Result<ResolvedPath> {
        // Parse path syntax
        let segments = parse_collection_path(path)?;

        // Batch fetch existing collections (READ - direct store access)
        let existing = self.store.get_collections_by_names(&segment_names).await?;

        // For each segment, find or create collection node
        for segment in segments {
            let (node, created) = match existing.get(&segment.normalized_name) {
                Some(existing) => (existing.clone(), false),
                None => {
                    // CREATE - goes through NodeService for event emission
                    let new_node = self.create_collection(&segment.name).await?;
                    (new_node, true)
                }
            };

            // Create hierarchy relationship (WRITE - through NodeService)
            if let Some(parent_id) = &previous_collection_id {
                self.node_service.create_builtin_relationship(
                    &node.id, "member_of", parent_id, json!({})
                ).await?;
            }
        }
    }

    /// Add node to collection - thin wrapper with validation
    pub async fn add_to_collection(&self, node_id: &str, collection_id: &str) -> Result<()> {
        // Validate target is a collection (READ - direct store access)
        let collection = self.store.get_node(collection_id).await?
            .ok_or(CollectionNotFound)?;

        if collection.node_type != "collection" {
            return Err(InvalidCollectionTarget);
        }

        // Delegate to NodeService for relationship (WRITE - handles events)
        self.node_service.create_builtin_relationship(
            node_id, "member_of", collection_id, json!({})
        ).await
    }
}
```

### Domain Service Data Access Pattern: Read from Store, Write through NodeService

Domain services follow a clear pattern for data access:

**READ Operations** - Direct store access is acceptable:
- Querying collections by name
- Getting collection members
- Finding nodes by criteria
- Any read-only query

**WRITE Operations** - MUST go through NodeService:
- Creating nodes (to emit `NodeCreated` events)
- Creating relationships (to emit `RelationshipCreated` events)
- Updating nodes (to emit `NodeUpdated` events)
- Deleting relationships (to emit `RelationshipDeleted` events)

This pattern allows domain services to have efficient reads while ensuring all write operations emit proper events.

```rust
// CollectionService holds both references
pub struct CollectionService<'a> {
    store: &'a Arc<SurrealStore>,       // For reads
    node_service: &'a NodeService,       // For writes
}

impl CollectionService {
    // READ: Direct store access is fine
    pub async fn get_collection_members(&self, collection_id: &str) -> Result<Vec<String>> {
        self.store.get_collection_members(collection_id).await
    }

    // WRITE: Must go through NodeService for event emission
    pub async fn add_to_collection(&self, node_id: &str, collection_id: &str) -> Result<()> {
        self.node_service.create_builtin_relationship(
            node_id, "member_of", collection_id, json!({})
        ).await
    }
}
```

### What Domain Services Should NOT Do

- **NO event emission** - NodeService is the single source
- **NO direct store writes** - Go through NodeService for creates/updates/deletes
- **NO duplicating NodeService logic**

## Guidelines for New Node Types

### Person, Project, Invoice Examples

| Type | Behavior Needed? | Service Needed? | Reasoning |
|------|-----------------|-----------------|-----------|
| Person | YES (`PersonBehavior`) | NO | Simple entity, CRUD via NodeService |
| Project | YES (`ProjectBehavior`) | MAYBE | Only if team management needs orchestration |
| Invoice | YES (`InvoiceBehavior`) | MAYBE | Only if line items need special handling |

### Adding a New Type

1. **Create the schema** (via MCP or programmatically)
2. **Create `XxxBehavior`** if type has core properties
3. **Register behavior** in `NodeBehaviorRegistry`
4. **Create `XxxService`** ONLY if multi-node orchestration needed
5. **Use generic NodeService** for CRUD operations

## Historical Context

### Why SchemaService Was Removed (Issue #690)

`SchemaService` was deleted because:
- It duplicated `NodeService` CRUD operations
- Schemas are nodes - should use generic node operations
- Validation moved to `SchemaNodeBehavior`
- DDL generation extracted to `SchemaTableManager` (pure function)

### Why CollectionService Exists

`CollectionService` was kept (unlike SchemaService) because:
- Collections have unique path syntax (`hr:policy:vacation`)
- Path resolution orchestrates multiple node creations
- Collection hierarchy requires relationship orchestration

As of Issue #813, `CollectionService` now delegates to `NodeService.create_builtin_relationship()` for all `member_of` operations, ensuring proper event emission.

## Related Documentation

- [Node Behavior System](./node-behavior-system.md) - Behavior trait details
- [Schema Management Guide](../development/schema-management-implementation-guide.md) - Schema-driven extensions
- Issue #690 - SchemaService removal
- Issue #813 - Centralize event emission in NodeService
- Issue #814 - MCP relationship operations
