# NodeSpace Skill

NodeSpace is a local-first knowledge graph that stores notes, tasks, and structured data on your machine. It persists across sessions — what you save today is searchable tomorrow.

## How NodeSpace Thinks (Mental Model)

**Everything is a node.** A node has a type, markdown content, and optional typed properties. Node types are schema-defined: `text`, `task`, and `date` are built-ins; custom types come from user-defined schemas (`nodespace schema list` shows what's registered).

**Built-in node types:**
- `text` — freeform notes, documents, findings, summaries
- `task` — structured to-do items; carry `status` (`open`/`in_progress`/`done`/`cancelled`), `due_date` (YYYY-MM-DD), and `priority` (`low`/`medium`/`high`)
- `date` — daily container nodes (e.g. "2026-05-30"); each day has one. Attach time-sensitive findings under the relevant date node so they're retrievable by day.

**Hierarchy is first-class edges, not nesting.** A node has one parent edge. Children are ordered via fractional ordering — siblings have a stable position without gap-numbering. Moving or reordering a node is an edge operation (change the parent or sibling position), not a recreate-and-delete.

**Relationships are distinct from hierarchy and mentions.** A relationship is a named, schema-defined edge between two nodes (e.g. `billed_to`, `has_task`) — different from the one parent edge and from inline `mention` links captured from markdown content. Relationships must be defined on a schema (via `nodespace schema create`/`nodespace schema update`) before they can be used; `nodespace relationship create` on a node whose schema has no matching relationship name fails.

**Content is markdown.** Store prose, code blocks, lists — whatever fits the note. The export commands render it back as clean markdown.

## When to Use NodeSpace (Session Judgment)

Use NodeSpace as a working memory across sessions:

1. **Search at session start** — run the preflight, then search for prior context before you begin. (`nodespace search "topic"`)
2. **Save as you go** — save discoveries, decisions, and summaries during the session. Don't wait until the end.
3. **It persists across sessions** — your context window does not. Anything worth remembering next time should be stored.

Date nodes make temporal retrieval reliable: if a finding is time-bound, attach it under today's date node so future searches can scope by day.

## Shared Workspaces (Multi-User)

A NodeSpace collection can be **synced and shared** with a teammate through NodeSpace Pro. When the daemon is bound to a shared collection, another engineer — or their agent — reads and writes the same graph, so you share context across sessions and across people. It is opt-in and private to its members. When you are working in a shared workspace, adjust how you use NodeSpace:

**You share the whole workspace, not a per-command target.** You do not pick the shared collection per write — the daemon is launched already bound to it, so nodes you create sync into it automatically. Everything you save here is visible to your teammate. Keep private scratch notes in a separate, unshared database (`nodespace database create`/`--database`) if you need them.

**You are not the only writer.** A node here may have been created or last edited by your teammate. Don't assume a node is yours or that its content is stable across your session. Search at session start to pull what your teammate has already saved.

**Attribute what you save.** Put the provenance in the content — who wrote it and when — so a teammate can tell where a note came from (e.g. begin a session summary with your name and the date). NodeSpace records a creator per node, but a human-readable marker helps a teammate scan.

**Prefer additive writes over editing a teammate's node.** Add a new node (a child, or a fresh note) rather than rewriting one your teammate authored. When you must update a shared node, pass the `version` you read via `nodespace node batch-update`, so a concurrent edit surfaces as an OCC conflict instead of silently overwriting it. (`node update` without a version bypasses that check — avoid it for shared nodes.)

**Recall is eventually consistent, and semantic search lags.** A teammate's write appears after sync latency, not instantly. Two caveats are specific to shared sync:
- **For immediate cross-engineer recall, use structured queries** — `nodespace query` or `nodespace node query --content-contains "..."`. A peer's node is queryable as soon as it syncs.
- **Semantic `nodespace search` over a teammate's node works only after your machine has embedded it.** Embeddings are generated locally, not synced, so there is a lag and it needs the local inference model loaded. If a recent teammate note isn't in `search` yet, fall back to `nodespace query`.

**Don't file shared memory under date nodes.** The single-user pattern of attaching findings under `--parent "YYYY-MM-DD"` does **not** round-trip through sync yet — date-container nodes stay local. In a shared workspace, save findings as regular nodes (optionally organized under a shared project or collection node), not under a date node, or your teammate won't see them.

**It's private and opt-in.** The shared collection is readable only by its members and syncs only after each engineer has signed in and enabled sync. Don't move sensitive or unrelated notes into it without intent.

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
| `command not found: nodespace` | CLI not installed or not on `$PATH` | Tell the user NodeSpace CLI is not installed and propose installing it — never run the installer without their explicit confirmation. If they confirm, run `sh -c "$(curl -fsSL https://nodespace.ai/install.sh)" -- --no-gui` (installs the CLI only, non-interactively — the same script the one-line install and `brew install --cask nodespaceai/nodespace/nodespace` both use). Then retry the original command. If it still fails because this shell session hasn't picked up the updated `$PATH`, tell the user to open a new terminal and try again. If they decline the install, stop. |
| `Could not connect to nodespaced` | Daemon not running | Surface the CLI's own message to the user: start the daemon with `nodespaced`. Do not retry automatically — wait for confirmation. |
| `diagnostics` shows entries in `errors` | Database issues | Report the specific error messages to the user before continuing. |

## Prerequisites

NodeSpace daemon must be running. The `nodespace` CLI communicates with `nodespaced` over a Unix socket. If the daemon is not running, CLI commands will fail with a connection error.

Start the daemon: `nodespaced` (or it starts automatically on login if installed via DMG).

## Tool Decision Guide

Use this to pick the right command for the task at hand.

### Finding things

| Goal | Command |
|------|---------|
| Find nodes by keywords or meaning | `nodespace search "<query>"` |
| List all nodes of a type | `nodespace search "" --type <type>` |
| Filter by property values (status, due_date, priority, etc.) | `nodespace query --type <type> --filters '<json>'` |
| Filter with comparison operators (gt, lt, gte, lte, in) | `nodespace query --type <type> --filters '<json>'` |
| Exact substring match on content or title | `nodespace node query --content-contains "..."` / `--title-contains "..."` |
| Get a specific node by ID | `nodespace node get <id>` |

**`nodespace node query` is for exact substring/type matching only** (`--content-contains`, `--title-contains`, `--mentioned-by`, `--type`). It has no property-filter flags.

**`nodespace query` is the command for structured property queries** — status, due_date, priority, or any comparison operator. Examples:
- "find all my open tasks" → `nodespace query --type task --filters '[{"type":"property","operator":"equals","property":"status","value":"open"}]'`
- "tasks due tomorrow" → `nodespace query --type task --filters '[{"type":"property","operator":"equals","property":"due_date","value":"<YYYY-MM-DD>"}]' --sorting '[{"field":"due_date","direction":"asc"}]'`
- "tasks due this week" → `nodespace query --type task --filters '[{"type":"property","operator":"gte","property":"due_date","value":"<week start>"},{"type":"property","operator":"lte","property":"due_date","value":"<week end>"}]'`
- "high priority tasks" → `nodespace query --type task --filters '[{"type":"property","operator":"equals","property":"priority","value":"high"}]'`

Date format for all date properties: **YYYY-MM-DD**. Available operators: `equals`, `contains`, `gt`, `lt`, `gte`, `lte`, `in`, `exists`. Filter types: `property`, `content`, `relationship`, `metadata`.

**`nodespace search` is semantic** (embedding-based similarity), ranked by relevance. Pass `--type` to narrow to one or more node types, `--limit` to cap results (default 20). It does not currently expose graph-boost, cross-collection exclusion, or edge-inclusion — for those, fall back to `nodespace query` plus a follow-up `nodespace relationship get` if you need connected nodes.

**Multiple topics:** run `nodespace search` once per topic rather than one broad search plus per-result fetches.

## CLI Reference

All commands accept `--json` for machine-readable output.

**Selecting a database.** A single daemon can serve several local databases. The data commands that read or write a database (`node`, `query`, `search`, `mention`, `schema`, `relationship`, `import`, `diagnostics`) accept a global `--database <name|id>` flag that routes the request to a specific database; the `NODESPACE_DATABASE` environment variable sets the same target when the flag is absent. Without either, requests go to the daemon's default database. Model management (`nodespace model`) is daemon-global — the loaded inference model is shared across all databases, so the flag is accepted but has no effect there. Manage the set of databases with the `nodespace database` subcommands (below).

```bash
nodespace --database work node create --type text --content "work note"
NODESPACE_DATABASE=work nodespace search "meeting notes"
```

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

**Creating an instance of a custom type:** read the schema first (`nodespace schema get <type>`) so you know its fields. Use the field name exactly as it appears in the schema's `fields[].name` — do not add namespace prefixes when setting properties on instances (namespace prefixing, where it applies, is a schema-authoring concern — see Schema fields below). If the schema has a `title_template`, `--content` only needs a brief descriptive label — the display title is generated from properties. If there's no `title_template`, set `--content` to the best human-readable name available.

Only include properties the schema actually defines as required, plus any optional ones the user gave a value for. Don't invent fields.

**Success semantics:** once `node create` returns an ID, the node exists — confirm what was created to the user and stop. Don't immediately `node get` the same ID to verify; the create response is the confirmation.

### Get a node

```bash
nodespace node get <node-id>
```

**Output:** Full node JSON including all properties

### Update a node

```bash
nodespace node update <node-id> --content "Updated content"
nodespace node update <node-id> --property status=in_progress --property priority=high
```

**Options:**
- `--content <text>` — replaces the node's content/title. Omit to leave content unchanged.
- `--property key=value` — repeatable; sets one property, deep-merged into existing properties (properties you don't mention are left untouched). Values are parsed as JSON when possible (numbers, booleans, arrays, objects), otherwise treated as a plain string.

At least one of `--content` or `--property` is required.

**Find then update:** if you don't already have the node's ID, run `nodespace search` or `nodespace node query` first to locate it, then update by ID. If the search comes back with zero matches or several equally plausible matches, ask the user one specific clarifying question rather than retrying — e.g. "I found 3 tickets in review — which one did you mean: the auth one, the CI one, or the audit-log one?"

**Do NOT use `node update --property status=...` for task status changes** — use `nodespace node set-status` instead (below); it validates against the allowed status values before writing.

**Output:** Updated node JSON. Confirm the change to the user from this response — don't re-fetch the node afterward to double-check.

### Set a task's status

```bash
nodespace node set-status <task-id> in_progress
```

Dedicated verb for task status transitions. Status must be one of: `open`, `in_progress`, `done`, `cancelled` — invalid values are rejected before the update is sent.

**Output:** Updated node JSON.

### Delete a node

```bash
nodespace node delete <node-id>
```

**Find then delete:** locate the node via `nodespace search` or `nodespace node query` if you don't have its ID, and confirm the title matches what the user described before deleting. Delete one node per call; confirm each deletion before moving to the next. Don't search again afterward to verify the deletion — the delete response confirms it.

**Output:** Confirmation JSON

### List children

```bash
nodespace node children <parent-id>
```

**Output:** JSON array of child nodes

### Query nodes (exact-match filter)

```bash
nodespace node query --type task
nodespace node query --content-contains "authentication" --limit 10
nodespace node query --title-contains "Ticket" --type text
nodespace node query --mentioned-by <node-id>
```

**Options:**
- `--type <type>` — filter by node type
- `--content-contains <text>` — substring match in content
- `--title-contains <text>` — substring match in title
- `--mentioned-by <id>` — nodes mentioned by the given node
- `--limit <n>` — max results
- `--offset <n>` — pagination offset

**Note:** for property-level filtering (status, due_date, priority, etc.) or comparison operators, use `nodespace query` (below) — this command only does exact substring/type matching.

**Output:** JSON array of matching nodes

### Structured property query

```bash
nodespace query --type task --filters '[{"type":"property","operator":"equals","property":"status","value":"open"}]'
nodespace query --type task --filters '[{"type":"property","operator":"gte","property":"due_date","value":"2026-07-11"}]' --sorting '[{"field":"due_date","direction":"asc"}]' --limit 20
```

**Options:**
- `--type <type>` — target node type, or `*` for all types
- `--filters <json>` — array of filter conditions: `{"type":"property"|"content"|"relationship"|"metadata","operator":"equals"|"contains"|"gt"|"lt"|"gte"|"lte"|"in"|"exists","property":"...","value":...}`
- `--sorting <json>` — array of `{"field":"...","direction":"asc"|"desc"}`
- `--limit <n>` — max results (0 = server default of 50; server caps at 500 regardless of the value passed)

See the Tool Decision Guide above for worked examples. This is the CLI counterpart of the property-filtering path of the local agent's `search_nodes` tool.

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
nodespace search "" --type task    # list all nodes of a type (empty query)
```

**Options:**
- `--type <type>` — filter by node type (repeatable)
- `--collection <path>` / `--collection-id <id>` — narrow to a collection (mutually exclusive)
- `--filters <json>` — array of `{field, operator, value}` filter objects
- `--threshold <0.0-1.0>` — similarity cutoff (0.0 = server default of 0.7); lower it (e.g. 0.1-0.2) for broader recall when results are sparse
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

Mentions are inline references captured from markdown content — distinct from schema-defined relationships (below), which are named, typed edges.

### Typed relationships

```bash
# Create a relationship edge (relationship name must exist on the source node's schema)
nodespace relationship create --from <source-id> --type has_task --to <target-id>
nodespace relationship create --from <source-id> --type billed_to --to <target-id> --edge-data '{"note":"..."}'

# Traverse relationships from a node
nodespace relationship get <node-id> --type has_task --direction out
nodespace relationship get <node-id> --type billed_to --direction in
```

**Options (`create`):**
- `--from <id>` — source node ID
- `--type <name>` — relationship name, as defined on the source node's schema (e.g. `has_task`, `billed_to`) — not an arbitrary label
- `--to <id>` — target node ID
- `--edge-data <json>` — optional JSON-encoded edge properties

**Options (`get`):**
- `<id>` — node ID to query relationships for
- `--type <name>` — relationship name to filter by
- `--direction <out|in>` — traversal direction (default: `out`)

Both node IDs must already exist, and the relationship name must be defined on the source node's schema — search for missing IDs first (`nodespace search` / `nodespace node query`), and define the relationship on the schema first (`nodespace schema create`/`update`) if it isn't there yet. `relationship create` on a node whose schema doesn't define that relationship name fails with an error naming the undefined relationship.

**Success semantics:** after `relationship create` returns, confirm the link to the user — don't call `relationship get` afterward just to verify it landed.

**Output:** confirmation of the created edge, or the list of related nodes with `count`/`direction`/`relationship_name`.

### Schema inspection and management

```bash
# List all registered schemas
nodespace schema list

# Get a specific schema definition
nodespace schema get task
nodespace schema get person

# Create a new schema
nodespace schema create --params '{"name":"Ticket","description":"A tracked unit of engineering work","fields":[{"name":"status","type":"enum","required":true,"coreValues":[{"value":"ready_for_dev","label":"Ready for Dev"},{"value":"in_dev","label":"In Dev"},{"value":"done","label":"Done"}]},{"name":"assignee","type":"text"}],"relationships":[{"name":"belongs_to_sprint","targetType":"sprint","direction":"out","cardinality":"one"}]}'

# Create a schema with a unique field — key flagged unique_case_insensitive
nodespace schema create --params '{"name":"ADR","description":"An architecture decision record","fields":[{"name":"key","type":"text","required":true,"unique_case_insensitive":true},{"name":"status","type":"enum","required":true,"coreValues":[{"value":"proposed","label":"Proposed"},{"value":"accepted","label":"Accepted"},{"value":"superseded","label":"Superseded"}]}]}'

# Update an existing schema — add/remove/rename fields, without re-creating it
nodespace schema update --params '{"schema_id":"ticket","add_fields":[{"name":"sprint","type":"text"}]}'
```

`create`/`update` take a single JSON `--params` blob (or `--params-file <path>` for a file) rather than per-field flags — the params shape mirrors `CreateSchemaParams`/`UpdateSchemaParams` in the daemon.

<!-- BEGIN GENERATED: schema-rules (see packages/agent/src/skill_rules.rs, packages/cli/examples/gen_skill_md.rs) -->
**One schema per request.** Create exactly the type asked for, in a single `schema create` call, then stop and report it. Don't proactively create related types the user didn't ask for (e.g. asked for "ADR" — don't also create "Ticket" or "Sprint"), and don't follow up with `schema update` to wire relationships unless explicitly asked. A relationship's `targetType` must already exist (check `nodespace schema list`); if it doesn't, omit the relationship rather than creating the other type as a side effect.

If `create` reports the schema already exists, stop and tell the user — they can create instances with `node create` against the existing type.

If `create` rejects the schema with a validation error (not "already exists") — for example a `title_template` placeholder missing from `fields`, or an invalid field type — the error names the specific problem. Fix exactly that and retry immediately with the corrected payload; don't ask the user to clarify and don't give up after one rejection.

**Editing:** to add, remove, or rename a field, or change a relationship on an existing schema, use `schema update` with only the fields that need changing (`add_fields`/`remove_fields`/`rename_fields`, or an updated `description`/`title_template`). Don't re-create the whole schema for a small change.

**Rename vs. relabel:** `rename_fields` can rename a field's storage key or relabel its display name only — see the tool schema for the `from`/`to`/`friendlyName` shape of each. A user asking to relabel what a field is called on screen almost always means the display label, not a storage rename.

**Schema fields:** define only type-specific fields — don't add a `name` or `title` field; every node already has a built-in content/title field. Exception: if `title_template` uses a `{name}` placeholder, `name` must be defined as a field (any placeholder in `title_template` must have a matching field).

**Field source:** derive every field from what the user's own request describes wanting to track — never from another schema shown in the entity-types context. That listing exists so you don't recreate a type that already exists; it is not a shape to copy fields from for a new, unrelated type.

**Enums:** lowercase values with readable labels — `{"value":"in_progress","label":"In Progress"}`.

**Relationships vs. fields:** use a relationship (not a field) when a value references another node type. `targetType` must be an existing schema ID. Examples: `{"name":"supersedes","targetType":"adr","direction":"out","cardinality":"one"}`, `{"name":"has_task","targetType":"task","direction":"out","cardinality":"many"}`.

**Title template:** set `title_template` when a node's identity comes from its fields rather than free-form content, using `{field_name}` placeholders — every placeholder must be a defined field. Omit it if the content/title field alone identifies the node.

**Unique fields:** set `"unique": true` on a field when the user's request implies each instance should have a distinct value for it (e.g. "each ticket should have a unique key" → flag `key` unique). Use `"unique_case_insensitive": true` instead when case shouldn't matter — email and username are the common case. This is advisory only: it does not prevent duplicates from being created, it only lets the system surface a likely existing match when a new value collides. Never describe it to the user as blocking or rejecting duplicates — it's a suggestion, not an enforced constraint. Example: `{"name":"key","type":"text","unique_case_insensitive":true}`.
<!-- END GENERATED: schema-rules -->

A `description` field is fine when it adds value beyond the title. Field names are alphanumeric-and-underscore only — the CLAUDE.md-documented `custom:` namespace prefix convention applies to natural-language schema authoring in the local agent, not to explicit `fields` arrays passed here; don't prefix field names when calling `schema create`/`update` directly.

**Output:** Schema nodes as JSON (same shape as regular nodes; `node_type="schema"`)

### Manage local databases

One daemon serves a registry of local databases. These subcommands operate on that registry globally — they are never affected by the `--database` flag.

```bash
# List every registered database (the default is marked with *)
nodespace database list

# Create a brand-new database and register it
nodespace database create work
nodespace database create work --path /path/to/work.db   # explicit file location

# Register an existing database file already on disk
nodespace database register /path/to/existing.db

# Rename a database's label (by name or id)
nodespace database rename work "Work Projects"

# Set the daemon-wide default database (used by requests without --database)
nodespace database use work

# Unregister a database (never deletes the underlying file)
nodespace database remove work
```

**Options:**
- `create <name> [--path <path>]` — omit `--path` to let the daemon place the file under its managed database directory
- `rename <name|id> <new-name>` — relabels the entry without moving the file
- `use <name|id>` — sets the registry's default; all clients that don't pass `--database`/`NODESPACE_DATABASE` route here afterwards
- `remove <name|id>` — detaches the registry entry only; the database file is left on disk

A database is addressed by **name or id**. When a name is ambiguous (shared by more than one database), select by id instead — `database list --json` shows each id.

**Output:** `list` prints a table (or the full list with `--json`); the other commands print the affected database record (`--json` emits the full `DatabaseInfo`).

### Complete command surface

<!-- BEGIN GENERATED: cli-surface (see packages/cli/src/lib.rs (clap derive), packages/cli/examples/gen_skill_md.rs) -->
Every command, subcommand, and flag below is generated from the CLI's own definitions, so this list is exhaustive and cannot fall behind the binary.

**Global flags** (accepted on every command):

- `--json` — Emit raw JSON instead of human-readable output
- `--socket <SOCKET>` — Override the socket path (default: ~/.nodespace/daemon.sock). Honors the `NODESPACED_SOCKET` environment variable when this flag is absent (env: `NODESPACED_SOCKET`)
- `--database <DATABASE>` — Target a specific local database by name or id (ADR-053). When omitted, requests route to the daemon's default database. Honors the `NODESPACE_DATABASE` environment variable when this flag is absent (env: `NODESPACE_DATABASE`)

### `nodespace node`

Operate on individual nodes (get, create, update, delete, children, query, export, batch-get, batch-update)

**`nodespace node get`** — Retrieve a node by ID

- `<ID>` — Node ID (UUID) (required)

**`nodespace node create`** — Create a new node

- `--type <NODE_TYPE>` — Node type, e.g. `text`, `task`, `date` (required)
- `--content <CONTENT>` — Content (plain text or markdown) (required)
- `--parent <PARENT>` — Parent node ID (omit to create a root node)

**`nodespace node update`** — Update an existing node's content and/or properties

- `<ID>` — Node ID to update (required)
- `--content <CONTENT>` — New content. Omit to leave content unchanged (e.g. when only setting properties)
- `--property <PROPERTIES>` — Set one or more properties: `--property key=value` (repeatable). Values are parsed as JSON when possible (numbers, booleans, `null`, arrays, objects), otherwise treated as a plain string. Deep-merged into the node's existing properties (unspecified keys are left untouched). Do NOT use this to change a task's status; use `node set-status` instead

**`nodespace node set-status`** — Set a task node's status (dedicated verb — do not use `update` for this)

- `<ID>` — Task node ID (required)
- `<STATUS>` — New status. Must be one of: open, in_progress, done, cancelled (required)

**`nodespace node delete`** — Delete a node

- `<ID>` — Node ID to delete (required)

**`nodespace node children`** — List the direct children of a node

- `<ID>` — Parent node ID (required)

**`nodespace node query`** — Query nodes with structured filters

- `--id <ID>` — Filter by exact node ID
- `--mentioned-by <MENTIONED_BY>` — Filter nodes that mention this node ID
- `--content-contains <CONTENT_CONTAINS>` — Filter by substring in content
- `--title-contains <TITLE_CONTAINS>` — Filter by substring in title
- `--type <NODE_TYPE>` — Filter by node type (e.g. `text`, `task`)
- `--limit <LIMIT>` — Maximum number of results (0 = server default)
- `--offset <OFFSET>` — Result offset for pagination

**`nodespace node export`** — Export a node and its subtree as markdown

- `<ID>` — Node ID to export (required)
- `--children <CHILDREN>` — Include children recursively (default: true)
- `--max-depth <MAX_DEPTH>` — Maximum recursion depth (0 = server default of 20)
- `--node-ids <NODE_IDS>` — Embed HTML comments with node IDs for OCC (default: true)

**`nodespace node batch-get`** — Fetch multiple nodes in one request

- `--id <IDS>` — Node IDs to fetch (repeatable: --id <id1> --id <id2>) (required)

**`nodespace node batch-update`** — Update multiple nodes in one request (OCC-aware)

- `--updates <UPDATES>` — JSON-encoded array of update objects: [{"node_id":"…","content":"…","version":N}]. Each item may have: node_id (required), version (optional), content, node_type, properties (required)

### `nodespace model`

Manage the local inference model (list, load, recommended)

**`nodespace model list`** — List models in the catalog and their download/load status

**`nodespace model load`** — Load a model (downloading first if needed); streams progress to stdout

- `<MODEL_ID>` — Model id to load, e.g. `gemma-4-e4b-q4km`. Omit to use the recommended model

**`nodespace model recommended`** — Print the recommended model id for this machine's RAM

**`nodespace model status`** — Print the loaded model, the context window granted to it, and host RAM

### `nodespace search`

Semantic search across the knowledge graph

- `<QUERY>` — Free-text query. Pass an empty string or "*" when using --type for type-only listing — both enumerate every node of the type rather than being treated as a literal search term
- `--type <TYPE>` — Filter results to one or more node types (e.g. `--type task --type text`)
- `--collection <COLLECTION>` — Filter to a collection by path (mutually exclusive with --collection-id)
- `--collection-id <COLLECTION_ID>` — Filter to a collection by ID (mutually exclusive with --collection)
- `--filters <FILTERS>` — JSON-encoded array of {field, operator, value} filter objects
- `--threshold <THRESHOLD>` — Semantic similarity threshold, 0.0-1.0 (0.0 = server default of 0.7)
- `--limit <LIMIT>` — Maximum number of results to return (0 = server default, currently 20)

### `nodespace query`

Structured property query with comparison operators (equals/contains/gt/lt/gte/lte/in/exists)

- `--type <TARGET_TYPE>` — Target node type ("task", "text", etc.) or "*" for all types (required)
- `--filters <FILTERS>` — JSON array of filter conditions, e.g. `[{"type":"property","operator":"equals","property":"status","value":"open"}]`. Supported types: property, content, relationship, metadata. Supported operators: equals, contains, gt, lt, gte, lte, in, exists
- `--sorting <SORTING>` — JSON array of sort configs, e.g. `[{"field":"due_date","direction":"desc"}]`
- `--limit <LIMIT>` — Max results to return (0 = server default of 50)

### `nodespace diagnostics`

Developer diagnostics: database path, size, node counts, schema count

### `nodespace import`

Import markdown files into NodeSpace

**`nodespace import file`** — Import a single markdown file

- `<FILE>` — Path to the markdown file (required)
- `--collection <COLLECTION>` — Collection path to assign the document to (e.g. "docs:rust")
- `--use-filename-as-title` — Use the filename stem as the document title
- `--auto-collection-routing` — Route files to collections based on directory structure
- `--replace` — Refresh an already-imported document in place: replace its child subtree from the fresh parse, keeping the root node so inbound links survive. Without this, an already-imported document is left untouched

**`nodespace import dir`** — Import all markdown files from a directory (recurses into sub-folders by default; see --no-recursive)

- `<DIRECTORY>` — Path to the directory containing markdown files (required)
- `--collection <COLLECTION>` — Collection path to assign all documents to
- `--use-filename-as-title` — Use filename stems as document titles
- `--auto-collection-routing` — Route files to collections based on directory structure
- `--exclude <EXCLUDE_PATTERNS>` — Directory names to exclude (repeatable, e.g. --exclude node_modules)
- `--include-agent-files` — Include CLAUDE.md / AGENTS.md files (default: excluded). Matched by basename, case-insensitive, at any depth
- `--include-hidden` — Include hidden files and folders — any path component starting with '.', e.g. .git/, .claude/, dotfiles (default: skipped)
- `--no-recursive` — Import only the top-level directory; do not descend into sub-folders (default: recurses into sub-folders)
- `--replace` — Refresh already-imported documents in place: replace each existing document's child subtree from the fresh parse, keeping its root node so inbound links survive. Without this, already-imported documents are skipped (a plain re-import never duplicates)

### `nodespace mention`

Manage mention relationships between nodes

**`nodespace mention create`** — Create a mention relationship from one node to another

- `--from <FROM>` — The node that contains the mention (source) (required)
- `--to <TO>` — The node being mentioned (target) (required)

**`nodespace mention delete`** — Delete a mention relationship

- `--from <FROM>` — The node that contains the mention (source) (required)
- `--to <TO>` — The node being mentioned (target) (required)

**`nodespace mention outgoing`** — List nodes that a given node mentions (outgoing)

- `<ID>` — Node ID to query mentions for (required)

**`nodespace mention incoming`** — List nodes that mention a given node (incoming)

- `<ID>` — Node ID to query mentions for (required)

### `nodespace schema`

Inspect and manage node type schema definitions

**`nodespace schema list`** — List all schema definitions

**`nodespace schema get`** — Get a single schema definition by ID

- `<ID>` — Schema ID (node type identifier, e.g. `task`, `person`) (required)

**`nodespace schema create`** — Create a new schema from a JSON params blob

- `--params <PARAMS>` — JSON params. For `create`: {"name", "description"?, "fields"?, "relationships"?, "title_template"?, ...} — see CreateSchemaParams. For `update`: {"schema_id", "add_fields"?, "remove_fields"?, "rename_fields"?, "add_relationships"?, "remove_relationships"?, ...} — see UpdateSchemaParams. Mutually exclusive with `--params-file`
- `--params-file <PARAMS_FILE>` — Path to a file containing the JSON params (alternative to inline `--params`)

**`nodespace schema update`** — Update an existing schema from a JSON params blob

- `--params <PARAMS>` — JSON params. For `create`: {"name", "description"?, "fields"?, "relationships"?, "title_template"?, ...} — see CreateSchemaParams. For `update`: {"schema_id", "add_fields"?, "remove_fields"?, "rename_fields"?, "add_relationships"?, "remove_relationships"?, ...} — see UpdateSchemaParams. Mutually exclusive with `--params-file`
- `--params-file <PARAMS_FILE>` — Path to a file containing the JSON params (alternative to inline `--params`)

### `nodespace relationship`

Manage typed relationship edges between nodes (distinct from mentions)

**`nodespace relationship create`** — Create a typed relationship edge from one node to another

- `--from <FROM>` — Source node ID (required)
- `--type <RELATIONSHIP_NAME>` — Relationship name (as defined on the source node's schema) (required)
- `--to <TO>` — Target node ID (required)
- `--edge-data <EDGE_DATA>` — Optional JSON-encoded edge properties

**`nodespace relationship get`** — List nodes related to a given node via a named relationship

- `<ID>` — Node ID to query relationships for (required)
- `--type <RELATIONSHIP_NAME>` — Relationship name (as defined on the node's schema) (required)
- `--direction <DIRECTION>` — Direction to traverse

### `nodespace session`

Manage PTY agent sessions (launch, attach, list, kill)

**`nodespace session launch`** — Launch a new agent session and stream its output to stdout

- `<AGENT>` — Agent to launch: claude-code, codex, gemini, pi, opencode (required)
- `--prompt <PROMPT>` — Initial prompt passed to the agent at launch time
- `--cols <COLS>` — Terminal width in columns (defaults to current terminal width)
- `--rows <ROWS>` — Terminal height in rows (defaults to current terminal height)

**`nodespace session attach`** — Attach to an existing session's output stream

- `<SESSION_ID>` — Session ID to attach to (required)

**`nodespace session list`** — List active agent sessions

**`nodespace session kill`** — Terminate a running session

- `<SESSION_ID>` — Session ID to terminate (required)

### `nodespace database`

Manage the daemon's registry of local databases (list, create, register, remove, rename, use)

**`nodespace database list`** — List every registered database with its status and the default marker

**`nodespace database create`** — Create a brand-new database and register it

- `<NAME>` — Human-facing label for the new database (required)
- `--path <PATH>` — Explicit path for the new database file. When omitted the daemon places it under its managed database directory

**`nodespace database register`** — Register an existing database file already present on disk

- `<PATH>` — Absolute path to an existing database file to register (required)

**`nodespace database remove`** — Unregister a database (never deletes the underlying file)

- `<DATABASE>` — Database to unregister, by name or id (required)

**`nodespace database rename`** — Rename a registered database's human-facing label

- `<DATABASE>` — Database to rename, by name or id (required)
- `<NEW_NAME>` — New human-facing label (required)

**`nodespace database use`** — Set the daemon-wide default database (used when no database is selected)

- `<DATABASE>` — Database to make the default, by name or id (required)

### `nodespace uninstall`

Uninstall NodeSpace: stop daemon, remove binaries and service registration

<!-- END GENERATED: cli-surface -->

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

### Change a task's status

```bash
nodespace node set-status <task-id> done
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

Date node IDs are the date string itself (`"2026-05-30"`). Pass the date string directly as `--parent` — the daemon auto-creates the date node if it doesn't exist yet.

```bash
# Attach a finding under today — date node is created automatically if absent
nodespace node create --type text --content "Discovered: rate limiter uses fixed windows" --parent "2026-05-30"

# To retrieve an existing date node directly
nodespace node get "2026-05-30"
```

### Define a new entity type, then create an instance

```bash
# 1. Create the schema (one schema per request — see Schema inspection and management above)
nodespace schema create --params '{"name":"Ticket","description":"A tracked unit of engineering work","fields":[{"name":"status","type":"enum","required":true,"coreValues":[{"value":"ready_for_dev","label":"Ready for Dev"},{"value":"in_dev","label":"In Dev"},{"value":"done","label":"Done"}]},{"name":"assignee","type":"text"}]}'

# 2. Create an instance
nodespace node create --type ticket --content "Fix flaky retry test" --parent <parent-id>
nodespace node update <the-new-id> --property assignee=dana --property status=in_dev
```

### Link two nodes with a typed relationship

```bash
# Relationship must already be defined on the source's schema (e.g. Ticket.belongs_to_sprint)
nodespace relationship create --from <ticket-id> --type belongs_to_sprint --to <sprint-id>
```

### Organize a node into a collection

Use a relationship with `--type member_of` against the collection node. If the collection doesn't exist as a node yet, ask the user to create it first.

```bash
nodespace relationship create --from <node-id> --type member_of --to <collection-id>
```

### Bulk import from markdown

```bash
nodespace import file ./notes.md
nodespace import dir ./docs --auto-collection-routing
```

Top-level headings become root nodes, sub-headings become children. Report the number of nodes created; don't follow up with search calls to verify.

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
