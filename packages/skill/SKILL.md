# NodeSpace Skill

NodeSpace is a local-first knowledge graph that stores notes, tasks, and structured data on your machine. Use it to persist information across sessions, build personal knowledge bases, and retrieve context from previous work.

## When to Use NodeSpace

- **Store notes or findings**: Save research, decisions, or summaries you'll want later
- **Search for context**: Look up information stored in previous sessions
- **Create structured data**: Organize tasks, project notes, or any typed content
- **Build a knowledge graph**: Link related information with parent-child relationships

## Prerequisites

NodeSpace daemon must be running. The `nodespace` CLI communicates with `nodespaced` over a Unix socket. If the daemon is not running, CLI commands will fail with a connection error.

Start the daemon: `nodespace daemon start` (or it starts automatically on login if installed via DMG).

## CLI Reference

All commands use the `nodespace` CLI.

### Create a node

```bash
nodespace node create --type note --content "Your content here"
nodespace node create --type task --content "Buy groceries" --parent <parent-id>
nodespace node create --type note --content "Meeting notes" --parent <parent-id>
```

**Options:**
- `--type <type>` — node type: `note`, `task`, `date`, or any schema-defined type
- `--content <text>` — the text content of the node
- `--parent <id>` — optional parent node ID (creates a child node)

**Output:** JSON with `id`, `nodeType`, `content`, `parentId`, `createdAt`

### Get a node

```bash
nodespace node get <node-id>
```

**Output:** Full node JSON including all properties

### Search nodes

```bash
nodespace node search --query "meeting notes from last week"
nodespace node search --query "project ideas" --limit 10
nodespace node search --query "rust async" --type note
```

**Options:**
- `--query <text>` — semantic search query
- `--limit <n>` — max results (default: 5)
- `--type <type>` — filter by node type

**Output:** JSON array of matching nodes with `id`, `nodeType`, `content`, `score`

### List nodes

```bash
nodespace node list
nodespace node list --type task
nodespace node list --parent <parent-id>
nodespace node list --limit 20
```

**Options:**
- `--type <type>` — filter by node type
- `--parent <id>` — list children of a node
- `--limit <n>` — max results (default: 50)

**Output:** JSON array of nodes

### Update a node

```bash
nodespace node update <node-id> --content "Updated content"
```

**Output:** Updated node JSON

### Delete a node

```bash
nodespace node delete <node-id>
```

**Output:** Confirmation JSON

## Common Agent Tasks

### Save a note for later

```bash
nodespace node create --type note --content "Key insight: the auth token expires after 1 hour and must be refreshed via /oauth/token"
```

### Search for previously stored context

```bash
nodespace node search --query "authentication token refresh"
```

### Create a task

```bash
nodespace node create --type task --content "Implement rate limiting on the API gateway"
```

### Organize under a parent

```bash
# Create a parent project node
nodespace node create --type note --content "Project: API Redesign"
# → returns {"id": "note:abc123", ...}

# Add sub-notes under it
nodespace node create --type note --content "Decision: use REST not GraphQL" --parent note:abc123
```

### Build a knowledge graph session

```bash
# At session start: search for relevant context
nodespace node search --query "previous work on this codebase"

# During session: save discoveries
nodespace node create --type note --content "Found that NodeService uses SurrealDB with RocksDB backend"

# At session end: save summary
nodespace node create --type note --content "Session summary: refactored auth middleware, tests passing"
```

## Output Format

All commands output JSON to stdout. Errors are written to stderr with a non-zero exit code.

```json
{
  "id": "note:ulid-here",
  "nodeType": "note",
  "content": "Your content",
  "parentId": null,
  "createdAt": "2026-01-01T00:00:00Z",
  "modifiedAt": "2026-01-01T00:00:00Z"
}
```
