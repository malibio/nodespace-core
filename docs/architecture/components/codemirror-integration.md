# CodeMirror Integration Architecture

**Last Updated:** 2025-08-11  
**Status:** Active  
**Related ADRs:** ADR-001, ADR-003, ADR-004

## Overview

NodeSpace uses **CodeMirror 6** as the universal text editing foundation for all node types. This provides professional editing experiences, native click-to-cursor positioning, and a foundation for hybrid markdown rendering.

## Integration Strategy

### Universal Editor Approach
- **All node types** use CodeMirror with different configurations
- **Single editor implementation** instead of multiple systems (textarea, contenteditable, etc.)
- **Consistent UX** across all editing experiences
- **Native positioning** eliminates need for custom cursor positioning code

### Configuration-Based Specialization
```typescript
// Different node types = different CodeMirror configs
interface EditorConfig {
  multiline: boolean;    // Single-line vs multiline editing
  markdown: boolean;     // Syntax highlighting on/off  
  editable: boolean;     // Read-only vs editable
  extensions: Extension[]; // CodeMirror extensions for features
}
```

## Architecture Components

### 1. CodeMirrorEditor.svelte
**Purpose:** Svelte wrapper for CodeMirror 6 with NodeSpace integration

```typescript
interface CodeMirrorEditorProps {
  // Content
  content: string;              // Text content (bind:content)
  
  // Editor Configuration  
  multiline: boolean;           // false = single-line, true = multiline
  markdown: boolean;            // false = plain text, true = markdown syntax
  editable: boolean;            // false = read-only, true = editable
  
  // Styling
  className: string;            // Additional CSS classes
  
  // Events (debounced)
  on:contentChanged: (event: { detail: { content: string } }) => void;
}
```

**Key Features:**
- **Debounced events:** Uses CodeMirror's native debouncing (~300-500ms)
- **Configuration-driven:** Extensions loaded based on props
- **Lifecycle management:** Proper creation/destruction handling
- **Svelte integration:** Reactive props and event dispatching

### 2. BaseNode Integration
**Purpose:** Universal editing foundation using CodeMirror

```svelte
<!-- BaseNode.svelte -->
<script lang="ts">
  // Configuration props with defaults
  export let multiline: boolean = false;   // Single-line by default
  export let markdown: boolean = false;    // Plain text by default  
  export let contentEditable: boolean = true; // Editable by default
</script>

<!-- Always-editing mode: CodeMirror always visible -->
<div class="ns-node">
  <CodeMirrorEditor 
    {content}
    {multiline}
    {markdown}
    editable={contentEditable}
    className="ns-node__editor"
    on:contentChanged={handleContentChanged}
  />
</div>
```

### 3. Node Type Specialization
**Purpose:** Different node types override BaseNode configuration

```svelte
<!-- TextNode: Multiline + Markdown -->
<BaseNode multiline={true} markdown={true} />

<!-- PersonNode: Read-only -->  
<BaseNode contentEditable={false} />

<!-- TaskNode: Single-line, Plain text -->
<BaseNode multiline={false} markdown={false} />
```

## CodeMirror Configuration

### Core Packages
```json
{
  "dependencies": {
    "@codemirror/view": "^6.38.1",      // Core editor view
    "@codemirror/state": "^6.5.2",      // State management  
    "@codemirror/lang-markdown": "^6.3.4" // Markdown language support
  }
}
```

**Bundle Size Impact:** ~70-80KB (well under 200KB target)

### Extension Configuration
```javascript
import { EditorView } from '@codemirror/view';
import { EditorState } from '@codemirror/state';
import { markdown } from '@codemirror/lang-markdown';

function createExtensions(multiline, markdownEnabled, editable) {
  const extensions = [
    // Base functionality
    EditorView.theme({
      '&': { fontSize: '14px', lineHeight: '1.4' },
      '.cm-content': { padding: '8px' },
      '.cm-focused': { outline: 'none' }
    }),
    
    // Debounced content change detection
    EditorView.updateListener.of((update) => {
      if (update.docChanged) {
        // Already debounced by CodeMirror (~300-500ms)
        dispatch('contentChanged', { 
          content: update.state.doc.toString() 
        });
      }
    }),
    
    // Conditional extensions
    ...(markdownEnabled ? [markdown()] : []),
    ...(multiline ? [] : [singleLineMode()]),
    ...(editable ? [] : [EditorState.readOnly.of(true)])
  ];
  
  return extensions;
}
```

### Single-Line Mode
```javascript
// Restrict editor to single line for BaseNode/TaskNode
function singleLineMode() {
  return [
    EditorView.theme({
      '.cm-content': {
        whiteSpace: 'nowrap',
        overflowX: 'auto', 
        overflowY: 'hidden'
      }
    }),
    EditorView.domEventHandlers({
      keydown: (event, view) => {
        if (event.key === 'Enter' && !event.shiftKey) {
          event.preventDefault();
          view.contentDOM.blur(); // Exit edit mode
          return true;
        }
        return false;
      }
    })
  ];
}
```

### Markdown Mode
```javascript
// Enable syntax highlighting for TextNode
import { markdown, markdownLanguage } from '@codemirror/lang-markdown';

function markdownMode() {
  return [
    markdown({
      base: markdownLanguage,
      // Future: Could add custom NodeSpace markdown extensions
    }),
    
    // Future: Hybrid rendering extensions will go here
    // EditorView.theme({ 
    //   '.cm-header1': { fontSize: '2rem' }  // Issue #47
    // })
  ];
}
```

## Event Architecture

### Debounced Content Changes
CodeMirror handles debouncing internally, firing events only when users pause typing:

```
User Types → CodeMirror Debouncing → contentChanged Event → Save Logic
   (immediate)      (~300-500ms)           (debounced)        (immediate)
```

### Event Flow
```svelte
<!-- CodeMirrorEditor.svelte -->
<script>
  const extensions = [
    EditorView.updateListener.of((update) => {
      if (update.docChanged && !isInternalUpdate) {
        // This is already debounced by CodeMirror
        dispatch('contentChanged', { 
          content: update.state.doc.toString() 
        });
      }
    })
  ];
</script>

<!-- BaseNode.svelte -->
<CodeMirrorEditor on:contentChanged={handleContentChanged} />

<script>
  function handleContentChanged(event) {
    content = event.detail.content;
    dispatch('contentChanged', { nodeId, content }); // Bubble up
  }
</script>

<!-- TextNode.svelte -->  
<BaseNode on:contentChanged={handleSave} />

<script>
  function handleSave(event) {
    // Event is already debounced - can save immediately
    if (autoSave) {
      saveToBackend(event.detail.content);
    }
  }
</script>
```

## Visual Integration

### Design System Integration
```css
/* CodeMirrorEditor.svelte */
<style>
  .codemirror-wrapper {
    width: 100%;
    font-family: inherit;
    font-size: inherit;
    line-height: inherit;
  }

  /* Editable state */
  .codemirror-wrapper :global(.cm-editor) {
    background: transparent;
    color: hsl(var(--foreground));
  }

  /* Read-only state */
  .codemirror-wrapper--readonly :global(.cm-editor) {
    color: hsl(var(--muted-foreground));
    cursor: default;
  }

  /* Focus states */  
  .codemirror-wrapper :global(.cm-focused) {
    outline: none;
    /* BaseNode handles focus styling */
  }
</style>
```

### Theme System
```javascript
// NodeSpace theme for consistent styling
const nodeSpaceTheme = EditorView.theme({
  '&': {
    fontSize: 'var(--ns-font-size-base, 14px)',
    fontFamily: 'var(--ns-font-family, inherit)',
    lineHeight: 'var(--ns-line-height, 1.4)'
  },
  '.cm-content': {
    padding: '8px',
    color: 'hsl(var(--foreground))',
    backgroundColor: 'transparent',
    caretColor: 'hsl(var(--foreground))'
  },
  '.cm-focused': {
    outline: 'none' // Use NodeSpace focus styling
  },
  // Read-only styling
  '&.cm-readonly': {
    color: 'hsl(var(--muted-foreground))'
  }
});
```

## Performance Considerations

### Bundle Optimization
```javascript
// Tree-shake unused CodeMirror features
import { EditorView } from '@codemirror/view';        // ~40KB
import { EditorState } from '@codemirror/state';      // ~20KB  
import { markdown } from '@codemirror/lang-markdown'; // ~10KB

// Avoid importing entire packages
// ❌ import * as CodeMirror from 'codemirror';
// ✅ Import only what's needed
```

### Memory Management
```svelte
<!-- CodeMirrorEditor.svelte -->
<script>
  import { onMount, onDestroy } from 'svelte';
  
  let editorView: EditorView | null = null;
  
  onMount(() => {
    // Create editor
    editorView = new EditorView({
      state: EditorState.create({ /* config */ }),
      parent: containerElement
    });
  });
  
  onDestroy(() => {
    // Critical: Clean up editor to prevent memory leaks
    if (editorView) {
      editorView.destroy();
      editorView = null;
    }
  });
</script>
```

### Update Performance
```javascript
// Efficient content updates
$: if (editorView && content !== editorView.state.doc.toString()) {
  // Prevent infinite update loops
  isInternalUpdate = true;
  
  editorView.dispatch({
    changes: {
      from: 0,
      to: editorView.state.doc.length,
      insert: content
    }
  });
  
  isInternalUpdate = false;
}
```

## Future Extensions

### Issue #47: Hybrid Markdown Rendering
```javascript
// Future: Add hybrid rendering theme
const hybridMarkdownTheme = EditorView.theme({
  '.cm-header1': { 
    fontSize: '2rem', 
    fontWeight: '600',
    lineHeight: '1.38em' 
  },
  '.cm-header2': { 
    fontSize: '1.5rem', 
    fontWeight: '600',
    lineHeight: '1.38em' 
  }
  // ... other heading levels
});
```

### Issue #34: Rich Decorations
```javascript
// Future: Add rich decorations for links, images, etc.
const richDecorations = ViewPlugin.fromClass(class {
  decorations: DecorationSet;
  
  constructor(view: EditorView) {
    this.decorations = this.buildDecorations(view);
  }
  
  buildDecorations(view: EditorView) {
    // Add interactive decorations for links, images, etc.
  }
});
```

### Custom NodeSpace Extensions
```javascript
// Future: NodeSpace-specific features
const nodeSpaceExtensions = [
  // Bi-directional links
  biDirectionalLinks(),
  
  // Entity mentions
  entityMentions(),
  
  // Task shortcuts
  taskKeyboardShortcuts(),
  
  // AI integration
  aiCompletions()
];
```

## Testing Strategy

### Component Testing
```typescript
// Test CodeMirror integration
test('CodeMirrorEditor dispatches debounced events', async () => {
  const { component } = render(CodeMirrorEditor, {
    content: '',
    multiline: false,
    markdown: false,
    editable: true
  });
  
  const mockHandler = vi.fn();
  component.$on('contentChanged', mockHandler);
  
  // Simulate typing (multiple rapid changes)
  await typeText(component, 'Hello world');
  
  // Should not fire immediately (debounced)
  expect(mockHandler).not.toHaveBeenCalled();
  
  // Should fire after debounce period
  await vi.advanceTimersByTime(500);
  expect(mockHandler).toHaveBeenCalledOnce();
  expect(mockHandler).toHaveBeenCalledWith(
    expect.objectContaining({
      detail: { content: 'Hello world' }
    })
  );
});
```

### Integration Testing
```typescript
// Test BaseNode + CodeMirror integration
test('BaseNode content changes trigger saves', async () => {
  const { component } = render(BaseNode, {
    nodeId: 'test',
    content: '',
    contentEditable: true
  });
  
  const mockSave = vi.fn();
  component.$on('contentChanged', mockSave);
  
  // Type content
  await typeInEditor(component, 'Test content');
  
  // Should trigger save after debounce
  await waitForDebounce();
  expect(mockSave).toHaveBeenCalledWith(
    expect.objectContaining({
      detail: { nodeId: 'test', content: 'Test content' }
    })
  );
});
```

## References

- **ADR-003:** Universal CodeMirror Strategy
- **ADR-004:** Debounced Events Architecture  
- [CodeMirror 6 Documentation](https://codemirror.net/docs/)
- Issue #26: Hybrid Markdown Rendering System
- Issue #46: CodeMirror Foundation Setup