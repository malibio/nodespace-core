# Extended Markdown Blocks - Implementation Guide

## Overview

This guide provides step-by-step implementation details for adding code blocks (```) and quote blocks (>) as extended markdown support in NodeSpace. The feature is broken down into 6 separate GitHub issues for manageable implementation.

## Architecture Summary

- **Node Type**: Text node variants (not separate node types)
- **Detection**: Content pattern matching (starts with ``` or >)
- **Activation**: Slash commands (/code, /quote) and direct shortcuts
- **Multi-line**: Shift+Enter support with smart behaviors
- **Visual States**: Show/hide syntax markers on focus/blur

---

## Issue #1: Add CSS and Icons for Code/Quote Blocks

### File to Modify
`packages/desktop-app/src/app.css`

### Implementation
Add after existing header square icons (~line 280):

```css
/* Code Block and Quote Block Square Icons */
.code-square-alt,
.quote-square-alt {
  width: 16px;
  height: 16px;
  position: absolute;
  top: 2px;
  left: 2px;
  border-radius: 0.5px;
  overflow: hidden;
}

.code-square-alt::before,
.quote-square-alt::before {
  content: '';
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  background: hsl(var(--primary));
}

.code-square-alt::after,
.quote-square-alt::after {
  content: '';
  position: absolute;
  top: 50%;
  left: 50%;
  width: 14px;
  height: 14px;
  transform: translate(-50%, -50%);
  background-size: contain;
  background-position: center;
  background-repeat: no-repeat;
}

/* Code block icon - braces {} */
.code-square-alt::after {
  background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 20 20' fill='%23FAF9F5'%3E%3Cpath d='M5,2.5 C3.34,2.5 2,3.84 2,5.5 V6.5 C2,7.33 1.33,8 0.5,8 H0 V12 H0.5 C1.33,12 2,12.67 2,13.5 V14.5 C2,16.16 3.34,17.5 5,17.5 H7 V15.5 H5 C4.45,15.5 4,15.05 4,14.5 V13.5 C4,11.84 2.66,10.5 1,10.5 V9.5 C2.66,9.5 4,8.16 4,6.5 V5.5 C4,4.95 4.45,4.5 5,4.5 H7 V2.5 H5 Z M15,2.5 H13 V4.5 H15 C15.55,4.5 16,4.95 16,5.5 V6.5 C16,8.16 17.34,9.5 19,9.5 V10.5 C17.34,10.5 16,11.84 16,13.5 V14.5 C16,15.05 15.55,15.5 15,15.5 H13 V17.5 H15 C16.66,17.5 18,16.16 18,14.5 V13.5 C18,12.67 18.67,12 19.5,12 H20 V8 H19.5 C18.67,8 18,7.33 18,6.5 V5.5 C18,3.84 16.66,2.5 15,2.5 Z'/%3E%3C/svg%3E");
}

/* Quote block icon - quotation marks */
.quote-square-alt::after {
  background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='%23FAF9F5'%3E%3Cpath d='M18.62 18h-5.24l2-4H13V6h8v7.24L18.62 18zm-2-2h.76L19 12.76V8h-4v4h3.62l-2 4zm-8 2H3.38l2-4H3V6h8v7.24L8.62 18zm-2-2h.76L9 12.76V8H5v4h3.62l-2 4z'/%3E%3C/svg%3E");
}

.dark .code-square-alt::after {
  background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 20 20' fill='%23252523'%3E%3Cpath d='M5,2.5 C3.34,2.5 2,3.84 2,5.5 V6.5 C2,7.33 1.33,8 0.5,8 H0 V12 H0.5 C1.33,12 2,12.67 2,13.5 V14.5 C2,16.16 3.34,17.5 5,17.5 H7 V15.5 H5 C4.45,15.5 4,15.05 4,14.5 V13.5 C4,11.84 2.66,10.5 1,10.5 V9.5 C2.66,9.5 4,8.16 4,6.5 V5.5 C4,4.95 4.45,4.5 5,4.5 H7 V2.5 H5 Z M15,2.5 H13 V4.5 H15 C15.55,4.5 16,4.95 16,5.5 V6.5 C16,8.16 17.34,9.5 19,9.5 V10.5 C17.34,10.5 16,11.84 16,13.5 V14.5 C16,15.05 15.55,15.5 15,15.5 H13 V17.5 H15 C16.66,17.5 18,16.16 18,14.5 V13.5 C18,12.67 18.67,12 19.5,12 H20 V8 H19.5 C18.67,8 18,7.33 18,6.5 V5.5 C18,3.84 16.66,2.5 15,2.5 Z'/%3E%3C/svg%3E");
}

.dark .quote-square-alt::after {
  background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='%23252523'%3E%3Cpath d='M18.62 18h-5.24l2-4H13V6h8v7.24L18.62 18zm-2-2h.76L19 12.76V8h-4v4h3.62l-2 4zm-8 2H3.38l2-4H3V6h8v7.24L8.62 18zm-2-2h.76L9 12.76V8H5v4h3.62l-2 4z'/%3E%3C/svg%3E");
}

/* Block content styling */
.text-node.code-block .markdown-content {
  background: hsl(var(--muted));
  padding: 0.5rem;
  border-radius: var(--radius);
  font-family: 'SF Mono', Monaco, 'Cascadia Code', monospace;
  font-size: 0.875rem;
  line-height: 1.5;
  white-space: pre-wrap;
}

.text-node.quote-block .markdown-content {
  border-left: 3px solid hsl(var(--primary));
  padding-left: 1rem;
  margin: 0;
  color: hsl(var(--muted-foreground));
  font-style: italic;
}

/* Syntax marker visibility */
.markdown-content.focused .syntax-marker {
  opacity: 0.7;
  color: hsl(var(--muted-foreground));
}

.markdown-content:not(.focused) .syntax-marker {
  display: none;
}
```

---

## Issue #2: Add Slash Command Entries

### Files to Modify
1. `packages/desktop-app/src/lib/plugins/corePlugins.ts`
2. `packages/desktop-app/src/lib/components/ui/slash-command-dropdown/slash-command-dropdown.svelte`

### Implementation

#### Step 1: Add Commands (corePlugins.ts)
Find the `commands` array and add after existing text/header commands:

```typescript
{
  id: 'code-block',
  name: 'Code Block',
  shortcut: '```',
  nodeType: 'text',
  content: '```\n\n```',
  cursorPosition: 4, // After opening ```\n
  description: 'Multi-line code with syntax highlighting'
},
{
  id: 'quote-block',
  name: 'Quote Block',
  shortcut: '>',
  nodeType: 'text',
  content: '> ',
  cursorPosition: 2, // After "> "
  description: 'Multi-line quotation block'
}
```

#### Step 2: Add Icon Display (slash-command-dropdown.svelte)
Find existing icon conditions (~line 210) and add:

```svelte
{:else if command.id === 'code-block'}
  <div class="ai-icon">
    <div class="code-square-alt"></div>
  </div>
{:else if command.id === 'quote-block'}
  <div class="ai-icon">
    <div class="quote-square-alt"></div>
  </div>
```

---

## Issue #3: Implement Code Block Detection and Behavior

### Files to Modify
`packages/desktop-app/src/lib/components/viewers/text-node-viewer.svelte`

### Implementation

#### Step 1: Add Detection Functions
```typescript
function isCodeBlock(content: string): boolean {
  return content.startsWith('```') && content.endsWith('```') && content.includes('\n');
}

function getBlockType(content: string): 'code' | 'quote' | 'normal' {
  if (isCodeBlock(content)) return 'code';
  if (isQuoteBlock(content)) return 'quote';
  return 'normal';
}
```

#### Step 2: Add Visual State Management
```typescript
// Reactive class name based on block type
$: blockClass = {
  code: 'code-block',
  quote: 'quote-block',
  normal: ''
}[getBlockType(content)];

$: nodeClassName = `text-node ${blockClass}`.trim();
```

#### Step 3: Add Content Processing
```typescript
function processContentForDisplay(content: string, focused: boolean): string {
  const blockType = getBlockType(content);

  if (blockType === 'code' && !focused) {
    // Hide ``` markers when not focused
    return content.replace(/^```\n/, '').replace(/\n```$/, '');
  }

  return content;
}
```

---

## Issue #4: Implement Quote Block Detection and Behavior

### Files to Modify
`packages/desktop-app/src/lib/components/viewers/text-node-viewer.svelte`

### Implementation

#### Step 1: Add Quote Detection
```typescript
function isQuoteBlock(content: string): boolean {
  return content.split('\n').every(line =>
    line.startsWith('> ') || line.trim() === '' || line === '>'
  );
}
```

#### Step 2: Add Quote Processing
```typescript
function processQuoteContent(content: string, focused: boolean): string {
  if (!isQuoteBlock(content)) return content;

  if (focused) {
    // Show > markers with reduced opacity
    return content.split('\n')
      .map(line => {
        if (line.startsWith('> ')) {
          return `<span class="syntax-marker">${line.substring(0, 2)}</span>${line.substring(2)}`;
        }
        return line;
      })
      .join('\n');
  } else {
    // Hide > markers
    return content.split('\n')
      .map(line => line.startsWith('> ') ? line.substring(2) : line)
      .join('\n');
  }
}
```

#### Step 3: Add Auto-prefixing for New Lines
```typescript
function handleShiftEnterInQuote(event: KeyboardEvent) {
  if (getBlockType(content) === 'quote') {
    insertAtCursor('\n> ');
    event.preventDefault();
  }
}
```

---

## Issue #5: Add Keyboard Shortcuts for Block Activation

### Files to Modify
`packages/desktop-app/src/lib/components/viewers/text-node-viewer.svelte`

### Implementation

#### Step 1: Add Shortcut Detection
```typescript
function detectAndActivateShortcuts(content: string, event: KeyboardEvent) {
  if (content.endsWith('```') && (event.key === ' ' || event.key === 'Enter')) {
    // Transform to code block
    const newContent = '```\n\n```';
    updateContent(newContent);
    setCursorPosition(4); // After opening ```\n
    event.preventDefault();
  } else if (content.endsWith('>') && event.key === ' ') {
    // Transform to quote block
    updateContent('> ');
    setCursorPosition(2); // After "> "
    event.preventDefault();
  }
}
```

#### Step 2: Integrate with Keyboard Handler
```typescript
function handleKeydown(event: KeyboardEvent) {
  // Handle shortcut detection first
  if (event.key === ' ' || event.key === 'Enter') {
    detectAndActivateShortcuts(content, event);
    if (event.defaultPrevented) return;
  }

  // Continue with existing keyboard handling...
}
```

---

## Issue #6: Implement Node Splitting and Type Conversion

### Files to Modify
`packages/desktop-app/src/lib/components/viewers/text-node-viewer.svelte`

### Implementation

#### Step 1: Add Node Splitting Logic
```typescript
function handleCodeBlockEnter(event: KeyboardEvent) {
  const cursorPos = getCursorPosition();
  const isAtEnd = cursorPos >= content.lastIndexOf('```');

  if (isAtEnd) {
    // Create new code block
    dispatch('createNewNode', {
      nodeType: 'text',
      content: '```\n\n```',
      cursorPosition: 4
    });
  } else {
    // Split current code block
    const beforeCursor = content.substring(0, cursorPos);
    const afterCursor = content.substring(cursorPos);

    // Update current node
    updateContent(beforeCursor + '\n```');

    // Create new node with remaining content
    dispatch('createNewNode', {
      nodeType: 'text',
      content: '```\n' + afterCursor,
      cursorPosition: 4
    });
  }
  event.preventDefault();
}

function handleQuoteBlockEnter(event: KeyboardEvent) {
  const cursorPos = getCursorPosition();
  const lines = content.split('\n');
  const currentLineIndex = getCurrentLineIndex(cursorPos);

  // Split at cursor position
  const beforeLines = lines.slice(0, currentLineIndex + 1);
  const afterLines = lines.slice(currentLineIndex + 1);

  // Ensure both parts maintain quote formatting
  const beforeContent = beforeLines.join('\n');
  const afterContent = afterLines.map(line =>
    line.startsWith('> ') ? line : `> ${line}`
  ).join('\n');

  // Update current node
  updateContent(beforeContent);

  // Create new quote block
  dispatch('createNewNode', {
    nodeType: 'text',
    content: afterContent || '> ',
    cursorPosition: 2
  });

  event.preventDefault();
}
```

#### Step 2: Add Type Conversion Rules
```typescript
function convertBlockType(fromType: string, toType: string, content: string): string {
  if (fromType === 'code' || fromType === 'quote') {
    if (toType === 'text') {
      // Preserve multi-line, remove syntax
      if (fromType === 'code') {
        return content.replace(/^```\n/, '').replace(/\n```$/, '');
      } else if (fromType === 'quote') {
        return content.split('\n')
          .map(line => line.startsWith('> ') ? line.substring(2) : line)
          .join('\n');
      }
    } else if (toType === 'header' || toType === 'task') {
      // Flatten to single line, remove syntax
      let flatContent = content;
      if (fromType === 'code') {
        flatContent = content.replace(/^```\n/, '').replace(/\n```$/, '');
      } else if (fromType === 'quote') {
        flatContent = content.split('\n')
          .map(line => line.startsWith('> ') ? line.substring(2) : line)
          .join(' ');
      }
      return flatContent.replace(/\n/g, ' ').trim();
    }
  }
  return content;
}
```

#### Step 3: Integrate with Enter Key Handling
```typescript
function handleKeydown(event: KeyboardEvent) {
  const blockType = getBlockType(content);

  if (event.key === 'Enter') {
    if (event.shiftKey) {
      // Shift+Enter: Add line within block
      if (blockType === 'quote') {
        handleShiftEnterInQuote(event);
        return;
      }
      // Code blocks: normal line break (default behavior)
      return;
    } else {
      // Enter: Split or create new node
      if (blockType === 'code') {
        handleCodeBlockEnter(event);
        return;
      }
      if (blockType === 'quote') {
        handleQuoteBlockEnter(event);
        return;
      }
    }
  }

  // Continue with existing logic...
}
```

---

## Testing Guidelines

### Manual Testing Checklist
For each issue, test the following:

#### Issue #1 (CSS/Icons):
- [ ] Icons display correctly in design patterns page
- [ ] Light/dark theme switching works
- [ ] CSS classes don't conflict with existing styles

#### Issue #2 (Slash Commands):
- [ ] `/code` and `/quote` commands appear in dropdown
- [ ] Icons display correctly in slash command dropdown
- [ ] Commands create proper initial content structure

#### Issue #3 (Code Blocks):
- [ ] Code block detection works correctly
- [ ] Syntax markers show on focus, hide on blur
- [ ] Monospace styling applies correctly

#### Issue #4 (Quote Blocks):
- [ ] Quote block detection works correctly
- [ ] `> ` prefixes show on focus, hide on blur
- [ ] Shift+Enter auto-prefixes new lines

#### Issue #5 (Shortcuts):
- [ ] ``` + space/enter triggers code block
- [ ] `>` + space triggers quote block
- [ ] Cursor positioning is correct after activation

#### Issue #6 (Node Splitting):
- [ ] Enter in middle splits blocks correctly
- [ ] Enter at end creates new block nodes
- [ ] Type conversion preserves/flattens content appropriately

### Edge Cases to Test
- Empty blocks with just syntax markers
- Very long content doesn't break layout
- Copy/paste preserves block formatting
- Undo/redo works properly
- Multiple consecutive blocks
- Block conversion between different types

---

## Reference Documentation

- **Complete specifications**: `docs/architecture/components/contenteditable-implementation.md` (Extended Markdown Blocks section)
- **Design patterns**: `docs/design-system/patterns.html` (Extended Markdown Blocks section)
- **Existing implementation patterns**: Look at how headers are currently implemented in the text-node-viewer component