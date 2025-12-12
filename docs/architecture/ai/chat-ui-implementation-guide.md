# Chat UI Implementation Guide

## Overview

This guide covers the frontend implementation of NodeSpace's AI chat interface. The Chat UI is responsible for rendering conversations, handling user input with @ mentions, and displaying AI responses with interactive node references.

## Architecture

### Component Hierarchy

```
ChatPanel (main container)
├── ChatHeader
│   ├── ChatTitle
│   ├── ProviderBadge
│   └── ChatActions (archive, settings)
│
├── ChatHistory (scrollable message list)
│   ├── UserMessage
│   │   └── MessageContent (with NodePill components)
│   │
│   ├── AssistantMessage
│   │   ├── ThinkingBlock (collapsible, optional)
│   │   ├── MessageContent (with NodePill components)
│   │   └── ToolCallBlock (collapsible)
│   │       ├── ToolCallHeader
│   │       ├── ToolCallInput (JSON params)
│   │       └── ToolCallOutput (result)
│   │
│   └── LoadingIndicator
│
└── ChatInput
    ├── MentionAutocomplete
    └── SendButton
```

### State Management

```typescript
// chat-store.svelte.ts
interface ChatState {
  // Current session
  chatNodeId: string | null;
  sessionId: string | null;
  provider: AIProvider | null;
  status: ChatSessionStatus;

  // Conversation
  entries: ConversationEntry[];
  isLoading: boolean;
  isStreaming: boolean;

  // UI preferences
  showThinking: boolean;
  showToolCalls: boolean;
  collapseToolCalls: boolean;
}

type ConversationEntry =
  | UserMessageEntry
  | AssistantMessageEntry
  | ThinkingEntry
  | ToolCallEntry;

interface UserMessageEntry {
  type: "user_message";
  id: string;
  content: string;
  timestamp: string;
  mentions: NodeReference[];
}

interface AssistantMessageEntry {
  type: "assistant_message";
  id: string;
  content: string;
  timestamp: string;
  nodeReferences: NodeReference[];  // Parsed nodespace:// links
}

interface ThinkingEntry {
  type: "thinking";
  id: string;
  content: string;
}

interface ToolCallEntry {
  type: "tool_call";
  id: string;
  toolName: string;
  input: Record<string, unknown>;
  output: unknown;
  status: "pending" | "success" | "error";
}

interface NodeReference {
  nodeId: string;
  startIndex: number;
  endIndex: number;
}
```

## ChatPanel Component

### Main Container

```svelte
<!-- ChatPanel.svelte -->
<script lang="ts">
  import { chatStore } from '$lib/stores/chat-store.svelte';
  import ChatHeader from './ChatHeader.svelte';
  import ChatHistory from './ChatHistory.svelte';
  import ChatInput from './ChatInput.svelte';
  import ProviderSelector from './ProviderSelector.svelte';

  export let chatNodeId: string;

  // Load chat on mount
  $effect(() => {
    if (chatNodeId) {
      chatStore.loadChat(chatNodeId);
    }
  });
</script>

<div class="chat-panel">
  {#if !chatStore.provider}
    <ProviderSelector onSelect={(p) => chatStore.setProvider(p)} />

  {:else if chatStore.status === 'expired'}
    <ExpiredSessionView {chatNodeId} />

  {:else}
    <ChatHeader
      title={chatStore.title}
      provider={chatStore.provider}
      onArchive={() => chatStore.archiveChat()}
    />

    <ChatHistory
      entries={chatStore.entries}
      showThinking={chatStore.showThinking}
      showToolCalls={chatStore.showToolCalls}
      isStreaming={chatStore.isStreaming}
    />

    <ChatInput
      disabled={chatStore.isStreaming}
      onSend={(msg, mentions) => chatStore.sendMessage(msg, mentions)}
    />
  {/if}
</div>

<style>
  .chat-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--surface-1);
  }
</style>
```

## ChatHistory Component

### Message Rendering

```svelte
<!-- ChatHistory.svelte -->
<script lang="ts">
  import UserMessage from './UserMessage.svelte';
  import AssistantMessage from './AssistantMessage.svelte';
  import ThinkingBlock from './ThinkingBlock.svelte';
  import ToolCallBlock from './ToolCallBlock.svelte';
  import type { ConversationEntry } from '$lib/types/chat';

  export let entries: ConversationEntry[];
  export let showThinking: boolean;
  export let showToolCalls: boolean;
  export let isStreaming: boolean;

  let scrollContainer: HTMLDivElement;

  // Auto-scroll to bottom on new messages
  $effect(() => {
    if (entries.length > 0) {
      scrollContainer?.scrollTo({
        top: scrollContainer.scrollHeight,
        behavior: 'smooth'
      });
    }
  });
</script>

<div class="chat-history" bind:this={scrollContainer}>
  {#each entries as entry (entry.id)}
    {#if entry.type === 'user_message'}
      <UserMessage {entry} />

    {:else if entry.type === 'assistant_message'}
      <AssistantMessage {entry} />

    {:else if entry.type === 'thinking' && showThinking}
      <ThinkingBlock {entry} />

    {:else if entry.type === 'tool_call' && showToolCalls}
      <ToolCallBlock {entry} />
    {/if}
  {/each}

  {#if isStreaming}
    <div class="streaming-indicator">
      <span class="dot"></span>
      <span class="dot"></span>
      <span class="dot"></span>
    </div>
  {/if}
</div>

<style>
  .chat-history {
    flex: 1;
    overflow-y: auto;
    padding: var(--space-4);
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .streaming-indicator {
    display: flex;
    gap: 4px;
    padding: var(--space-2);
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--text-muted);
    animation: bounce 1.4s ease-in-out infinite both;
  }

  .dot:nth-child(1) { animation-delay: -0.32s; }
  .dot:nth-child(2) { animation-delay: -0.16s; }

  @keyframes bounce {
    0%, 80%, 100% { transform: scale(0); }
    40% { transform: scale(1); }
  }
</style>
```

## AssistantMessage with Node References

### Parsing and Rendering

```svelte
<!-- AssistantMessage.svelte -->
<script lang="ts">
  import { parseNodeReferences } from '$lib/utils/node-reference-parser';
  import NodePill from './NodePill.svelte';
  import type { AssistantMessageEntry } from '$lib/types/chat';

  export let entry: AssistantMessageEntry;

  // Parse content into segments with node references
  $: segments = parseNodeReferences(entry.content);
</script>

<div class="assistant-message">
  <div class="message-header">
    <span class="role">Assistant</span>
    <span class="timestamp">{formatTime(entry.timestamp)}</span>
  </div>

  <div class="message-content">
    {#each segments as segment}
      {#if segment.type === 'text'}
        <span>{segment.text}</span>
      {:else if segment.type === 'node-link'}
        <NodePill nodeId={segment.nodeId} />
      {/if}
    {/each}
  </div>
</div>

<style>
  .assistant-message {
    background: var(--surface-2);
    border-radius: var(--radius-lg);
    padding: var(--space-3);
  }

  .message-header {
    display: flex;
    justify-content: space-between;
    margin-bottom: var(--space-2);
    font-size: var(--text-sm);
  }

  .role {
    font-weight: 600;
  }

  .timestamp {
    color: var(--text-muted);
  }

  .message-content {
    line-height: 1.6;
  }
</style>
```

### Node Reference Parser

```typescript
// node-reference-parser.ts
interface TextSegment {
  type: 'text';
  text: string;
}

interface NodeLinkSegment {
  type: 'node-link';
  nodeId: string;
  raw: string;  // Original matched text
}

type Segment = TextSegment | NodeLinkSegment;

export function parseNodeReferences(content: string): Segment[] {
  const segments: Segment[] = [];

  // Pattern for nodespace:// links
  const nodespacePattern = /nodespace:\/\/([\w-]{36})/g;

  // Pattern for raw UUIDs (fallback)
  const uuidPattern = /(?<!nodespace:\/\/)\b([\da-f]{8}-[\da-f]{4}-[\da-f]{4}-[\da-f]{4}-[\da-f]{12})\b/gi;

  let lastIndex = 0;
  let match;

  // First pass: nodespace:// links
  while ((match = nodespacePattern.exec(content)) !== null) {
    // Text before the link
    if (match.index > lastIndex) {
      segments.push({
        type: 'text',
        text: content.slice(lastIndex, match.index)
      });
    }

    // The node link
    segments.push({
      type: 'node-link',
      nodeId: match[1],
      raw: match[0]
    });

    lastIndex = nodespacePattern.lastIndex;
  }

  // Remaining text
  if (lastIndex < content.length) {
    segments.push({
      type: 'text',
      text: content.slice(lastIndex)
    });
  }

  return segments;
}
```

## NodePill Component

### Type-Aware Decoration

```svelte
<!-- NodePill.svelte -->
<script lang="ts">
  import { getNode } from '$lib/services/node-service';
  import { openInPane } from '$lib/services/pane-manager';
  import TaskCheckbox from '$lib/icons/TaskCheckbox.svelte';
  import HeaderIcon from '$lib/icons/HeaderIcon.svelte';
  import TextIcon from '$lib/icons/TextIcon.svelte';
  import DateIcon from '$lib/icons/DateIcon.svelte';
  import type { Node } from '$lib/types/node';

  export let nodeId: string;

  let node: Node | null = $state(null);
  let loading = $state(true);
  let error = $state(false);

  // Fetch node data
  $effect(() => {
    loading = true;
    error = false;

    getNode(nodeId)
      .then(n => {
        node = n;
        loading = false;
      })
      .catch(() => {
        error = true;
        loading = false;
      });
  });

  function handleClick() {
    if (node) {
      openInPane(nodeId);
    }
  }

  const icons: Record<string, typeof TextIcon> = {
    'task': TaskCheckbox,
    'header': HeaderIcon,
    'text': TextIcon,
    'date': DateIcon,
  };
</script>

<button
  class="node-pill"
  class:loading
  class:error
  class:task={node?.node_type === 'task'}
  class:header={node?.node_type === 'header'}
  onclick={handleClick}
  disabled={loading || error}
>
  {#if loading}
    <span class="loading-spinner"></span>
    <span class="title">Loading...</span>

  {:else if error || !node}
    <span class="error-icon">⚠️</span>
    <span class="title not-found">Node not found</span>

  {:else}
    <!-- Type-specific icon -->
    {#if node.node_type === 'task'}
      <span class="checkbox" class:checked={node.properties?.status === 'completed'}>
        {node.properties?.status === 'completed' ? '☑' : '☐'}
      </span>
    {:else}
      <svelte:component this={icons[node.node_type] || TextIcon} />
    {/if}

    <span class="title">{truncate(node.content, 30)}</span>
  {/if}
</button>

<style>
  .node-pill {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px;
    border-radius: 4px;
    background: var(--surface-3);
    border: 1px solid var(--border);
    cursor: pointer;
    font-size: 0.9em;
    max-width: 200px;
    vertical-align: middle;
  }

  .node-pill:hover:not(:disabled) {
    background: var(--surface-4);
    border-color: var(--border-hover);
  }

  .node-pill.task {
    background: var(--task-bg, #f0f9ff);
  }

  .node-pill.header {
    background: var(--header-bg, #f5f0ff);
  }

  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .not-found {
    color: var(--text-muted);
    font-style: italic;
  }

  .checkbox {
    font-family: monospace;
  }

  .checkbox.checked {
    color: var(--success);
  }

  .loading-spinner {
    width: 12px;
    height: 12px;
    border: 2px solid var(--border);
    border-top-color: var(--primary);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
```

## ChatInput with @ Mentions

### Autocomplete Integration

```svelte
<!-- ChatInput.svelte -->
<script lang="ts">
  import { searchNodes } from '$lib/services/node-service';
  import MentionAutocomplete from './MentionAutocomplete.svelte';
  import type { Node } from '$lib/types/node';

  export let disabled: boolean = false;
  export let onSend: (message: string, mentions: string[]) => void;

  let content = $state('');
  let inputElement: HTMLTextAreaElement;

  // Autocomplete state
  let showAutocomplete = $state(false);
  let autocompleteQuery = $state('');
  let autocompletePosition = $state({ top: 0, left: 0 });
  let selectedMentions: Map<string, Node> = $state(new Map());

  function handleInput(e: Event) {
    const target = e.target as HTMLTextAreaElement;
    content = target.value;

    // Detect @ trigger
    const cursorPos = target.selectionStart;
    const textBeforeCursor = content.slice(0, cursorPos);
    const match = textBeforeCursor.match(/@([\w\s]*)$/);

    if (match) {
      showAutocomplete = true;
      autocompleteQuery = match[1];
      autocompletePosition = getCaretCoordinates(target, cursorPos - match[0].length);
    } else {
      showAutocomplete = false;
    }
  }

  function handleMentionSelect(node: Node) {
    const cursorPos = inputElement.selectionStart;
    const textBeforeCursor = content.slice(0, cursorPos);
    const match = textBeforeCursor.match(/@([\w\s]*)$/);

    if (match) {
      // Replace @query with @NodeTitle
      const beforeMention = content.slice(0, cursorPos - match[0].length);
      const afterMention = content.slice(cursorPos);
      const mentionText = `@${node.content} `;

      content = beforeMention + mentionText + afterMention;
      selectedMentions.set(node.id, node);
    }

    showAutocomplete = false;
    inputElement.focus();
  }

  function handleSend() {
    if (!content.trim() || disabled) return;

    // Convert @mentions to nodespace:// format
    let processedContent = content;
    const mentionIds: string[] = [];

    for (const [nodeId, node] of selectedMentions) {
      const mentionPattern = new RegExp(`@${escapeRegex(node.content)}`, 'g');
      processedContent = processedContent.replace(mentionPattern, `nodespace://${nodeId}`);
      mentionIds.push(nodeId);
    }

    onSend(processedContent, mentionIds);

    // Reset
    content = '';
    selectedMentions = new Map();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  }
</script>

<div class="chat-input-container">
  {#if showAutocomplete}
    <MentionAutocomplete
      query={autocompleteQuery}
      position={autocompletePosition}
      onSelect={handleMentionSelect}
      onClose={() => showAutocomplete = false}
    />
  {/if}

  <div class="input-wrapper">
    <textarea
      bind:this={inputElement}
      bind:value={content}
      oninput={handleInput}
      onkeydown={handleKeydown}
      placeholder="Type a message... Use @ to mention nodes"
      rows="1"
      {disabled}
    ></textarea>

    <button
      class="send-button"
      onclick={handleSend}
      disabled={disabled || !content.trim()}
    >
      Send
    </button>
  </div>
</div>

<style>
  .chat-input-container {
    position: relative;
    padding: var(--space-3);
    border-top: 1px solid var(--border);
    background: var(--surface-1);
  }

  .input-wrapper {
    display: flex;
    gap: var(--space-2);
    align-items: flex-end;
  }

  textarea {
    flex: 1;
    resize: none;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: var(--space-2) var(--space-3);
    font-family: inherit;
    font-size: var(--text-base);
    min-height: 40px;
    max-height: 120px;
  }

  textarea:focus {
    outline: none;
    border-color: var(--primary);
  }

  .send-button {
    padding: var(--space-2) var(--space-4);
    background: var(--primary);
    color: white;
    border: none;
    border-radius: var(--radius-md);
    cursor: pointer;
  }

  .send-button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
```

### MentionAutocomplete Component

```svelte
<!-- MentionAutocomplete.svelte -->
<script lang="ts">
  import { searchNodes } from '$lib/services/node-service';
  import type { Node } from '$lib/types/node';

  export let query: string;
  export let position: { top: number; left: number };
  export let onSelect: (node: Node) => void;
  export let onClose: () => void;

  let results: Node[] = $state([]);
  let selectedIndex = $state(0);
  let loading = $state(false);

  // Search as user types
  $effect(() => {
    if (query.length > 0) {
      loading = true;
      searchNodes(query, { limit: 8 })
        .then(nodes => {
          results = nodes;
          selectedIndex = 0;
          loading = false;
        });
    } else {
      results = [];
    }
  });

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      selectedIndex = Math.min(selectedIndex + 1, results.length - 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      selectedIndex = Math.max(selectedIndex - 1, 0);
    } else if (e.key === 'Enter' && results.length > 0) {
      e.preventDefault();
      onSelect(results[selectedIndex]);
    } else if (e.key === 'Escape') {
      onClose();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div
  class="autocomplete-dropdown"
  style="top: {position.top}px; left: {position.left}px"
>
  {#if loading}
    <div class="loading">Searching...</div>

  {:else if results.length === 0}
    <div class="no-results">No nodes found</div>

  {:else}
    <ul class="results">
      {#each results as node, i (node.id)}
        <li
          class="result-item"
          class:selected={i === selectedIndex}
          onclick={() => onSelect(node)}
          onmouseenter={() => selectedIndex = i}
        >
          <span class="node-type">{node.node_type}</span>
          <span class="node-title">{node.content}</span>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .autocomplete-dropdown {
    position: absolute;
    bottom: 100%;
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-lg);
    max-height: 300px;
    overflow-y: auto;
    min-width: 250px;
    z-index: 100;
  }

  .results {
    list-style: none;
    margin: 0;
    padding: var(--space-1);
  }

  .result-item {
    display: flex;
    gap: var(--space-2);
    padding: var(--space-2);
    border-radius: var(--radius-sm);
    cursor: pointer;
  }

  .result-item.selected {
    background: var(--surface-3);
  }

  .node-type {
    font-size: var(--text-xs);
    color: var(--text-muted);
    text-transform: uppercase;
    min-width: 50px;
  }

  .node-title {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .loading, .no-results {
    padding: var(--space-3);
    color: var(--text-muted);
    text-align: center;
  }
</style>
```

## ToolCallBlock Component

### Collapsible Tool Visualization

```svelte
<!-- ToolCallBlock.svelte -->
<script lang="ts">
  import type { ToolCallEntry } from '$lib/types/chat';

  export let entry: ToolCallEntry;

  let expanded = $state(false);
</script>

<div class="tool-call-block" class:expanded>
  <button
    class="tool-call-header"
    onclick={() => expanded = !expanded}
  >
    <span class="status-icon">
      {#if entry.status === 'pending'}
        ⏳
      {:else if entry.status === 'success'}
        ✓
      {:else}
        ✗
      {/if}
    </span>

    <span class="tool-name">{entry.toolName}</span>

    <span class="expand-icon">{expanded ? '▼' : '▶'}</span>
  </button>

  {#if expanded}
    <div class="tool-call-details">
      <div class="section">
        <div class="section-label">Input</div>
        <pre class="code-block">{JSON.stringify(entry.input, null, 2)}</pre>
      </div>

      {#if entry.output}
        <div class="section">
          <div class="section-label">Output</div>
          <pre class="code-block">{JSON.stringify(entry.output, null, 2)}</pre>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .tool-call-block {
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--surface-2);
    font-size: var(--text-sm);
  }

  .tool-call-header {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    padding: var(--space-2);
    background: transparent;
    border: none;
    cursor: pointer;
    text-align: left;
  }

  .tool-call-header:hover {
    background: var(--surface-3);
  }

  .status-icon {
    width: 20px;
  }

  .tool-name {
    flex: 1;
    font-family: monospace;
    font-weight: 500;
  }

  .tool-call-details {
    padding: var(--space-2);
    border-top: 1px solid var(--border);
  }

  .section {
    margin-bottom: var(--space-2);
  }

  .section-label {
    font-size: var(--text-xs);
    color: var(--text-muted);
    text-transform: uppercase;
    margin-bottom: var(--space-1);
  }

  .code-block {
    background: var(--surface-1);
    padding: var(--space-2);
    border-radius: var(--radius-sm);
    overflow-x: auto;
    font-size: var(--text-xs);
    margin: 0;
  }
</style>
```

## Implementation Checklist

### Phase 1: Basic Chat UI
- [ ] ChatPanel container component
- [ ] ChatHistory with message rendering
- [ ] UserMessage and AssistantMessage components
- [ ] Basic styling and layout

### Phase 2: Streaming and State
- [ ] Chat store with state management
- [ ] Message streaming handling
- [ ] Loading indicators
- [ ] Auto-scroll behavior

### Phase 3: Node References
- [ ] Node reference parser
- [ ] NodePill component with type decoration
- [ ] Batch node fetching and caching
- [ ] Click to open in pane

### Phase 4: @ Mentions
- [ ] MentionAutocomplete component
- [ ] Node search integration
- [ ] Mention selection and insertion
- [ ] Convert mentions to nodespace:// on send

### Phase 5: Tool Calls
- [ ] ToolCallBlock component
- [ ] Collapsible details
- [ ] Status indicators
- [ ] ThinkingBlock (optional display)

## Related Documents

- [AI Integration Overview](./ai-integration-overview.md)
- [AIChatNode Specification](./ai-chat-node-specification.md)
- [Node Reference System in Chat](./node-reference-system-in-chat.md)
