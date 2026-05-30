# NodeSpace Skill

NodeSpace is a local-first knowledge graph that stores notes, tasks, and structured data on your machine. It persists across sessions — what you save today is searchable tomorrow.

## How NodeSpace Thinks (Mental Model)

**Everything is a node.** A node has a type, markdown content, and optional typed properties. Node types are schema-defined: `text`, `task`, and `date` are built-ins; custom types come from user-defined schemas (`nodespace schema list` shows what's registered).

**Built-in node types:**
- `text` — freeform notes, documents, findings, summaries
- `task` — structured to-do items; carry status (`todo`/`in_progress`/`done`) and priority
- `date` — daily anchor nodes (e.g. "2026-05-30"); the temporal spine of the graph. Attach time-sensitive findings under the relevant date node so they're retrievable by day.

**Hierarchy is first-class edges, not nesting.** A node has one parent edge. Children are ordered via fractional ordering — siblings have a stable position without gap-numbering. Moving or reordering a node is an edge operation (change the parent or sibling position), not a recreate-and-delete.

**Content is markdown.** Store prose, code blocks, lists — whatever fits the note. The export commands render it back as clean markdown.

## When to Use NodeSpace (Session Judgment)

Use NodeSpace as a working memory across sessions:

1. **Session start** — run the preflight, then search for prior context before you begin. (`nodespace search "topic"`)
2. **During the session** — save discoveries, decisions, and summaries as you go. Don't wait until the end.
3. **Session end** — anything worth remembering next time should be stored. NodeSpace persists across sessions; your context window does not.

Date nodes make temporal retrieval reliable: if a finding is time-bound, attach it under today's date node so future searches can scope by day.

## Preflight Check

**Before starting any multi-step NodeSpace operation**, run these two commands to confirm the tooling is present and healthy:

```bash
nodespace --version
nodespace diagnostics
```

Run this preflight once per session or task, not before every individual command.

### Failure recovery

| Symptom | Cause | Recovery |
|---------|-------|----------|
| `command not found: nodespace` | CLI not installed or not on `$PATH` | Tell the user: NodeSpace CLI is not installed. They need to install it (e.g. via the NodeSpace DMG or `cargo install nodespace-cli`). Do not proceed. |
| `Could not connect to nodespaced` | Daemon not running | Surface the CLI's own message to the user: start the daemon with `nodespaced`. Do not retry automatically — wait for confirmation. |
| `diagnostics` shows entries in `errors` | Database issues | Report the specific error messages to the user before continuing. |

## Prerequisites

NodeSpace daemon must be running. The `nodespace` CLI communicates with `nodespaced` over a Unix socket. If the daemon is not running, CLI commands will fail with a connection error.

Start the daemon: `nodespaced` (or it starts automatically on login if installed via DMG).

## CLI Reference

All commands accept `--json` for machine-readable output.

### Create a node

```bash
nodespace node create --type text --content "Your content here"
nodespace node create --type task --content "Buy groceries" --parent <parent-id>
nodespace node create --type text --content "Meeting notes" --parent <parent-id>
```

**Options:**
- `--type <type>` — node type: `text`, `task`, `date`, or any schema-defined type
- `--content <text>` — the text content of the node
- `--parent <id>` — optional parent node ID (creates a child node)

**Output:** JSON with `id`, `node_type`, `content`, `parent_id`, `created_at`

### Get a node

```bash
nodespace node get <node-id>
```

**Output:** Full node JSON including all properties

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

### List children

```bash
nodespace node children <parent-id>
```

**Output:** JSON array of child nodes

### Query nodes (structured filter)

```bash
nodespace node query --type task
nodespace node query --content-contains "authentication" --limit 10
nodespace node query --title-contains "Project" --type text
nodespace node query --mentioned-by <node-id>
```

**Options:**
- `--type <type>` — filter by node type
- `--content-contains <text>` — substring match in content
- `--title-contains <text>` — substring match in title
- `--mentioned-by <id>` — nodes mentioned by the given node
- `--limit <n>` — max results
- `--offset <n>` — pagination offset

**Output:** JSON array of matching nodes

### Export node as markdown

```bash
nodespace node export <node-id>
nodespace node export <node-id> --children false   # root node only
nodespace node export <node-id> --max-depth 3
nodespace node export <node-id> --node-ids false   # clean markdown without OCC comments
```

**Options:**
- `--children` — include children recursively (default: true)
- `--max-depth <n>` — maximum recursion depth (default: 20)
- `--node-ids` — embed `<!-- id v<version> -->` OCC comments (default: true)

**Output:** Markdown string (human mode) or `{"markdown":"…","node_count":N}` (JSON mode)

### Batch fetch nodes

```bash
nodespace node batch-get --id <id1> --id <id2> --id <id3>
```

**Output:** `{"count":N,"nodes":[…],"not_found":["id-that-was-missing"]}`

### Batch update nodes (OCC-aware)

```bash
nodespace node batch-update --updates '[{"node_id":"abc","content":"new text","version":3}]'
```

Each object in the array: `node_id` (required), `version` (optional — omit to auto-fetch, note this bypasses OCC), `content`, `node_type`, `properties`.

**Output:** `{"count":N,"updated":["id1",…],"failed":[{"node_id":"…","error":"…"}]}`

### Semantic search

```bash
nodespace search "meeting notes from last week"
nodespace search "rust async" --type text --limit 10
```

**Options:**
- `--type <type>` — filter by node type (repeatable)
- `--limit <n>` — max results (default: 20)

**Output:** JSON array of matching nodes

### Mention relationships

```bash
# Create a mention link
nodespace mention create --from <source-id> --to <target-id>

# Delete a mention link
nodespace mention delete --from <source-id> --to <target-id>

# List nodes this node mentions
nodespace mention outgoing <node-id>

# List nodes that mention this node
nodespace mention incoming <node-id>
```

### Schema inspection

```bash
# List all registered schemas
nodespace schema list

# Get a specific schema definition
nodespace schema get task
nodespace schema get person
```

**Output:** Schema nodes as JSON (same shape as regular nodes; `node_type="schema"`)

## Common Agent Tasks

### Save a note for later

```bash
nodespace node create --type text --content "Key insight: the auth token expires after 1 hour"
```

### Search for previously stored context

```bash
nodespace search "authentication token refresh"
```

### Create a task

```bash
nodespace node create --type task --content "Implement rate limiting on the API gateway"
```

### Organize under a parent

```bash
# Create a parent project node
nodespace node create --type text --content "Project: API Redesign"
# → returns {"id": "abc123", ...}

# Add sub-notes under it
nodespace node create --type text --content "Decision: use REST not GraphQL" --parent abc123
```

### Attach a finding to today's date node

```bash
# Find today's date node (or create it if absent)
nodespace node query --type date --title-contains "2026-05-30"
# → returns {"id": "date-node-id", ...}  (or empty array)

nodespace node create --type date --content "2026-05-30"
# → use the returned id as parent for today's findings

# Attach a finding under it
nodespace node create --type text --content "Discovered: rate limiter uses fixed windows" --parent <date-node-id>
```

### Export a document for AI context

```bash
# Export with OCC tokens so AI can update individual nodes
nodespace node export <doc-id> --json | jq '.markdown'

# Clean export for reading
nodespace node export <doc-id> --node-ids false
```

### Build a knowledge graph session

```bash
# At session start: preflight check, then search for relevant context
nodespace --version
nodespace diagnostics
nodespace search "previous work on this codebase"

# During session: save discoveries
nodespace node create --type text --content "Session summary: refactored auth middleware, tests passing"
```

## Output Format

All `--json` commands output to stdout. Errors are written to stderr with a non-zero exit code.

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "node_type": "text",
  "content": "Your content",
  "parent_id": null,
  "properties": {},
  "version": 1,
  "lifecycle_status": "active",
  "created_at": "2026-01-01T00:00:00Z",
  "modified_at": "2026-01-01T00:00:00Z"
}
```
