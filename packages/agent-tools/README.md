# @nodespaceai/agent-tools

gRPC-backed knowledge graph tools for PTY-spawned agents (Claude Code, Codex, etc.).

Exposes the NodeSpace knowledge graph to AI agents running inside a NodeSpace session — search nodes semantically, read, create, and update nodes via the local `nodespaced` daemon.

## Installation

```bash
npm install @nodespaceai/agent-tools
# or
bun add @nodespaceai/agent-tools
```

## Requirements

A running NodeSpace desktop app (or `nodespaced` daemon) with the gRPC server listening on its Unix domain socket. The tools connect automatically when the `NODESPACE_SOCKET` environment variable is set (injected by NodeSpace when spawning PTY agents).

## Usage

```typescript
import {
  searchSemantic,
  getNode,
  createNode,
  updateNode,
  getChildren,
} from '@nodespaceai/agent-tools';

// Semantic search across the knowledge graph
const results = await searchSemantic('meeting notes from last week', 5);

// Read a node by ID
const node = await getNode('node-uuid-here');

// Create a new node
const created = await createNode('text', 'My note content', parentId);

// Update an existing node
const updated = await updateNode('node-uuid-here', 'Updated content');

// List child nodes
const children = await getChildren('parent-node-uuid');
```

## API

### `searchSemantic(query, limit?)`

Searches the knowledge graph using semantic (embedding) similarity.

- `query` — natural language search query
- `limit` — max results to return (default: 10)
- Returns: `SearchResult[]` with `id`, `nodeType`, `content`, `score`

### `getNode(id)`

Fetches a single node by its UUID.

- Returns: `NodeResult` with `id`, `nodeType`, `content`, `parentId`

### `createNode(nodeType, content, parentId?)`

Creates a new node in the knowledge graph.

- `nodeType` — e.g. `"text"`, `"task"`, `"date"`
- Returns: `NodeResult` for the created node

### `updateNode(id, content)`

Updates the content of an existing node.

- Returns: `NodeResult` for the updated node

### `getChildren(parentId)`

Lists all direct children of a node.

- Returns: `NodeResult[]`

## Error Handling

All functions throw `ToolError` on failure:

```typescript
import { ToolError } from '@nodespaceai/agent-tools';

try {
  const node = await getNode('bad-id');
} catch (err) {
  if (err instanceof ToolError) {
    console.error(err.code, err.message);
  }
}
```

## License

MIT
