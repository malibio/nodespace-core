# NodeSpace Component Architecture Guide

## Overview

NodeSpace follows a **consistent, hierarchical component architecture** with clear naming conventions that separate concerns and provide intuitive developer experience. This guide documents the established patterns for building new components.

## Architectural Hierarchy

### 1. Foundation Layer

**BaseNode** (`src/lib/design/components/base-node.svelte`)
- **Purpose**: Abstract core component providing fundamental node functionality
- **Usage**: INTERNAL USE ONLY - should never be used directly in application code
- **Responsibilities**:
  - Content editing via **TextareaController** (see [Textarea Architecture](./textarea-editor-architecture.md))
  - Dual-mode rendering (edit: `<textarea>`, view: rendered markdown)
  - Markdown syntax handling
  - Keyboard shortcuts (Cmd+B, Cmd+I for formatting)
  - Event dispatch and management via keyboard command system
  - Focus management integration with FocusManager service
  - Base styling and layout

**BaseNodeViewer** (`src/lib/design/components/base-node-viewer.svelte`)
- **Purpose**: Container that manages collections of nodes
- **Usage**: Main entry point for page-level node management
- **Responsibilities**:
  - Node tree operations (create, delete, indent, outdent)
  - Navigation between nodes (arrow keys, tab)
  - Plugin registry integration
  - Dynamic node component loading
  - Focus management

### 2. Node Components Layer

**Naming Convention**: `*Node` (e.g., TextNode, TaskNode, DateNode)

**Purpose**: Individual node implementations that wrap BaseNode with type-specific functionality

**Pattern**:
```svelte
<!-- TextNode.svelte -->
<script>
  import BaseNode from '$lib/design/components/base-node.svelte';
  // Add node-specific logic here
</script>

<BaseNode
  {nodeId}
  {nodeType}
  {content}
  {autoFocus}
  on:contentChanged
  on:createNewNode
  <!-- Forward all BaseNode events -->
/>
```

**Current Implementations**:
- **TextNode** (`src/lib/components/viewers/text-node.svelte`)
  - Header-aware multiline logic
  - Smart editing based on header level
  - Markdown processing integration

- **TaskNode** (`src/lib/design/components/task-node.svelte`)
  - Task state management (pending/inProgress/completed)
  - Checkbox interaction
  - Task-specific metadata handling

- **DateNode** (`src/lib/design/components/date-node.svelte`)
  - Date formatting and validation
  - Calendar icon integration
  - Date parsing utilities
  - Cannot be created via slash commands (exists implicitly for all dates)

- **CodeBlockNode** (`src/lib/design/components/code-block-node.svelte`)
  - Multiline code editing with language selection
  - Markdown processing bypass via metadata (`disableMarkdown: true`)
  - Three-phase content transformation (storage/edit/view)
  - Language dropdown and copy button UI
  - Leaf node (cannot have children)
  - Auto-completion for fence syntax

### 3. Viewer Components Layer

**Naming Convention**: `*NodeViewer` (e.g., DateNodeViewer, TaskNodeViewer)

**Purpose**: Page-level components that wrap BaseNodeViewer with specialized UI and navigation

**Pattern**:
```svelte
<!-- DateNodeViewer.svelte -->
<script>
  import BaseNodeViewer from '$lib/design/components/base-node-viewer.svelte';
  // Add viewer-specific UI (navigation, headers, etc.)
</script>

<div class="specialized-viewer">
  <!-- Custom UI elements (navigation, toolbars, etc.) -->
  <div class="viewer-header">
    <!-- Navigation controls, date picker, etc. -->
  </div>

  <!-- Node content area -->
  <div class="node-content-area">
    <BaseNodeViewer />
  </div>
</div>
```

**Current Implementations**:
- **DateNodeViewer** (`src/lib/components/viewers/date-node-viewer.svelte`)
  - Date navigation header
  - Previous/next day functionality
  - Keyboard navigation support
  - Tab title management

**Future Implementations**:
- **TaskNodeViewer** - Project management interface with Gantt charts, filters
- **EntityNodeViewer** - Structured data management interface
- **QueryNodeViewer** - Live query results with real-time updates

## Naming Convention Rules

### ✅ Correct Naming Patterns

| Component Type | Pattern | Example | Purpose |
|---------------|---------|---------|---------|
| **Foundation** | `Base*` | `BaseNode`, `BaseNodeViewer` | Core abstract components |
| **Node Components** | `*Node` | `TextNode`, `TaskNode`, `DateNode` | Individual node wrappers |
| **Viewer Components** | `*NodeViewer` | `DateNodeViewer`, `TaskNodeViewer` | Page-level viewer wrappers |

### ❌ Incorrect Patterns (Avoid These)

- `TextNodeViewer` - Text nodes don't need viewers (BaseNodeViewer is sufficient)
- `DatePageViewer` - Should be `DateNodeViewer` to follow convention
- `BaseTextNode` - Base components should be generic, not type-specific

## File Structure Conventions

```
src/lib/
├── design/components/           # Foundation layer
│   ├── base-node.svelte        # Abstract core component
│   ├── base-node-viewer.svelte # Container management
│   └── task-node.svelte        # Node components (some)
└── components/viewers/          # Node and viewer components
    ├── text-node.svelte        # Text node implementation
    ├── date-node.svelte        # Date node implementation
    └── date-node-viewer.svelte  # Date viewer implementation
```

**Guidelines**:
- **Foundation components** → `src/lib/design/components/`
- **Node & Viewer components** → `src/lib/components/viewers/`
- Use kebab-case for filenames
- Component names match file names (PascalCase in code)

## Plugin Integration

### Registering New Node Types

```typescript
// In src/lib/plugins/corePlugins.ts
export const customNodePlugin: PluginDefinition = {
  id: 'custom',
  name: 'Custom Node',
  description: 'Custom node type with special functionality',
  version: '1.0.0',
  config: {
    slashCommands: [
      {
        id: 'custom',
        name: 'Custom',
        description: 'Create a custom node',
        contentTemplate: ''
      }
    ],
    canHaveChildren: true,
    canBeChild: true
  },
  node: {
    lazyLoad: () => import('../components/viewers/custom-node.svelte'),
    priority: 1
  },
  // Optional: Add viewer for complex page-level functionality
  viewer: {
    lazyLoad: () => import('../components/viewers/custom-node-viewer.svelte'),
    priority: 1
  }
};
```

### Component Implementation Template

#### Node Component Template

```svelte
<!-- custom-node.svelte -->
<!--
  CustomNode - Brief description of functionality

  Specialized wrapper around BaseNode that provides:
  - Feature 1
  - Feature 2
  - Feature 3
-->

<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import BaseNode from '$lib/design/components/base-node.svelte';
  import type { NodeViewerProps } from '$lib/types/nodeViewers.js';

  // Props following NodeViewerProps interface
  let {
    nodeId,
    content = '',
    autoFocus = false,
    nodeType = 'custom',
    inheritHeaderLevel = 0,
    children = []
  }: NodeViewerProps = $props();

  const dispatch = createEventDispatcher();

  // Custom node-specific logic here

  // Event forwarding helper
  function forwardEvent<T>(eventName: string) {
    return (event: CustomEvent<T>) => dispatch(eventName, event.detail);
  }
</script>

<!-- Custom UI elements if needed -->
{#if customUINeeded}
  <div class="custom-controls">
    <!-- Custom controls here -->
  </div>
{/if}

<!-- Wrapped BaseNode -->
<BaseNode
  {nodeId}
  {nodeType}
  {autoFocus}
  {content}
  headerLevel={inheritHeaderLevel}
  {children}
  on:createNewNode={forwardEvent('createNewNode')}
  on:contentChanged={forwardEvent('contentChanged')}
  <!-- Forward all other BaseNode events -->
/>

<style>
  .custom-controls {
    /* Custom styling */
  }
</style>
```

#### Viewer Component Template

```svelte
<!-- custom-node-viewer.svelte -->
<!--
  CustomNodeViewer - Brief description of viewer functionality

  Page-level viewer that wraps BaseNodeViewer with:
  - Navigation elements
  - Specialized UI
  - Custom controls
-->

<script lang="ts">
  import BaseNodeViewer from '$lib/design/components/base-node-viewer.svelte';

  // Viewer-specific props
  let { customProp = 'default' }: { customProp?: string } = $props();

  // Viewer-specific logic
</script>

<div class="custom-node-viewer">
  <!-- Custom viewer header/navigation -->
  <div class="viewer-header">
    <!-- Navigation, filters, controls, etc. -->
  </div>

  <!-- Node content area -->
  <div class="node-content-area">
    <BaseNodeViewer />
  </div>
</div>

<style>
  .custom-node-viewer {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }

  .viewer-header {
    flex-shrink: 0;
    /* Header styling */
  }

  .node-content-area {
    flex: 1;
    overflow-y: auto;
    /* Content area styling */
  }
</style>
```

## Advanced Patterns

### Metadata-Driven Extension (CodeBlockNode Example)

CodeBlockNode demonstrates how to extend BaseNode functionality without modifying its core API, using the metadata-driven extension pattern:

**Pattern Overview**:
- BaseNode accepts a `metadata` prop with arbitrary properties
- Node components can add custom metadata flags to control BaseNode behavior
- This maintains BaseNode's generality while enabling specialized behavior

**CodeBlockNode Implementation**:

```svelte
<script lang="ts">
  import BaseNode from '$lib/design/components/base-node.svelte';

  // Parse language from fence syntax
  let language = $state<string>(parseLanguage(content));

  // Create reactive metadata with custom flags
  let codeMetadata = $derived({
    language,
    disableMarkdown: true  // Tell BaseNode to skip markdown processing
  });
</script>

<BaseNode
  {nodeId}
  {nodeType}
  {content}
  metadata={codeMetadata}
  on:contentChanged
  on:createNewNode
/>
```

**BaseNode Metadata Extension Points**:
```typescript
/**
 * Metadata extension points supported by BaseNode:
 *
 * - disableMarkdown: boolean
 *   Skip markdown processing (e.g., for code blocks, raw text)
 *   Implementation: base-node.svelte lines 556-591
 *
 * - language: string
 *   Language identifier (e.g., for code blocks)
 *   Usage: Stored in node metadata, displayed in UI
 *
 * - headerLevel: number
 *   Header level 1-6 (for header nodes)
 *   Usage: Controls markdown heading syntax
 *
 * - taskState: string
 *   Task completion state (for task nodes)
 *   Values: 'pending' | 'inProgress' | 'completed'
 */
```

**Content Transformation Pattern (CodeBlockNode)**:

CodeBlockNode demonstrates a three-phase content transformation approach:

```svelte
<script lang="ts">
  // Phase 1: Storage format (with language in fence)
  // Stored: ```javascript\ncode\n```
  let internalContent = $state(content);

  // Phase 2: Edit format (fences without language)
  // User edits: ```\ncode\n``` (language managed by dropdown)
  function extractCodeForEditing(content: string): string {
    return content.replace(/^```\w+/, '```');
  }
  let editContent = $derived(extractCodeForEditing(internalContent));

  // Phase 3: Display format (no fences, just code)
  // User sees: code (fences become empty lines for spacing)
  function extractCodeForDisplay(content: string): string {
    let result = content.replace(/^```\w*/, '');
    result = result.replace(/```$/, '\n');
    return result;
  }
  let displayContent = $derived(extractCodeForDisplay(internalContent));
</script>

<BaseNode
  content={editContent}
  {displayContent}
  metadata={{ disableMarkdown: true }}
/>
```

**Benefits of This Pattern**:
- ✅ **No BaseNode modification** - Maintains core component stability
- ✅ **Type-specific behavior** - Each node type controls its own rendering
- ✅ **Clear separation** - Storage, editing, and display logic isolated
- ✅ **Reusable pattern** - Other node types can follow the same approach

**Leaf Node Pattern (CodeBlockNode)**:

Some node types should not have children. CodeBlockNode demonstrates proper leaf node implementation:

```typescript
// In plugin configuration (core-plugins.ts)
export const codeBlockNodePlugin: PluginDefinition = {
  config: {
    canHaveChildren: false,  // Leaf node constraint
    canBeChild: true         // Can be nested under other nodes
  }
};

// Backend validation (mod.rs - Rust)
pub struct CodeBlockNodeBehavior;

impl NodeBehavior for CodeBlockNodeBehavior {
    fn can_have_children(&self) -> bool {
        false  // Enforced at backend level
    }
}
```

**Icon Registry Configuration**:
```typescript
// Icon registry (registry.ts)
iconRegistry.register('code-block', {
  component: CodeBlockIcon,
  semanticClass: 'node-icon',
  colorVar: 'hsl(var(--node-text, 200 40% 45%))',
  hasState: false,
  hasRingEffect: false  // No ring because leaf nodes have no children
});
```

## Development Guidelines

### When to Create Each Component Type

**Create a Node Component When**:
- You need specialized behavior for individual nodes
- The node type has unique data or interaction patterns
- You need custom rendering or editing logic
- Example: TaskNode (checkbox, state), DateNode (formatting)

**Create a Viewer Component When**:
- You need page-level navigation or controls
- The viewing experience requires specialized UI
- You need custom layouts or organizational features
- Example: DateNodeViewer (date navigation), TaskNodeViewer (project views)

**Don't Create When**:
- BaseNodeViewer already handles the use case adequately
- The functionality could be added to an existing component
- Example: TextNode doesn't need a TextNodeViewer

### Event Handling Patterns

**Node Components** should forward all BaseNode events:
```typescript
// Standard event forwarding pattern
function forwardEvent<T>(eventName: string) {
  return (event: CustomEvent<T>) => dispatch(eventName, event.detail);
}

// Usage
on:contentChanged={forwardEvent('contentChanged')}
on:createNewNode={forwardEvent('createNewNode')}
```

**Viewer Components** handle their own UI events and delegate node management to BaseNodeViewer:
```typescript
// Viewer-specific event handlers
function handleNavigationClick() {
  // Custom navigation logic
}

function handleFilterChange(filter: string) {
  // Custom filtering logic
}
```

### Styling Conventions

**CSS Class Naming**:
```css
/* Component root */
.component-name-node { }      /* Node components */
.component-name-node-viewer { } /* Viewer components */

/* Sub-elements */
.component-name-node .controls { }
.component-name-node-viewer .header { }
.component-name-node-viewer .content-area { }
```

**Design System Integration**:
```css
/* Use CSS custom properties */
color: hsl(var(--foreground));
background: hsl(var(--background));
border: 1px solid hsl(var(--border));

/* Use semantic spacing */
padding: var(--spacing-4);
margin: var(--spacing-2);
```

## Testing Patterns

### Component Testing Strategy

```typescript
// Node component tests
describe('CustomNode', () => {
  test('forwards all BaseNode events', () => {
    // Test event forwarding
  });

  test('applies custom logic correctly', () => {
    // Test node-specific functionality
  });
});

// Viewer component tests
describe('CustomNodeViewer', () => {
  test('renders navigation correctly', () => {
    // Test viewer UI
  });

  test('integrates with BaseNodeViewer', () => {
    // Test BaseNodeViewer integration
  });
});
```

## Migration Guide

### Converting Old Components

If you have components that don't follow the new naming convention:

1. **Identify the component type** (Node or Viewer)
2. **Rename following the pattern**: `*Node` or `*NodeViewer`
3. **Update all imports** throughout the codebase
4. **Update plugin registrations**
5. **Update CSS class names** to match
6. **Test thoroughly** with type checking

### Example Migration

```typescript
// Before (incorrect naming)
import DatePageViewer from './date-page-viewer.svelte';
import TextNodeViewer from './text-node-viewer.svelte';

// After (correct naming)
import DateNodeViewer from './date-node-viewer.svelte';
import TextNode from './text-node.svelte';
```

## Type-Safe CRUD Architecture (Issue #709)

NodeSpace uses a **hybrid architecture** for schema property forms that provides:
- **Compile-time type safety** for core node types (task, date, entity, etc.)
- **Runtime flexibility** for user-defined types (dynamic schemas)

### The Hybrid Pattern

```
┌─────────────────────────────────────────────────────────────────────┐
│                        BaseNodeViewer                                │
│                                                                      │
│  ┌────────────────────────┐      ┌────────────────────────┐         │
│  │   Core Node Types      │      │   User-Defined Types   │         │
│  │   (compile-time)       │      │   (runtime schema)     │         │
│  └──────────┬─────────────┘      └──────────┬─────────────┘         │
│             │                               │                        │
│             ▼                               ▼                        │
│  ┌────────────────────────┐      ┌────────────────────────┐         │
│  │   TaskSchemaForm       │      │   SchemaPropertyForm   │         │
│  │   DateSchemaForm       │      │   (generic/dynamic)    │         │
│  │   EntitySchemaForm     │      │                        │         │
│  │   (typed, hardcoded)   │      │                        │         │
│  └──────────┬─────────────┘      └──────────┬─────────────┘         │
│             │                               │                        │
│             └───────────┬───────────────────┘                        │
│                         ▼                                            │
│           ┌─────────────────────────────────┐                        │
│           │  sharedNodeStore.updateNode()   │                        │
│           │  (smart routing via plugin)     │                        │
│           └──────────┬──────────────────────┘                        │
│                      │                                               │
│        ┌─────────────┼─────────────┐                                │
│        ▼             ▼             ▼                                │
│   updateTaskNode  updateNode   (other typed                         │
│   (spoke table)   (properties)  updaters)                           │
└─────────────────────────────────────────────────────────────────────┘
```

### Schema Form Components

#### Type-Specific Schema Forms (Core Types)

For core node types with spoke tables, create **hardcoded schema forms** with full TypeScript type safety:

```svelte
<!-- task-schema-form.svelte -->
<script lang="ts">
  import type { TaskNode, TaskStatus, TaskPriority } from '$lib/types';
  import { sharedNodeStore } from '$lib/services/shared-node-store.svelte';

  let { nodeId }: { nodeId: string } = $props();

  // Fully typed - IDE autocomplete, compile-time checking
  let node = $derived(sharedNodeStore.getNode(nodeId) as TaskNode | null);

  function updateStatus(status: TaskStatus) {
    // Type-safe update - TypeScript validates the shape
    sharedNodeStore.updateNode(nodeId, { status });
  }

  function updatePriority(priority: TaskPriority) {
    sharedNodeStore.updateNode(nodeId, { priority });
  }
</script>

{#if node}
  <div class="task-schema-form">
    <Select value={node.status} onchange={updateStatus}>
      <option value="open">Open</option>
      <option value="in_progress">In Progress</option>
      <option value="done">Done</option>
    </Select>

    <Select value={node.priority} onchange={updatePriority}>
      <option value={1}>Low</option>
      <option value={2}>Medium</option>
      <option value={3}>High</option>
    </Select>

    <DatePicker value={node.dueDate} onchange={(d) => sharedNodeStore.updateNode(nodeId, { dueDate: d })} />
  </div>
{/if}
```

**Benefits of hardcoded forms:**
- ✅ Full TypeScript inference (`node.status` is `TaskStatus`, not `unknown`)
- ✅ Compile-time error catching (typos, wrong field types)
- ✅ IDE autocomplete for all properties
- ✅ Optimized rendering (no schema lookup at runtime)
- ✅ Direct spoke field access (no nested `properties.task.status`)

#### Generic Schema Form (User-Defined Types)

For user-defined types (runtime schemas), use the **dynamic SchemaPropertyForm**:

```svelte
<!-- schema-property-form.svelte (existing) -->
<script lang="ts">
  // Generic - works with any schema-defined type
  let { nodeId, nodeType }: { nodeId: string; nodeType: string } = $props();

  // Runtime schema lookup
  let schema = $state<SchemaNode | null>(null);
  $effect(() => {
    loadSchema(nodeType);
  });

  function updateProperty(fieldName: string, value: unknown) {
    // Generic update - no type safety on field values
    sharedNodeStore.updateNode(nodeId, {
      properties: { [nodeType]: { [fieldName]: value } }
    });
  }
</script>

{#each schema?.fields as field}
  <!-- Dynamic field rendering based on schema -->
{/each}
```

### Smart Routing in SharedNodeStore

The `sharedNodeStore.updateNode()` method uses **plugin-based routing** to dispatch updates to the correct backend method:

```typescript
// shared-node-store.svelte.ts
class SharedNodeStore {
  // Cache updaters for O(1) lookup after first access
  private updaterCache = new Map<string, NodeUpdater>();

  async updateNode(nodeId: string, changes: Partial<Node>, source: UpdateSource) {
    const node = this.getNode(nodeId);
    if (!node) return;

    try {
      // Cached plugin lookup (O(1) after first access per type)
      let updater = this.updaterCache.get(node.nodeType);
      if (updater === undefined) {
        updater = pluginRegistry.getNodeUpdater(node.nodeType) ?? null;
        this.updaterCache.set(node.nodeType, updater);
      }

      if (updater) {
        // Type-specific path → spoke table
        return await updater.update(nodeId, node.version, changes);
      }

      // Generic path → properties JSON (hub table only)
      return await backendAdapter.updateNode(nodeId, node.version, { properties: changes });
    } catch (error) {
      // Unified error handling for both paths
      this.handleUpdateError(nodeId, error, source);
      throw error;
    }
  }

  // Also expose type-specific methods for explicit calls (tests, specialized workflows)
  async updateTaskNode(nodeId: string, changes: TaskNodeUpdate, source: UpdateSource) {
    return this.updateNode(nodeId, changes, source); // Routes through plugin
  }
}
```

**Design decisions:**
- **Updater caching**: Avoids repeated plugin lookups for hot paths
- **Unified error handling**: Both spoke and hub updates use same rollback/notification logic
- **Explicit type methods**: Export `updateTaskNode()` for tests and type-safe direct calls
- **Single pipeline**: All updates flow through same debouncing/conflict detection

### Plugin Registration for Type-Specific Updaters

```typescript
// core-plugins.ts
export const taskNodePlugin: PluginDefinition = {
  id: 'task',
  // ... existing config

  // NEW: Schema form component (lazy-loaded)
  schemaForm: {
    lazyLoad: () => import('../components/property-forms/task-schema-form.svelte'),
  },

  // NEW: Type-specific updater
  updater: {
    update: async (id: string, version: number, changes: TaskNodeUpdate) => {
      return backendAdapter.updateTaskNode(id, version, changes);
    }
  }
};
```

### Type-Safe Plugin Lookup

Use type guards in the plugin registry for full type safety:

```typescript
// plugin-registry.ts
export interface NodeUpdater<T extends Node = Node> {
  update: (id: string, version: number, changes: Partial<T>) => Promise<T>;
}

export function getNodeUpdater<T extends Node>(
  nodeType: string
): NodeUpdater<T> | null {
  const plugin = this.plugins.get(nodeType);
  return plugin?.updater ?? null;
}

// Usage with full type inference
const updater = pluginRegistry.getNodeUpdater<TaskNode>('task');
if (updater) {
  const result = await updater.update(nodeId, version, { status: 'done' }); // Fully typed!
}
```

### When to Use Each Pattern

| Scenario | Pattern | Example |
|----------|---------|---------|
| Core node type with spoke table | Type-specific SchemaForm | TaskSchemaForm, DateSchemaForm |
| Core node type without spoke table | SchemaPropertyForm (generic) | TextNode (hub-only) |
| User-defined type | SchemaPropertyForm (generic) | Custom "recipe" or "workout" types |
| New core type being added | Create type-specific form | EntitySchemaForm (planned) |

### Implementation Checklist for New Core Types

When adding a new core node type with type-safe CRUD:

1. **Backend (Rust)**
   - [ ] Create `TypeNodeUpdate` struct in `models/`
   - [ ] Add `update_type_node()` to `SurrealStore`
   - [ ] Add `update_type_node()` to `NodeService`
   - [ ] Add Tauri command in `commands/nodes.rs`
   - [ ] Register command in `lib.rs`

2. **Frontend (TypeScript)**
   - [ ] Create `TypeNode` interface in `types/`
   - [ ] Create `TypeNodeUpdate` interface
   - [ ] Add to BackendAdapter interface and implementations
   - [ ] Add wrapper in `tauri-commands.ts`
   - [ ] Create `type-schema-form.svelte` component
   - [ ] Register in plugin system with `schemaForm` and `updater`

3. **Integration**
   - [ ] Update BaseNodeViewer to use plugin's schema form
   - [ ] Ensure `sharedNodeStore.updateNode()` routes correctly
   - [ ] Add tests for type-safe update flow

### Data Flow Example: Task Status Update

```
1. User clicks task status dropdown in TaskSchemaForm
   ↓
2. TaskSchemaForm calls sharedNodeStore.updateNode(nodeId, { status: 'in_progress' })
   ↓
3. sharedNodeStore detects nodeType === 'task', looks up plugin
   ↓
4. Plugin's updater.update() calls backendAdapter.updateTaskNode()
   ↓
5. TauriAdapter invokes 'update_task_node' command
   ↓
6. Rust NodeService::update_task_node() validates TaskNodeUpdate
   ↓
7. SurrealStore updates spoke table (task) with atomic transaction
   ↓
8. Returns updated TaskNode with new version
   ↓
9. sharedNodeStore updates local state, notifies subscribers
   ↓
10. TaskSchemaForm re-renders with new status (reactive)
```

### Incremental Implementation Strategy

Implement this architecture **progressively** to avoid big-bang refactoring:

**Phase 1: Plugin Registration (no behavior change)**
```typescript
// Add to taskNodePlugin - SchemaPropertyForm still used
taskNodePlugin.schemaForm = { lazyLoad: () => import('./task-schema-form.svelte') };
```

**Phase 2: TaskSchemaForm Component**
- Create hardcoded form with full type safety
- Test alongside existing SchemaPropertyForm as fallback
- BaseNodeViewer selects form via plugin lookup

**Phase 3: Type-Specific Updater**
```typescript
// Add updater - routes through sharedNodeStore.updateNode()
taskNodePlugin.updater = {
  update: (id, version, changes) => backendAdapter.updateTaskNode(id, version, changes)
};
```

**Phase 4: Repeat for Other Core Types**
- DateSchemaForm (if date nodes have spoke fields)
- EntitySchemaForm (planned)
- Each type follows same pattern

This approach allows **per-type validation** and avoids breaking existing functionality.

### BaseNodeViewer Schema Form Selection

```svelte
<!-- base-node-viewer.svelte -->
{#if currentViewedNode && nodeId}
  {@const SchemaFormComponent = pluginRegistry.getSchemaForm(currentViewedNode.nodeType)}

  {#if SchemaFormComponent}
    <!-- Type-specific form (TaskSchemaForm, etc.) -->
    <svelte:component this={SchemaFormComponent} {nodeId} />
  {:else}
    <!-- Fallback to generic form for user-defined types -->
    <SchemaPropertyForm {nodeId} nodeType={currentViewedNode.nodeType} />
  {/if}
{/if}
```

### Relation to Node Behavior System

This pattern complements the [Node Behavior System](../business-logic/node-behavior-system.md):

- **Behaviors** (Rust): Validation, deletion rules, computed content
- **Schema Forms** (Svelte): UI for editing spoke table fields
- **Both**: Compile-time type safety for core types, runtime flexibility for extensions

The hybrid approach ensures:
- Core types are **fast and type-safe** (hardcoded UI + typed backend)
- User types are **flexible and extensible** (dynamic UI + schema validation)

### Important: Dual Update Paths

Document clearly for future developers:

| Update Path | When Used | Backend Method |
|-------------|-----------|----------------|
| **Plugin updater** | Core types with spoke tables | `updateTaskNode()`, `updateDateNode()`, etc. |
| **Generic update** | Hub-only types, user-defined types | `updateNode()` with properties JSON |

Both paths share:
- Same debouncing logic
- Same optimistic update/rollback
- Same error handling and notifications
- Same version conflict detection (OCC)

## Summary

This architecture provides:
- **Consistent naming** that indicates component purpose
- **Clear separation** between node-level and page-level concerns
- **Extensible patterns** for new node types
- **Maintainable code** with predictable structure
- **Type safety** with proper interfaces
- **Hybrid CRUD pattern** - compile-time safety for core types, runtime flexibility for user types

Follow these patterns for all new component development to maintain architectural consistency and developer productivity.