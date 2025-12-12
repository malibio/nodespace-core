# Node Reference System in Chat

## Overview

This document describes how NodeSpace nodes are referenced, linked, and displayed within AI chat conversations. The system enables bidirectional linking between chat sessions and nodes, creating a connected knowledge graph.

## Reference Formats

### Primary Format: `nodespace://`

The canonical format for node references in chat:

```
nodespace://abc-123-def-456-789
```

**Usage**:
- MCP tool responses
- Agent messages
- Stored in content for persistence

### Display Format: Node Pills

Visual representation of node references in the UI:

```
[📄 Q4 Planning]    ← Header node
[☐ Review budget]   ← Task node (pending)
[☑ Setup meeting]   ← Task node (completed)
[📝 Meeting notes]  ← Text node
```

## Two-Way Reference Flow

### Input: User @ Mentions

```
User types:
"Summarize @Q4 Planning and add tasks to @Project Alpha"
         ↓
Autocomplete resolves to node IDs
         ↓
Sent to agent as:
"Summarize nodespace://abc-123 and add tasks to nodespace://def-456"
         ↓
Mention edges created:
  AIChatNode → mentions → abc-123
  AIChatNode → mentions → def-456
```

### Output: Agent References

```
Agent returns (via MCP tool or response):
"Created nodespace://task-001 in nodespace://def-456"
         ↓
Parsed on render
         ↓
Displayed as:
"Created [☐ Review budget] in [📄 Project Alpha]"
         ↓
Mention edges created:
  AIChatNode → mentions → task-001
  AIChatNode → mentions → def-456
```

## MCP Tool Response Format

### Ensuring `nodespace://` Format

MCP tools should return node references in a consistent format:

```typescript
// MCP create_node tool response
{
  "success": true,
  "message": "Created task nodespace://abc-123",
  "node": {
    "id": "abc-123",
    "reference": "nodespace://abc-123",  // Agent uses this
    "title": "Review Q4 budget",
    "type": "task"
  }
}
```

The agent naturally echoes tool output, propagating the `nodespace://` format.

### Query Results

```typescript
// MCP query_nodes tool response
{
  "nodes": [
    {
      "id": "abc-123",
      "reference": "nodespace://abc-123",
      "title": "Q4 Planning",
      "type": "header",
      "preview": "Planning document for Q4..."
    },
    {
      "id": "def-456",
      "reference": "nodespace://def-456",
      "title": "Review budget",
      "type": "task",
      "status": "pending"
    }
  ],
  "message": "Found 2 nodes matching your query"
}
```

## Parsing and Validation

### Detection Patterns

```typescript
// node-reference-parser.ts

// Primary: nodespace:// links (from MCP tools)
const NODESPACE_LINK_PATTERN = /nodespace:\/\/([\w-]{36})/g;

// Fallback: raw UUIDs not already in nodespace:// format
const RAW_UUID_PATTERN = /(?<!nodespace:\/\/)\b([\da-f]{8}-[\da-f]{4}-[\da-f]{4}-[\da-f]{4}-[\da-f]{12})\b/gi;

export function extractNodeIds(content: string): string[] {
  const ids = new Set<string>();

  // Extract from nodespace:// links
  let match;
  while ((match = NODESPACE_LINK_PATTERN.exec(content)) !== null) {
    ids.add(match[1]);
  }

  // Extract raw UUIDs (fallback)
  while ((match = RAW_UUID_PATTERN.exec(content)) !== null) {
    ids.add(match[1]);
  }

  return Array.from(ids);
}
```

### Validation Against Database

Not every UUID is a valid node. Validate before rendering as a link:

```typescript
export async function validateAndResolveReferences(
  content: string
): Promise<ResolvedContent> {
  // 1. Extract all potential node IDs
  const candidateIds = extractNodeIds(content);

  if (candidateIds.length === 0) {
    return { segments: [{ type: 'text', text: content }], validNodes: [] };
  }

  // 2. Batch validate against database
  const validNodes = await batchFetchNodes(candidateIds);
  const validIdSet = new Set(validNodes.map(n => n.id));

  // 3. Build segments, only linking valid nodes
  const segments = buildSegments(content, validIdSet);

  return { segments, validNodes };
}

async function batchFetchNodes(ids: string[]): Promise<Node[]> {
  // Single database query for efficiency
  return await nodeService.query({
    filters: [{ field: 'id', op: 'in', value: ids }]
  });
}
```

## Node Resolution and Decoration

### Type-Aware Display

Different node types get different visual treatments:

| Node Type | Icon | Status Display | Background |
|-----------|------|----------------|------------|
| `task` | ☐/☑ | Checkbox reflects status | Blue tint |
| `header` | # | None | Purple tint |
| `text` | 📄 | None | Default |
| `date` | 📅 | None | Green tint |
| `code-block` | `</>` | Language badge | Gray |
| `quote-block` | ❝ | None | Yellow tint |

### Resolution Service

```typescript
// node-resolution-service.ts

interface ResolvedNode {
  id: string;
  content: string;
  node_type: string;
  properties: Record<string, unknown>;
  displayTitle: string;
  icon: string;
  statusIndicator?: string;
}

const NODE_TYPE_CONFIG: Record<string, NodeTypeConfig> = {
  'task': {
    icon: '☐',
    getStatus: (node) => node.properties?.status === 'completed' ? '☑' : '☐',
    background: 'var(--task-bg)'
  },
  'header': {
    icon: '#',
    background: 'var(--header-bg)'
  },
  'text': {
    icon: '📄',
    background: 'var(--surface-3)'
  },
  'date': {
    icon: '📅',
    background: 'var(--date-bg)'
  }
};

export function resolveNodeForDisplay(node: Node): ResolvedNode {
  const config = NODE_TYPE_CONFIG[node.node_type] || NODE_TYPE_CONFIG['text'];

  return {
    id: node.id,
    content: node.content,
    node_type: node.node_type,
    properties: node.properties,
    displayTitle: truncate(node.content, 30),
    icon: config.icon,
    statusIndicator: config.getStatus?.(node)
  };
}
```

## Caching Strategy

### Multi-Level Cache

```typescript
// node-cache.ts

class NodeReferenceCache {
  // L1: In-memory cache for current session
  private memoryCache = new Map<string, ResolvedNode>();

  // L2: IndexedDB for persistence across sessions
  private dbCache: IDBDatabase;

  // Pending fetches to deduplicate concurrent requests
  private pendingFetches = new Map<string, Promise<ResolvedNode | null>>();

  async get(nodeId: string): Promise<ResolvedNode | null> {
    // Check memory first
    if (this.memoryCache.has(nodeId)) {
      return this.memoryCache.get(nodeId)!;
    }

    // Check IndexedDB
    const cached = await this.getFromDB(nodeId);
    if (cached && !this.isStale(cached)) {
      this.memoryCache.set(nodeId, cached);
      return cached;
    }

    // Fetch from server
    return this.fetchAndCache(nodeId);
  }

  async getMany(nodeIds: string[]): Promise<Map<string, ResolvedNode>> {
    const results = new Map<string, ResolvedNode>();
    const toFetch: string[] = [];

    // Check cache for each
    for (const id of nodeIds) {
      if (this.memoryCache.has(id)) {
        results.set(id, this.memoryCache.get(id)!);
      } else {
        toFetch.push(id);
      }
    }

    // Batch fetch missing
    if (toFetch.length > 0) {
      const fetched = await this.batchFetch(toFetch);
      for (const node of fetched) {
        results.set(node.id, node);
        this.memoryCache.set(node.id, node);
      }
    }

    return results;
  }

  invalidate(nodeId: string): void {
    this.memoryCache.delete(nodeId);
    this.removeFromDB(nodeId);
  }

  private isStale(cached: CachedNode): boolean {
    const maxAge = 5 * 60 * 1000; // 5 minutes
    return Date.now() - cached.cachedAt > maxAge;
  }
}

export const nodeCache = new NodeReferenceCache();
```

### Cache Invalidation

```typescript
// Listen for node updates to invalidate cache
eventBus.on('node:updated', (nodeId: string) => {
  nodeCache.invalidate(nodeId);
});

eventBus.on('node:deleted', (nodeId: string) => {
  nodeCache.invalidate(nodeId);
});
```

## Mention Edge Relationships

### Creating Mention Edges

```typescript
// mention-service.ts

interface MentionEdge {
  from: string;      // Source node (AIChatNode)
  to: string;        // Referenced node
  context: MentionContext;
  created_at: string;
}

type MentionContext =
  | 'chat_prompt'      // User mentioned in their message
  | 'ai_response'      // AI referenced in response
  | 'tool_creation';   // AI created this node

export async function createMentionEdges(
  chatNodeId: string,
  content: string,
  context: MentionContext
): Promise<void> {
  const nodeIds = extractNodeIds(content);

  // Validate these are real nodes
  const validNodes = await batchFetchNodes(nodeIds);

  // Create edges for valid references
  for (const node of validNodes) {
    await db.create('mentions', {
      from: chatNodeId,
      to: node.id,
      context,
      created_at: new Date().toISOString()
    });
  }
}
```

### Querying Mentions

```typescript
// Find all nodes mentioned in a chat
async function getNodesMentionedInChat(chatNodeId: string): Promise<Node[]> {
  const edges = await db.query(`
    SELECT ->mentions->node.* as mentioned
    FROM $chatNodeId
  `, { chatNodeId });

  return edges.mentioned;
}

// Find all chats that mention a node
async function getChatsMentioningNode(nodeId: string): Promise<AIChatNode[]> {
  const edges = await db.query(`
    SELECT <-mentions<-ai_chat_node.* as chats
    FROM $nodeId
  `, { nodeId });

  return edges.chats;
}

// Find nodes created by a chat
async function getNodesCreatedByChat(chatNodeId: string): Promise<Node[]> {
  const edges = await db.query(`
    SELECT ->mentions->node.*
    FROM $chatNodeId
    WHERE context = 'tool_creation'
  `, { chatNodeId });

  return edges;
}
```

## Click Behavior

### Opening Nodes in Panes

```typescript
// pane-manager.ts

export function openNodeFromChat(nodeId: string): void {
  const userPref = getUserPreference('nodeLinkBehavior');

  switch (userPref) {
    case 'split':
      // Open in side panel (split view)
      paneManager.openInSplit(nodeId, 'right');
      break;

    case 'tab':
      // Open in new tab
      tabManager.openInNewTab(nodeId);
      break;

    case 'replace':
      // Replace current view
      navigationService.navigateTo(nodeId);
      break;

    default:
      // Default: split view
      paneManager.openInSplit(nodeId, 'right');
  }
}
```

## Error Handling

### Deleted/Missing Nodes

```svelte
<!-- NodePill.svelte - handling missing nodes -->
{#if !node}
  <span class="node-pill not-found">
    <span class="icon">⚠️</span>
    <span class="title">Node not found</span>
  </span>
{/if}
```

### Fallback Display

When a node can't be resolved:

| Scenario | Display |
|----------|---------|
| Node deleted | `[⚠️ Node not found]` (grayed out, not clickable) |
| Network error | `[🔄 Loading...]` with retry |
| Invalid UUID | Plain text (not converted to pill) |

## Implementation Checklist

### Phase 1: Basic Parsing
- [ ] `nodespace://` link detection
- [ ] Raw UUID fallback detection
- [ ] Segment builder for content

### Phase 2: Resolution
- [ ] Batch node fetching
- [ ] Type-aware decoration
- [ ] NodePill component

### Phase 3: Caching
- [ ] In-memory cache
- [ ] IndexedDB persistence
- [ ] Cache invalidation on updates

### Phase 4: Mention Edges
- [ ] Edge creation on user mentions
- [ ] Edge creation on AI references
- [ ] Query methods for bidirectional lookup

### Phase 5: @ Autocomplete
- [ ] Trigger detection
- [ ] Node search integration
- [ ] Selection and insertion
- [ ] Convert to nodespace:// on send

## Related Documents

- [AI Integration Overview](./ai-integration-overview.md)
- [AIChatNode Specification](./ai-chat-node-specification.md)
- [Chat UI Implementation Guide](./chat-ui-implementation-guide.md)
- [Node Reference System](../components/node-reference-system.md) - General node reference architecture
