# Database Debugging Guide

## Quick Start: Accessing the Development Database

### Connecting with SurrealDB CLI

The development database runs on `ws://localhost:8000` (or `http://localhost:8000`).

**Critical Details (often not obvious):**
- **Namespace**: `nodespace`
- **Database**: `nodespace`
- **Credentials**: `root:root`

```bash
# Connect to the database
surreal sql --endpoint ws://localhost:8000 --user root --pass root --ns nodespace --db nodespace

# Then run queries
SELECT * FROM node;
SELECT * FROM has_child;
```

### Connecting with Surrealist GUI

Surrealist can connect directly: `ws://localhost:8000`

---

## Database Schema: Graph Relations Architecture

Unlike traditional foreign keys, NodeSpace uses **SurrealDB Graph Relations** for the parent-child hierarchy.

### The Three Tables

#### 1. `node` Table
Stores all nodes (text, task, date, etc.)

```surql
SELECT id, content, type FROM node;
```

**Fields** (relevant to debugging):
- `id`: Record ID like `node:⟨uuid⟩` or `node:2025-11-17` for dates
- `content`: The node's text/title
- `type`: Node type (`text`, `task`, `date`, `schema`, etc.)
- `created_at`, `modified_at`: Timestamps
- `version`: Optimistic concurrency control version
- `properties`: JSON object for schema-driven properties
- `embedding_stale`: Whether vector embedding needs refresh

#### 2. `has_child` Table (Graph Relation)
**Defines parent-child relationships** via edges

```surql
SELECT * FROM has_child;
```

**Structure:**
- `in`: Parent node ID (e.g., `node:parent-uuid`)
- `out`: Child node ID (e.g., `node:child-uuid`)
- `id`: Relation ID

**Example result:**
```
[
  { in: node:⟨parent-id⟩, out: node:⟨child-id⟩, id: has_child:relation-id }
]
```

**What it means:**
- `in->has_child->out` means the `in` node **has** the `out` node as a child
- No `parent_id` field on nodes! The relationship is stored only in `has_child`

#### 3. `mentions` Table
Stores mention relationships (less commonly debugged)

---

## Common Debugging Queries

### 1. Find Nodes by Type
```sql
SELECT id, content, type FROM node WHERE type = 'text';
```

### 2. Get All Parent-Child Relationships
```sql
SELECT in AS parent, out AS child FROM has_child;
```

### 3. Get Children of a Specific Node
```sql
-- First, find the parent node
SELECT * FROM node WHERE content = 'Parent';

-- Then get its children (use the ID from above)
SELECT VALUE out FROM has_child WHERE in = node:⟨parent-uuid⟩;

-- Or get the full child nodes
SELECT * FROM node WHERE id IN (
  SELECT VALUE out FROM has_child WHERE in = node:⟨parent-uuid⟩
);
```

### 4. Get Parent of a Specific Node
```sql
SELECT VALUE in FROM has_child WHERE out = node:⟨child-uuid⟩ LIMIT 1;
```

### 5. Get All Root Nodes (No Parent)
```sql
SELECT * FROM node WHERE count(<-has_child) = 0;
```

The `<-has_child` syntax means "incoming has_child edges" - nodes with no incoming edges are roots.

### 6. Check Hierarchy Depth
```sql
-- Find all descendants of a node (recursive)
SELECT id, content FROM node WHERE id IN (
  SELECT VALUE out FROM has_child WHERE in = node:⟨parent-uuid⟩
    FETCH out
)
LIMIT 100;
```

---

## Why Graph Relations Instead of Foreign Keys?

The original design intended to use `parent_id` fields (stored in the `node` record):
```sql
-- Old approach (NOT used in current code)
DEFINE FIELD parent_id ON node TYPE option<record(node)>;
```

**But the current implementation uses `has_child` relations instead because:**
1. **Cleaner queries** - Relations make the hierarchy explicit
2. **Better performance** - Graph edges are optimized in SurrealDB
3. **Bidirectional traversal** - Easy to query "who is my parent?" or "who are my children?"
4. **Multiple relationship types** - Can add `has_sibling`, `references`, etc. without schema changes

---

## Development Workflow: Before & After Database Changes

### Before You Modify Node Structure

1. **Capture current state:**
   ```bash
   echo "SELECT * FROM has_child;" | surreal sql \
     --endpoint ws://localhost:8000 \
     --user root --pass root \
     --ns nodespace --db nodespace
   ```

2. **Take a full snapshot:**
   ```bash
   echo "SELECT * FROM node;" | surreal sql \
     --endpoint ws://localhost:8000 \
     --user root --pass root \
     --ns nodespace --db nodespace > node-backup.json
   ```

### After Modifying Node Structure

1. **Check if relationships changed:**
   ```bash
   echo "SELECT * FROM has_child;" | surreal sql \
     --endpoint ws://localhost:8000 \
     --user root --pass root \
     --ns nodespace --db nodespace
   ```

2. **Verify the change:**
   - Did the `in`/`out` values change?
   - Was a new edge created?
   - Was an old edge deleted?

### Troubleshooting: Nodes Not Persisting

If you create nodes in the UI but they don't appear in the database:

1. **Check the namespace/database:**
   ```bash
   echo "INFO FOR DB;" | surreal sql --endpoint ws://localhost:8000 --user root --pass root --ns nodespace --db nodespace
   ```

2. **Verify proxy is connected:**
   - Check dev-proxy logs for connection errors
   - Verify `http://localhost:3001/health` returns 200

3. **Check if nodes exist but relationships don't:**
   ```bash
   echo "SELECT COUNT() FROM node; SELECT COUNT() FROM has_child;" | surreal sql ...
   ```

---

## API Endpoints for Reference

The dev-proxy runs on `http://localhost:3001` and forwards to the database:

```bash
# Get all nodes (paginated)
curl http://localhost:3001/api/nodes

# Get specific node by ID
curl http://localhost:3001/api/nodes/2025-11-17

# Create node
curl -X POST http://localhost:3001/api/nodes \
  -H "Content-Type: application/json" \
  -d '{
    "id": "node-id",
    "nodeType": "text",
    "content": "My node",
    "parentId": "parent-node-id"
  }'

# Update node
curl -X PATCH http://localhost:3001/api/nodes/node-id \
  -H "Content-Type: application/json" \
  -d '{"content": "Updated"}'
```

---

## Common Issues & Solutions

### "Specify a namespace to use" Error
You forgot to include `--ns nodespace --db nodespace` in your surreal command.

**Fix:**
```bash
echo "SELECT * FROM node;" | surreal sql \
  --endpoint ws://localhost:8000 \
  --user root --pass root \
  --ns nodespace --db nodespace  # ← Add these
```

### Empty Results When Querying
Make sure you're querying the right namespace/database.

**Verify:**
```bash
echo "INFO FOR DB;" | surreal sql \
  --endpoint ws://localhost:8000 \
  --user root --pass root \
  --ns nodespace --db nodespace
```

Should show `tables: { has_child: ..., node: ..., mentions: ... }`

### Nodes in UI But Not in Database
Likely a persistence layer issue. Check:
1. Dev-proxy logs for errors
2. Browser console for failed API calls
3. Network tab to see if PATCH requests succeeded

---

## Key Files to Reference

- **Schema definition**: `packages/core/src/db/surreal_store.rs` (lines 480-546)
- **Query implementation**: `packages/core/src/db/surreal_store.rs` (methods for get_children, get_parent, etc.)
- **Dev proxy**: `packages/desktop-app/src-tauri/src/bin/dev-proxy.rs`

---

## Quick Reference: Surreal SQL Syntax for Relationships

```sql
-- Create a parent-child edge
RELATE $parent_id->has_child->$child_id;

-- Query children (graph traversal)
SELECT * FROM $parent_id->has_child->node;

-- Query parent (reverse traversal)
SELECT * FROM node WHERE id IN (
  SELECT VALUE in FROM has_child WHERE out = $child_id
);

-- Count incoming edges
SELECT count(<-has_child) FROM node;

-- Recursive tree query
SELECT id, content, (
  SELECT id, content FROM ->has_child->node
) AS children
FROM node WHERE count(<-has_child) = 0;
```
