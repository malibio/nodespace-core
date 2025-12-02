# How to Add a New Node Type in NodeSpace

This guide provides a **complete checklist** for adding a new node type to NodeSpace. NodeSpace uses a hybrid architecture with both hardcoded behaviors and schema-driven extensions.

## Architecture Overview

NodeSpace's node type system consists of several interconnected components:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                          NODE TYPE ARCHITECTURE                          │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │                     BACKEND (Rust)                                │   │
│  │                                                                   │   │
│  │  1. NodeBehavior Trait    - Validation, capabilities, processing │   │
│  │  2. Core Schema           - Database-queryable fields (optional) │   │
│  │  3. Spoke Table           - Type-specific indexed data (auto)    │   │
│  │  4. Hub Table (node)      - Universal metadata (always exists)   │   │
│  │                                                                   │   │
│  └──────────────────────────────────────────────────────────────────┘   │
│                                    │                                     │
│                                    ▼                                     │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │                    FRONTEND (TypeScript/Svelte)                   │   │
│  │                                                                   │   │
│  │  1. PluginDefinition      - Slash commands, pattern detection    │   │
│  │  2. Node Component        - Individual node rendering (*Node)    │   │
│  │  3. Viewer Component      - Page-level display (*NodeViewer)     │   │
│  │  4. Icon Registration     - Visual representation                │   │
│  │                                                                   │   │
│  └──────────────────────────────────────────────────────────────────┘   │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

### Schema Management Note

When creating schemas for new node types, follow the [Schema Management Implementation Guide](./schema-management-implementation-guide.md) for:
- Setting appropriate **protection levels** (core/user/system) for fields
- Using **extensible enums** where users might extend values
- **Versioning schemas** starting at version 1
- Documenting **migration strategy** upfront for future schema changes

---

## Decision Tree: What Do You Need?

Before implementing, determine your requirements:

```
Is this a CORE node type that ships with NodeSpace?
├─ YES → Full implementation (behavior + schema + frontend + tests)
│        Examples: text, task, date, header, code-block
│
└─ NO → Is it a user-defined custom type?
        ├─ YES → Schema-only (no behavior modification needed)
        │        Users create via schema system
        │
        └─ Plugin/Extension type?
            └─ Plugin + behavior + optional spoke table
               Examples: whiteboard, image, query
```

---

## Complete Implementation Checklist

### Step 1: Backend - Add NodeBehavior (Required)

**File:** `packages/core/src/behaviors/mod.rs`

#### 1.1 Define Node Behavior Struct

```rust
/// Built-in behavior for [your-type] nodes
///
/// [Description of what this node type represents and how it's used]
///
/// # Characteristics
/// - Can have children: [true/false]
/// - Supports markdown: [true/false]
pub struct YourTypeNodeBehavior;

impl NodeBehavior for YourTypeNodeBehavior {
    fn type_name(&self) -> &'static str {
        "your-type"  // kebab-case identifier
    }

    fn validate(&self, node: &Node) -> Result<(), NodeValidationError> {
        // Add validation logic
        // - Check required fields
        // - Validate field formats
        // - Type-specific constraints
        //
        // Note: Per Issue #479, blank nodes are generally allowed
        // Only add validation if this type has specific requirements
        Ok(())
    }

    fn can_have_children(&self) -> bool {
        true  // Can this node contain child nodes?
    }

    fn supports_markdown(&self) -> bool {
        false  // Does content support markdown formatting?
    }

    fn default_metadata(&self) -> serde_json::Value {
        // Default properties for new nodes of this type
        // Use type-namespaced properties (Issue #397)
        serde_json::json!({
            "your-type": {
                "your_field": "default_value"
            }
        })
    }

    // Optional: Override for embedding behavior (Issue #573)
    fn get_embeddable_content(&self, node: &Node) -> Option<String> {
        // Return content to embed, or None to skip embedding
        // Tasks and dates return None (not embedded as standalone)
        if node.content.trim().is_empty() {
            None
        } else {
            Some(node.content.clone())
        }
    }

    fn get_parent_contribution(&self, node: &Node) -> Option<String> {
        // Return content that contributes to parent's embedding
        // Tasks return None (don't pollute parent embeddings)
        if node.content.trim().is_empty() {
            None
        } else {
            Some(node.content.clone())
        }
    }
}
```

#### 1.2 Register in NodeBehaviorRegistry

```rust
impl NodeBehaviorRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            behaviors: HashMap::new(),
        };

        // ... existing registrations ...
        registry.register(Arc::new(YourTypeNodeBehavior));  // Add this

        registry
    }
}
```

#### 1.3 Add Backend Tests

```rust
#[test]
fn test_your_type_node_behavior_validation() {
    let behavior = YourTypeNodeBehavior;

    let valid_node = Node::new(
        "your-type".to_string(),
        "Content".to_string(),
        json!({}),
    );
    assert!(behavior.validate(&valid_node).is_ok());

    // Add edge case tests
}

#[test]
fn test_your_type_node_behavior_capabilities() {
    let behavior = YourTypeNodeBehavior;

    assert_eq!(behavior.type_name(), "your-type");
    assert!(behavior.can_have_children());  // or assert!(!...)
    assert!(!behavior.supports_markdown());  // or assert!(...)
}
```

---

### Step 2: Backend - Add Core Schema (Required for Core Types)

**File:** `packages/core/src/models/core_schemas.rs`

Add to the `get_core_schemas()` function:

```rust
SchemaNode {
    id: "your-type".to_string(),
    content: "Your Type".to_string(),  // Display name
    version: 1,
    created_at: now,
    modified_at: now,
    is_core: true,
    schema_version: 1,
    description: "Description of this node type".to_string(),
    fields: vec![
        // Only add fields if the type needs queryable properties
        // Simple types like text, date, header have empty fields: vec![]
        SchemaField {
            name: "your_field".to_string(),
            field_type: "text".to_string(),  // text, enum, date, number, boolean
            protection: SchemaProtectionLevel::Core,  // or User
            indexed: true,  // true = creates spoke table column
            required: Some(false),
            // ... other field options
            ..Default::default()
        },
    ],
    relationships: vec![],
}
```

**Important Notes:**
- **Spoke tables are auto-generated**: The `SchemaTableManager` automatically generates spoke tables for types with indexed fields. You do NOT need to manually edit `schema.surql` (Issue #691).
- **Simple types have no fields**: Types like `text`, `header`, `code-block` have `fields: vec![]` and store data in the hub table's `properties` field.
- **Complex types have indexed fields**: Types like `task` have indexed fields that create spoke table columns for efficient querying.

---

### Step 3: Frontend - Create Plugin Definition

**File:** `packages/desktop-app/src/lib/plugins/core-plugins.ts`

```typescript
export const yourTypeNodePlugin: PluginDefinition = {
  id: 'your-type',
  name: 'Your Type Node',
  description: 'Description shown in slash command menu',
  version: '1.0.0',

  // Optional: Pattern detection for auto-conversion (Issue #667)
  pattern: {
    detect: /^your-pattern/,       // Regex to detect in content
    canRevert: true,               // Can revert to text when deleted?
    revert: /^your-revert$/,       // Pattern that triggers reversion
    onEnter: 'inherit',            // 'inherit' | 'text' | 'none'
    prefixToInherit: 'prefix ',    // Or function: (content) => prefix
    splittingStrategy: 'prefix-inheritance',  // or 'simple-split'
    cursorPlacement: 'after-prefix',  // 'start' | 'after-prefix' | 'end'
    extractMetadata: (match) => ({
      // Extract metadata from regex match
    })
  },

  config: {
    slashCommands: [
      {
        id: 'your-type',
        name: 'Your Type',
        description: 'Create a your-type node',
        shortcut: 'yt',              // Optional keyboard shortcut
        contentTemplate: '',         // Default content
        nodeType: 'your-type',
        desiredCursorPosition: 0     // Optional cursor position
      }
    ],
    canHaveChildren: true,
    canBeChild: true
  },

  // Node component (renders individual node in tree)
  node: {
    lazyLoad: () => import('../design/components/your-type-node.svelte'),
    priority: 1
  },

  // Optional: Viewer component (page-level display)
  // Only needed for custom page rendering (most use BaseNodeViewer)
  // viewer: {
  //   lazyLoad: () => import('../components/viewers/your-type-node-viewer.svelte'),
  //   priority: 1
  // },

  // Reference component (how node appears when referenced)
  reference: {
    component: BaseNodeReference as NodeReferenceComponent,
    priority: 1
  },

  // Optional: Custom metadata extraction (Issue #698)
  extractMetadata: (node) => {
    const props = node.properties?.[node.nodeType] || {};
    return { ...props };
  },

  // Optional: Protect structured content from merges
  acceptsContentMerge: true  // Set false for code-block, quote-block, etc.
};
```

Add to `corePlugins` array:

```typescript
export const corePlugins = [
  // ... existing plugins ...
  yourTypeNodePlugin
];
```

---

### Step 4: Frontend - Create Node Component

**File:** `packages/desktop-app/src/lib/design/components/your-type-node.svelte`

```svelte
<!--
  YourTypeNode - Wraps BaseNode for [specific functionality]

  Responsibilities:
  - [Feature 1]
  - [Feature 2]
-->

<script lang="ts">
  import type { NodeComponentProps } from '../../types/node-viewers';
  import BaseNode from './base-node.svelte';

  let {
    node,
    isEditing = false,
    depth = 0,
    metadata = {},
    onUpdate,
    onDelete,
    onNavigate
  }: NodeComponentProps = $props();

  // Optional: Component-specific derived state
  let customMetadata = $derived({
    ...metadata,
    // Add custom metadata transformations
  });
</script>

<BaseNode
  {node}
  {isEditing}
  {depth}
  metadata={customMetadata}
  {onUpdate}
  {onDelete}
  {onNavigate}
>
  <!-- Optional: Custom content in slots -->
  <!-- Most types just use BaseNode's defaults -->
</BaseNode>

<style>
  /* Component-specific styling */
</style>
```

---

### Step 5: Frontend - Register Icon

**File:** `packages/desktop-app/src/lib/design/icons/registry.ts`

#### 5.1 Import Icon Component (if creating custom)

```typescript
import YourTypeIcon from './components/your-type-icon.svelte';
```

#### 5.2 Register in `registerCoreConfigs()`

```typescript
// Your type nodes - [brief description]
this.register('your-type', {
  component: CircleIcon,  // Or YourTypeIcon if custom
  semanticClass: 'node-icon',
  colorVar: 'hsl(var(--node-your-type, 200 40% 45%))',
  hasState: false,    // true for nodes with state variations (like task)
  hasRingEffect: true // true if node can have children
});
```

**Key decisions:**
- `hasState: true` → Node has visual states (like task: pending/inProgress/completed)
- `hasRingEffect: true` → Node can have children (shows ring when it does)
- `hasRingEffect: false` → Leaf node, cannot have children

---

### Step 6: Frontend - Add Tests

**File:** `packages/desktop-app/src/tests/plugins/your-type-plugin.test.ts`

```typescript
import { describe, it, expect } from 'vitest';
import { pluginRegistry } from '$lib/plugins/plugin-registry';

describe('YourType Plugin', () => {
  describe('Plugin Registration', () => {
    it('should have plugin registered', () => {
      expect(pluginRegistry.hasPlugin('your-type')).toBe(true);
    });

    it('should have correct configuration', () => {
      const plugin = pluginRegistry.getPlugin('your-type');
      expect(plugin?.config.canHaveChildren).toBe(true);
      expect(plugin?.config.canBeChild).toBe(true);
    });
  });

  describe('Slash Command', () => {
    it('should register slash command', () => {
      const commands = pluginRegistry.getAllSlashCommands();
      const command = commands.find(c => c.nodeType === 'your-type');
      expect(command).toBeDefined();
      expect(command?.id).toBe('your-type');
    });
  });

  // Add pattern detection tests if applicable
  describe('Pattern Detection', () => {
    it('should detect pattern and extract metadata', () => {
      const content = 'your pattern here';
      const detection = pluginRegistry.detectPatternInContent(content);
      expect(detection).not.toBeNull();
      expect(detection?.config.targetNodeType).toBe('your-type');
    });
  });
});
```

---

## Database Architecture

### Hub-and-Spoke Pattern

NodeSpace uses a hub-and-spoke database architecture (Issue #511):

```
┌─────────────────────────────────────────────────────────────────┐
│                    HUB TABLE: node                               │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │ id, content, node_type, version, created_at, modified_at  │  │
│  │ data → record<spoke_table>  (Record Link to spoke)        │  │
│  │ properties (for types without spoke tables)               │  │
│  └───────────────────────────────────────────────────────────┘  │
│                              │                                   │
│               ┌──────────────┼──────────────┐                   │
│               ▼              ▼              ▼                   │
│  ┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐   │
│  │  SPOKE: task    │ │  SPOKE: schema  │ │  (auto-gen)     │   │
│  │                 │ │                 │ │                 │   │
│  │ status, priority│ │ is_core, fields │ │ indexed fields  │   │
│  │ due_date, etc.  │ │ relationships   │ │ from schema     │   │
│  └─────────────────┘ └─────────────────┘ └─────────────────┘   │
│                                                                  │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │              GRAPH EDGES: has_child, mentions              │  │
│  │  Parent-child hierarchy and node references                │  │
│  └───────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

### When Do You Need a Spoke Table?

**Spoke tables are automatically generated** by `SchemaTableManager` when your schema has indexed fields. You need indexed fields when:

- You want to query nodes by a specific field (e.g., "find all tasks with status=done")
- You need to sort/filter by the field
- The field is used frequently in searches

**Types WITHOUT spoke tables** (simple types):
- `text`, `header`, `code-block`, `quote-block`, `ordered-list`, `date`
- Properties stored in hub table's `properties` field

**Types WITH spoke tables** (complex types with indexed fields):
- `task` - has indexed status, priority, due_date fields
- `schema` - structural table for type definitions (hardcoded in schema.surql)
- Future: `query`, `person`, etc.

---

## Component Naming Conventions

```
*Node       = Individual node component that wraps BaseNode
*NodeViewer = Page-level viewer that wraps BaseNodeViewer
```

**Examples:**
- `TaskNode` - Renders a single task in the tree
- `DateNodeViewer` - Page view when navigating to a date

Most node types only need a `*Node` component. The `*NodeViewer` is only needed for types with custom page-level rendering (like `DateNodeViewer` for calendar views).

---

## Common Patterns

### Pattern Detection (Auto-Conversion)

When users type certain patterns, automatically convert to the node type:

```typescript
pattern: {
  detect: /^>\s/,      // Detect "> " at start
  canRevert: true,
  revert: /^>$/,       // Revert when just ">" remains
  onEnter: 'inherit',
  prefixToInherit: '> ',
  splittingStrategy: 'prefix-inheritance',
  cursorPlacement: 'after-prefix'
}
```

### State-Based Icons (like Task)

For nodes with visual state variations:

```typescript
// In icon registry
this.register('task', {
  component: TaskIcon,
  semanticClass: 'task-icon',
  colorVar: 'hsl(var(--node-task, 200 40% 45%))',
  hasState: true,  // Enables pending/inProgress/completed states
  hasRingEffect: true
});

// In plugin extractMetadata
extractMetadata: (node) => {
  const status = node.properties?.task?.status;
  let taskState = 'pending';
  if (status === 'in_progress') taskState = 'inProgress';
  if (status === 'done') taskState = 'completed';
  return { taskState };
}
```

### Protected Structured Content

For nodes with syntax that shouldn't be merged (code blocks, quotes):

```typescript
acceptsContentMerge: false  // Prevents backspace from merging content
```

---

## Common Pitfalls & Lessons Learned

### ❌ Behavior Not Registered
**Symptom:** Backend validation fails, default metadata not initialized
**Fix:** Register in `NodeBehaviorRegistry::new()` in `packages/core/src/behaviors/mod.rs`

### ❌ Schema Not Added
**Symptom:** Node type not recognized, spoke table not created
**Fix:** Add to `get_core_schemas()` in `packages/core/src/models/core_schemas.rs`

### ❌ Icon Not Registered
**Symptom:** Default circle icon appears instead of custom icon
**Fix:** Register in `packages/desktop-app/src/lib/design/icons/registry.ts`

### ❌ Plugin Not Exported
**Symptom:** Slash commands don't appear, pattern detection doesn't work
**Fix:** Add to `corePlugins` array in `packages/desktop-app/src/lib/plugins/core-plugins.ts`

### ❌ Wrong hasRingEffect Setting
**Symptom:** Ring appears on leaf nodes or missing on parent nodes
**Fix:** Set `hasRingEffect: false` for leaf nodes (`canHaveChildren: false`)

### ❌ Tests Not Added
**Symptom:** Regressions in future changes, incomplete validation
**Fix:** Add comprehensive tests for plugin, behavior, and component

---

## Validation Checklist

Before considering the node type implementation complete:

### Backend
- [ ] `*NodeBehavior` struct implemented with all trait methods
- [ ] Behavior registered in `NodeBehaviorRegistry::new()`
- [ ] Schema added to `get_core_schemas()` (for core types)
- [ ] Backend tests passing

### Frontend
- [ ] Plugin defined in `core-plugins.ts`
- [ ] Plugin added to `corePlugins` array
- [ ] Node component created (`*-node.svelte`)
- [ ] Icon registered in `registry.ts`
- [ ] Frontend tests created and passing

### Integration
- [ ] Manual test: Create node via slash command
- [ ] Manual test: Create node via pattern detection (if applicable)
- [ ] Manual test: Indent/outdent operations work correctly
- [ ] Manual test: Icon displays correctly (with ring if applicable)
- [ ] Manual test: Node persists and loads from database

### Quality
- [ ] All tests passing (`bun run test:all`)
- [ ] Quality checks passing (`bun run quality:fix`)

---

## Example Implementations

For reference implementations, see:

- **TextNode** - Simplest implementation, no extra fields
- **HeaderNode** - Pattern detection with metadata extraction (headerLevel)
- **TaskNode** - State management, spoke table with indexed fields
- **CodeBlockNode** - Leaf node with `acceptsContentMerge: false`
- **QuoteBlockNode** - Pattern inheritance on Enter key

---

## Related Documentation

- [Node Behavior System](../business-logic/node-behavior-system.md) - Detailed behavior architecture
- [Schema Management Guide](./schema-management-implementation-guide.md) - Schema system details
- [Component Architecture Guide](../components/component-architecture-guide.md) - Frontend patterns
- [SurrealDB Schema Design](../data/surrealdb-schema-design.md) - Database architecture
- [Persistence Architecture](../persistence-architecture.md) - Data flow overview
