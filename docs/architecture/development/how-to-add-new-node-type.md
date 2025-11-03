# How to Add a New Node Type

## Overview

This guide provides a **complete checklist** for adding a new node type to NodeSpace. It's based on lessons learned from implementing HeaderNode (#275) and CodeBlockNode (#276), where we discovered missing steps that caused issues.

**Use this checklist for EVERY new node type to ensure nothing is missed.**

---

## Complete Implementation Checklist

### 1. Backend (Rust) - Behavior System

**File:** `packages/core/src/behaviors/mod.rs`

#### 1.1 Define Node Behavior Struct

```rust
/// Behavior for [YourNodeType] nodes
///
/// # Characteristics
/// - Can have children: [true/false]
/// - Supports markdown: [true/false]
/// - [Other characteristics]
///
/// # Validation Rules
/// - [Rule 1]
/// - [Rule 2]
pub struct YourNodeTypeBehavior;
```

#### 1.2 Implement NodeBehavior Trait

```rust
impl NodeBehavior for YourNodeTypeBehavior {
    fn can_have_children(&self) -> bool {
        // true for parent nodes (text, header, task, date)
        // false for leaf nodes (code-block)
        true
    }

    fn supports_markdown(&self) -> bool {
        // true for text-based content
        // false for structured content (tasks, dates, code-blocks)
        true
    }

    fn default_metadata(&self) -> serde_json::Value {
        json!({
            // Add any default metadata fields
            // Example: "language": "plaintext" for code blocks
        })
    }

    fn validate(&self, node: &Node) -> Result<(), String> {
        // Add validation logic
        // Example: Check content format, required metadata, etc.

        if node.content.trim().is_empty() {
            return Err("Content cannot be empty".to_string());
        }

        Ok(())
    }
}
```

#### 1.3 Register in NodeBehaviorRegistry

```rust
impl NodeBehaviorRegistry {
    pub fn new() -> Self {
        let mut registry = HashMap::new();

        // ... existing registrations ...

        registry.insert(
            "your-node-type".to_string(),
            Box::new(YourNodeTypeBehavior) as Box<dyn NodeBehavior>
        );

        Self { registry }
    }
}
```

#### 1.4 Add Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_your_node_type_can_have_children() {
        let behavior = YourNodeTypeBehavior;
        assert_eq!(behavior.can_have_children(), true); // or false
    }

    #[test]
    fn test_your_node_type_supports_markdown() {
        let behavior = YourNodeTypeBehavior;
        assert_eq!(behavior.supports_markdown(), true); // or false
    }

    #[test]
    fn test_your_node_type_default_metadata() {
        let behavior = YourNodeTypeBehavior;
        let metadata = behavior.default_metadata();

        // Assert expected metadata structure
        assert!(metadata.is_object());
    }

    #[test]
    fn test_your_node_type_validation() {
        let behavior = YourNodeTypeBehavior;

        // Test valid node
        let valid_node = Node::new(
            "your-node-type".to_string(),
            "Valid content".to_string(),
            None,
            json!({})
        );
        assert!(behavior.validate(&valid_node).is_ok());

        // Test invalid node
        let invalid_node = Node::new(
            "your-node-type".to_string(),
            "".to_string(), // Invalid: empty
            None,
            json!({})
        );
        assert!(behavior.validate(&invalid_node).is_err());
    }
}
```

---

### 2. Backend - Constants

**File:** `packages/desktop-app/src-tauri/src/constants.rs`

#### 2.1 Add to ALLOWED_NODE_TYPES

```rust
/// Valid node types supported by the system
///
/// - "text": Basic text nodes
/// - "header": Markdown headers (h1-h6)
/// - "task": Task/todo nodes with completion status
/// - "date": Date-based nodes for daily notes
/// - "code-block": Code blocks with language selection
/// - "your-node-type": [Description]  // ← ADD THIS
pub const ALLOWED_NODE_TYPES: &[&str] = &[
    "text",
    "header",
    "task",
    "date",
    "code-block",
    "your-node-type"  // ← ADD THIS
];
```

**⚠️ CRITICAL:** Forgetting this step causes HTTP 400 errors when creating nodes!

---

### 3. Frontend - Icon Component

**File:** `packages/desktop-app/src/lib/design/icons/components/your-node-type-icon.svelte`

#### 3.1 Create Icon Component

```svelte
<!--
  YourNodeTypeIcon - Icon for Your Node Type

  Design System Reference: docs/design-system/components.html → [Section]
-->

<script lang="ts">
  let {
    size = 20,
    hasChildren = false,
    className = ''
  }: {
    size?: number;
    hasChildren?: boolean;
    className?: string;
  } = $props();
</script>

<div
  class="ns-icon your-node-type-icon {className}"
  class:your-node-type-icon-with-ring={hasChildren}
  style="--icon-size: {size}px"
  role="img"
  aria-label="Your node type icon"
>
  {#if hasChildren}
    <div class="node-ring"></div>
  {/if}

  <!-- Your icon SVG/CSS here -->
  <div class="icon-content">
    <!-- Icon implementation -->
  </div>
</div>

<style>
  .ns-icon {
    width: var(--icon-size, 20px);
    height: var(--icon-size, 20px);
    position: relative;
    display: block;
    flex-shrink: 0;
  }

  .your-node-type-icon-with-ring {
    width: calc(var(--icon-size, 20px) + 4px);
    height: calc(var(--icon-size, 20px) + 4px);
  }

  .icon-content {
    /* Your icon styling */
  }

  /* Parent node ring (optional - only if canHaveChildren: true) */
  .node-ring {
    width: 20px;
    height: 20px;
    border-radius: 50%;
    border: 2px solid hsl(var(--node-text) / 0.5);
    box-sizing: border-box;
    position: absolute;
    top: 0;
    left: 0;
  }
</style>
```

---

### 4. Frontend - Icon Registry

**File:** `packages/desktop-app/src/lib/design/icons/registry.ts`

#### 4.1 Import Icon Component

```typescript
import YourNodeTypeIcon from './components/your-node-type-icon.svelte';
```

#### 4.2 Register Icon

```typescript
class IconRegistry {
  constructor() {
    // ... existing registrations ...

    // Your node type - [brief description]
    this.register('your-node-type', {
      component: YourNodeTypeIcon,
      semanticClass: 'node-icon',
      colorVar: 'hsl(var(--node-text, 200 40% 45%))',
      hasState: false, // true for task nodes with completion states
      hasRingEffect: true // false for leaf nodes (cannot have children)
    });
  }
}
```

**Key decisions:**
- `hasState: true` → Node has multiple visual states (like task: pending/inProgress/completed)
- `hasRingEffect: true` → Node can have children (shows ring when it does)
- `hasRingEffect: false` → Leaf node, cannot have children

---

### 5. Frontend - Node Component

**File:** `packages/desktop-app/src/lib/design/components/your-node-type-node.svelte`

#### 5.1 Create Node Component

```svelte
<!--
  YourNodeTypeNode - Wraps BaseNode with [specific functionality]

  Responsibilities:
  - [Feature 1]
  - [Feature 2]
  - [Feature 3]

  Integration:
  - Uses icon registry for proper icon rendering
  - Maintains compatibility with BaseNode API
  - Works seamlessly in node tree structure

  Design System Reference: docs/design-system/components.html → [Section]
-->

<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import BaseNode from './base-node.svelte';

  // Props using Svelte 5 runes mode - same interface as BaseNode
  let {
    nodeId,
    nodeType = 'your-node-type',
    autoFocus = false,
    content = '',
    children = []
  }: {
    nodeId: string;
    nodeType?: string;
    autoFocus?: boolean;
    content?: string;
    children?: string[];
  } = $props();

  const dispatch = createEventDispatcher();

  // Node-specific state and logic here

  // Configure editable behavior
  const editableConfig = {
    allowMultiline: false // true for multiline editing (Shift+Enter)
  };

  // Create metadata object (if needed)
  let nodeMetadata = $derived({
    // Add metadata fields
    // Example: disableMarkdown: true for code blocks
  });

  /**
   * Event forwarding helper
   */
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
  {children}
  {editableConfig}
  metadata={nodeMetadata}
  on:createNewNode={forwardEvent('createNewNode')}
  on:contentChanged={forwardEvent('contentChanged')}
  on:indentNode={forwardEvent('indentNode')}
  on:outdentNode={forwardEvent('outdentNode')}
  on:navigateArrow={forwardEvent('navigateArrow')}
  on:combineWithPrevious={forwardEvent('combineWithPrevious')}
  on:deleteNode={forwardEvent('deleteNode')}
  on:focus={forwardEvent('focus')}
  on:blur={forwardEvent('blur')}
  on:nodeReferenceSelected={forwardEvent('nodeReferenceSelected')}
  on:slashCommandSelected={forwardEvent('slashCommandSelected')}
  on:nodeTypeChanged={forwardEvent('nodeTypeChanged')}
  on:iconClick={forwardEvent('iconClick')}
/>

<style>
  /* Component-specific styling */
</style>
```

---

### 6. Frontend - Plugin System

**File:** `packages/desktop-app/src/lib/plugins/core-plugins.ts`

#### 6.1 Define Plugin

```typescript
export const yourNodeTypePlugin: PluginDefinition = {
  id: 'your-node-type',
  name: 'Your Node Type',
  description: 'Brief description of functionality',
  version: '1.0.0',
  config: {
    slashCommands: [
      {
        id: 'your-command',
        name: 'Your Command',
        description: 'Create a [node type]',
        shortcut: '/shortcut', // Optional keyboard shortcut
        contentTemplate: '', // Default content when created via slash command
        nodeType: 'your-node-type'
      }
    ],
    // Optional: Pattern detection for auto-conversion
    // See: docs/architecture/development/pattern-detection-and-templates.md
    patternDetection: [
      {
        pattern: /^your-pattern-regex/,
        targetNodeType: 'your-node-type',

        // Choose ONE content handling strategy:
        contentTemplate: '',          // Option 1: Apply template (e.g., '```\n\n```' for code blocks)
        // cleanContent: true,         // Option 2: Remove pattern from content
        // cleanContent: false,        // Option 3: Keep pattern in content (default)

        extractMetadata: (match: RegExpMatchArray) => ({
          // Extract metadata from pattern match
        }),
        desiredCursorPosition: 0,     // Optional: Cursor position after conversion
        priority: 10
      }
    ],
    canHaveChildren: true, // false for leaf nodes
    canBeChild: true
  },
  node: {
    lazyLoad: () => import('../design/components/your-node-type-node.svelte'),
    priority: 1
  },
  reference: {
    component: BaseNodeReference as NodeReferenceComponent,
    priority: 1
  }
};
```

#### 6.2 Export in corePlugins Array

```typescript
export const corePlugins = [
  textNodePlugin,
  headerNodePlugin,
  taskNodePlugin,
  aiChatNodePlugin,
  dateNodePlugin,
  codeBlockNodePlugin,
  yourNodeTypePlugin // ← ADD THIS
];
```

---

### 7. Frontend - Tests

**File:** `packages/desktop-app/src/tests/plugins/your-node-type-plugin.test.ts`

#### 7.1 Create Plugin Tests

```typescript
/**
 * YourNodeType Tests
 *
 * Tests for [node type] plugin functionality including:
 * - Pattern detection
 * - Plugin registration
 * - Slash command configuration
 */

import { describe, it, expect } from 'vitest';
import { pluginRegistry } from '$lib/plugins/plugin-registry';

describe('YourNodeType Plugin', () => {
  describe('Pattern Detection', () => {
    it('should detect pattern and extract metadata', () => {
      const content = 'your pattern here';
      const detection = pluginRegistry.detectPatternInContent(content);

      expect(detection).not.toBeNull();
      expect(detection?.config.targetNodeType).toBe('your-node-type');
      // Assert extracted metadata
    });
  });

  describe('Plugin Registration', () => {
    it('should have plugin registered', () => {
      expect(pluginRegistry.hasPlugin('your-node-type')).toBe(true);
    });

    it('should have correct configuration', () => {
      const plugin = pluginRegistry.getPlugin('your-node-type');

      expect(plugin?.config.canHaveChildren).toBe(true); // or false
      expect(plugin?.config.canBeChild).toBe(true);
    });
  });

  describe('Slash Command', () => {
    it('should register slash command', () => {
      const commands = pluginRegistry.getAllSlashCommands();
      const command = commands.find(c => c.nodeType === 'your-node-type');

      expect(command).toBeDefined();
      expect(command?.id).toBe('your-command');
    });
  });
});
```

---

### 8. Documentation

#### 8.1 Update Component Architecture Guide

**File:** `docs/architecture/components/component-architecture-guide.md`

Add to "Current Implementations" section:

```markdown
- **YourNodeTypeNode** (`src/lib/design/components/your-node-type-node.svelte`)
  - [Feature 1]
  - [Feature 2]
  - [Specific characteristics]
```

If this is a complex node with unique patterns, add to "Advanced Patterns" section.

#### 8.2 Update CLAUDE.md (Optional)

If this node type introduces new architectural patterns, document them in `CLAUDE.md`.

---

## Common Pitfalls & Lessons Learned

### ❌ Missing ALLOWED_NODE_TYPES Entry
**Symptom:** HTTP 400 Bad Request when creating nodes via API
**Fix:** Add to `packages/desktop-app/src-tauri/src/constants.rs`

### ❌ Icon Not Registered
**Symptom:** Default icon appears instead of custom icon
**Fix:** Import and register in `packages/desktop-app/src/lib/design/icons/registry.ts`

### ❌ Plugin Not Exported
**Symptom:** Slash commands don't appear, pattern detection doesn't work
**Fix:** Add to `corePlugins` array in `packages/desktop-app/src/lib/plugins/core-plugins.ts`

### ❌ Behavior Not Registered
**Symptom:** Backend validation fails, metadata not initialized
**Fix:** Register in `NodeBehaviorRegistry::new()` in `packages/core/src/behaviors/mod.rs`

### ❌ Wrong hasRingEffect Setting
**Symptom:** Ring appears on leaf nodes or missing on parent nodes
**Fix:** Set `hasRingEffect: false` for leaf nodes (`canHaveChildren: false`)

### ❌ Tests Not Added
**Symptom:** Regressions in future changes, incomplete validation
**Fix:** Add comprehensive tests for plugin, behavior, and component

---

## Validation Checklist

Before considering the node type implementation complete:

- [ ] Backend behavior implemented and registered
- [ ] Backend tests passing
- [ ] ALLOWED_NODE_TYPES updated
- [ ] Icon component created
- [ ] Icon registered in registry
- [ ] Node component created (wraps BaseNode)
- [ ] Plugin defined and exported
- [ ] Frontend tests created and passing
- [ ] Documentation updated
- [ ] Manual testing: Create node via slash command
- [ ] Manual testing: Create node via pattern detection (if applicable)
- [ ] Manual testing: Indent/outdent operations work correctly
- [ ] Manual testing: Icon displays correctly (with ring if applicable)
- [ ] Quality checks passing (`bun run quality:fix`)
- [ ] All tests passing (`bun run test`)

---

## Example Implementations

For reference implementations, see:

- **HeaderNode** (Issue #275) - Text node with header levels
- **CodeBlockNode** (Issue #276) - Leaf node with language selection
- **TaskNode** - Node with state management (pending/inProgress/completed)

---

## Questions or Issues?

If you encounter problems not covered by this guide:

1. Check existing node type implementations for patterns
2. Review recent issues for similar problems
3. Consult these guides:
   - `docs/architecture/components/component-architecture-guide.md`
   - `docs/architecture/development/pattern-detection-and-templates.md` (for pattern detection issues)
4. Create a GitHub issue with the `documentation` label
