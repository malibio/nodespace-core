# Browser Development Mode

## Overview

Browser development mode allows rapid frontend iteration using SurrealDB's HTTP API, bypassing the Tauri desktop shell. This mode is ideal for:

- **UI/UX development** - Fast hot-reload without Tauri rebuilds
- **Component iteration** - Svelte components update instantly
- **Debugging** - Full browser DevTools access
- **Database inspection** - Connect Surrealist to view data in real-time

## Architecture

**Browser Dev Mode:**
```
Frontend (Svelte) → HttpAdapter → SurrealDB HTTP API (port 8000)
```

**Desktop Mode (Production):**
```
Frontend (Svelte) → TauriAdapter → Tauri IPC → SurrealStore (embedded RocksDB)
```

## Quick Start

### 1. Start Development Server

```bash
bun run dev
```

This single command:
- Starts SurrealDB server on `http://127.0.0.1:8000`
- Initializes database schemas (idempotent)
- Starts Vite dev server on `http://localhost:5173`
- Opens browser automatically

### 2. Open Application

Navigate to: `http://localhost:5173`

The application will automatically use `HttpAdapter` in browser mode.

## SurrealDB Configuration

### Default Settings

- **Server**: `http://127.0.0.1:8000`
- **Namespace**: `nodespace`
- **Database**: `nodes`
- **Username**: `root`
- **Password**: `root`

### Storage Modes

**In-Memory Mode (Default):**
```bash
bun run dev
# or explicitly
bun run dev:db:memory
```

Data is lost when server stops. Fast, clean state for testing.

**Persistent Mode:**
```bash
bun run dev:db
```

Data persists in `~/.nodespace/database/nodespace-dev`. Survives server restarts.

### Manual Schema Initialization

Schemas are initialized automatically on `bun run dev`, but you can re-run manually:

```bash
bun run dev:db:init
```

This script (`scripts/init-surrealdb.ts`):
- Creates core schemas: `task`, `text`, `date`, `header`, `code-block`, `quote-block`, `ordered-list`
- Is idempotent - safe to run multiple times
- Checks for existing schemas before creating
- Stores schemas as nodes with `node_type="schema"`, `uuid=schema-name`

## Connecting Surrealist

[Surrealist](https://surrealdb.com/surrealist) is SurrealDB's official database explorer. Use it to inspect data in real-time while developing.

### 1. Install Surrealist

Download from: https://surrealdb.com/surrealist

### 2. Create Connection

**Connection Settings:**
- **Endpoint**: `http://127.0.0.1:8000`
- **Namespace**: `nodespace`
- **Database**: `nodes`
- **Username**: `root`
- **Password**: `root`

### 3. Explore Data

**View All Nodes:**
```sql
SELECT * FROM node;
```

**View Specific Node Type:**
```sql
SELECT * FROM node WHERE node_type = "task";
```

**View Schemas:**
```sql
SELECT * FROM node WHERE node_type = "schema";
```

**View Mentions:**
```sql
SELECT * FROM mention;
```

### 4. Live Data Sync

Changes made in the browser app appear instantly in Surrealist. You can also:
- Execute queries directly
- Modify data and see updates in the app
- Inspect database structure
- Monitor query performance

## HttpAdapter Implementation

### How It Works

`HttpAdapter` (`packages/desktop-app/src/lib/services/backend-adapter.ts`) translates NodeSpace operations to SurrealQL queries over HTTP.

**Key Technical Details:**

1. **Stateless HTTP API**: SurrealDB's HTTP endpoint doesn't persist namespace/database from headers. Solution: prepend `USE NS nodespace DB dev;` to every query.

2. **Query Execution**:
```typescript
async surrealQuery<T>(query: string): Promise<T[]> {
  const fullQuery = `USE NS nodespace DB dev; ${query}`;
  // Execute via fetch to http://127.0.0.1:8000/sql
}
```

3. **Authentication**: Basic Auth with `root:root` credentials in Development mode only.

4. **Response Handling**: SurrealDB returns array of result objects. Adapter extracts results from `[0].result`.

### Supported Operations

**CRUD Operations:**
- `createNode(node)` → `CREATE node CONTENT {...}`
- `getNode(uuid)` → `SELECT * FROM node WHERE uuid = $uuid`
- `updateNode(uuid, updates)` → `UPDATE node:⟨uuid⟩ MERGE {...}`
- `deleteNode(uuid)` → `DELETE node:⟨uuid⟩`

**Query Operations:**
- `queryNodes(query)` → `SELECT * FROM node WHERE ...`
- `getChildren(uuid)` → `SELECT * FROM node WHERE parent_uuid = $uuid`
- `getNodesByRootId(id)` → `SELECT * FROM node WHERE root_id = $id`

**Schema Operations:**
- `getSchema(name)` → `SELECT * FROM node WHERE node_type = "schema" AND uuid = $name`

**Mention Operations:**
- `createMention(mention)` → `CREATE mention CONTENT {...}`
- `deleteMention(source, target)` → `DELETE mention WHERE ...`
- `getMentions(uuid)` → `SELECT * FROM mention WHERE source_uuid = $uuid OR target_uuid = $uuid`

**Optimistic Concurrency Control (OCC):**
- All updates check version numbers
- Conflicts throw `VersionConflictError`
- Frontend retries with fresh data

## Troubleshooting

### Port Already in Use

**Error**: `Address already in use (os error 48)`

**Solution**:
```bash
# Find process using port 8000
lsof -i :8000

# Kill it
kill -9 <PID>

# Or use the kill script
bun run dev:db:kill
```

### Schema Not Found

**Error**: Node operations fail with schema validation errors

**Solution**:
```bash
# Reinitialize schemas
bun run dev:db:init
```

### Connection Refused

**Error**: `fetch failed` or `ECONNREFUSED`

**Cause**: SurrealDB server not running

**Solution**:
```bash
# Start SurrealDB
bun run dev:db:memory
# or
bun run dev:db
```

### Authentication Failed

**Error**: `401 Unauthorized`

**Cause**: Incorrect credentials or missing auth header

**Solution**: Verify `HttpAdapter` sends Basic Auth with `root:root`

## Performance Considerations

### Development vs Production

**Browser Dev Mode (Development)**:
- ✅ Fast hot-reload
- ✅ Full DevTools
- ✅ Database inspection
- ❌ Slower than native (HTTP overhead)
- ❌ Requires SurrealDB server

**Desktop Mode (Production)**:
- ✅ Native performance
- ✅ Embedded database (no server)
- ✅ Offline capable
- ✅ Single executable
- ❌ Requires Tauri rebuild for frontend changes

### HTTP Overhead

Each operation has ~1-5ms HTTP overhead vs native IPC:
- **Browser mode**: ~5-15ms per operation
- **Desktop mode**: ~1-5ms per operation

This is acceptable for development. Use desktop builds for performance testing.

## Best Practices

### When to Use Browser Mode

- ✅ Svelte component development
- ✅ CSS/styling iteration
- ✅ UI layout debugging
- ✅ Frontend logic testing
- ✅ Database inspection with Surrealist

### When to Use Desktop Mode

- ❌ Performance testing
- ❌ Native API testing (file system, window management)
- ❌ Production builds
- ❌ Offline scenarios

### Development Workflow

1. **Start dev server**: `bun run dev`
2. **Develop in browser** with hot-reload
3. **Inspect data** with Surrealist
4. **Test in desktop** periodically: `bun run tauri dev`
5. **Run tests** before committing: `bun run test:all`

## Migration from Old Dev Server

**Before (Custom HTTP Server):**
```
Frontend → HTTP (port 3001) → Custom Node Endpoints → SurrealStore
```

**After (SurrealDB Native):**
```
Frontend → HTTP (port 8000) → SurrealDB HTTP API → SurrealDB Storage
```

**Benefits:**
- ✅ Removed 1,833 lines of custom server code
- ✅ Direct SurrealQL queries (no translation layer)
- ✅ Surrealist integration (real-time inspection)
- ✅ Standard SurrealDB tooling works
- ✅ Simpler architecture

**Breaking Changes:**
- ❌ Old `bun run dev:server` removed
- ✅ New `bun run dev` starts SurrealDB instead
- ✅ Port changed: 3001 → 8000

## Related Documentation

- [Development Setup](deployment/development-setup.md)
- [Testing Guide](testing-guide.md)
- [Architecture Overview](../core/system-overview.md)
- [HttpAdapter Implementation](packages/desktop-app/src/lib/services/backend-adapter.ts)
- [Schema Initialization Script](packages/desktop-app/scripts/init-surrealdb.ts)
