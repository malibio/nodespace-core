# HTTP Dev Server Architecture

## Overview

The HTTP dev server provides a REST API that mirrors Tauri IPC commands, enabling web-mode testing with real database access. This allows comprehensive testing without requiring a full Tauri desktop environment.

## Problem Statement

**Before HTTP Dev Server:**
- `bun run tauri:dev`: Tauri app with Rust backend via IPC + real database
- `bun run dev`: Web-only mode with NO backend access, mocked services
- Tests had no way to access real backend/database
- AI agents with MCP DevTools couldn't inspect same data as Tauri app

**After HTTP Dev Server:**
- `bun run tauri:dev`: Tauri app uses IPC (unchanged)
- `bun run dev`: Web mode can access real backend via HTTP adapter
- Tests can run against real database via HTTP adapter
- Single codebase works in both modes automatically

## Architecture Diagram

```
Development Mode:
├─ Port 1420: Vite dev server (frontend)
│  ├─ bun run tauri:dev → TauriAdapter (IPC)
│  └─ bun run dev → HttpAdapter (HTTP to port 3001)
│
├─ Port 3001: HTTP dev server (Rust backend)
│  ├─ Phase 1: Node CRUD operations
│  ├─ Phase 2: Query endpoints (added by #211)
│  └─ Phase 3: Embeddings (added by #212)
│
Production Mode:
└─ Tauri app only (IPC, no HTTP server)
```

## Components

### 1. HTTP Dev Server (Rust)

**Location**: `packages/desktop-app/src-tauri/src/dev_server/`

**Structure**:
```
dev_server/
├─ mod.rs              # Main router, CORS, error handling
├─ node_endpoints.rs   # Phase 1 MVP endpoints
├─ query_endpoints.rs  # Phase 2 (placeholder)
└─ embedding_endpoints.rs # Phase 3 (placeholder)
```

**Key Features**:
- Feature-gated (`--features dev-server`) - never compiles in production
- CORS restricted to localhost:1420
- Error responses match Tauri's `CommandError` structure
- Modular routing for easy endpoint addition

**Starting the Server**:
```bash
# Via npm script (recommended)
bun run dev:server

# Or directly
cd packages/desktop-app/src-tauri
cargo run --bin dev-server --features dev-server

# Custom port
DEV_SERVER_PORT=3002 bun run dev:server
```

### 2. Backend Adapter Pattern (TypeScript)

**Location**: `packages/desktop-app/src/lib/services/backend-adapter.ts`

**Components**:
- `BackendAdapter` interface: Defines all backend operations
- `TauriAdapter`: Uses Tauri `invoke()` for IPC
- `HttpAdapter`: Uses `fetch()` to communicate with dev server
- `getBackendAdapter()`: Auto-detects environment and returns appropriate adapter

**Auto-Detection Logic**:
```typescript
export function getBackendAdapter(): BackendAdapter {
  if (typeof window !== 'undefined' && window.__TAURI__) {
    return new TauriAdapter(); // Tauri desktop mode
  } else {
    return new HttpAdapter(); // Web/test mode
  }
}
```

### 3. Test Infrastructure

**Location**: `packages/desktop-app/src/tests/utils/`

**Utilities**:
- `createTestDatabase(testName)`: Creates isolated test database
- `cleanupTestDatabase(dbPath)`: Deletes test database and temp files
- `initializeTestDatabase(dbPath)`: Initializes database via HTTP
- `TestNodeBuilder`: Fluent API for creating test nodes

**Usage Example**:
```typescript
import { createTestDatabase, cleanupTestDatabase, initializeTestDatabase } from '$tests/utils';
import { TestNodeBuilder } from '$tests/utils';

describe('Node CRUD Tests', () => {
  let dbPath: string;

  beforeEach(async () => {
    dbPath = createTestDatabase('node-crud');
    await initializeTestDatabase(dbPath);
  });

  afterEach(async () => {
    await cleanupTestDatabase(dbPath);
  });

  it('should create a node', async () => {
    const node = TestNodeBuilder.text('Hello World')
      .withId('test-1')
      .build();

    // Test with real backend via HTTP adapter
    const adapter = getBackendAdapter();
    const nodeId = await adapter.createNode(node);
    expect(nodeId).toBe('test-1');
  });
});
```

## Phase 1 MVP Endpoints

### Database Initialization
```http
POST /api/database/init?db_path=/path/to/db.db
Response: { "dbPath": "/path/to/db.db" }
```

### Create Node
```http
POST /api/nodes
Content-Type: application/json

{
  "id": "node-123",
  "nodeType": "text",
  "content": "Hello World",
  "parentId": null,
  "containerNodeId": null,
  "beforeSiblingId": null,
  "properties": {}
}

Response: "node-123"
```

### Get Node
```http
GET /api/nodes/node-123
Response: { "id": "node-123", "nodeType": "text", ... }
```

### Update Node
```http
PATCH /api/nodes/node-123
Content-Type: application/json

{
  "content": "Updated content"
}

Response: 200 OK
```

### Delete Node
```http
DELETE /api/nodes/node-123
Response: 200 OK
```

### Get Children
```http
GET /api/nodes/parent-123/children
Response: [{ "id": "child-1", ... }, { "id": "child-2", ... }]
```

## Error Handling

Errors follow the same structure as Tauri's `CommandError`:

```json
{
  "message": "User-facing error message",
  "code": "ERROR_CODE",
  "details": "Optional debug information"
}
```

**HTTP Status Codes**:
- `200`: Success
- `400`: Bad Request (invalid input, validation error)
- `404`: Not Found (resource doesn't exist)
- `500`: Internal Server Error (database error, service error)

## Security Considerations

**Development Only**:
- Server only compiles with `--features dev-server`
- Feature flag prevents inclusion in production builds
- Binary size unaffected by dev server code in production

**CORS**:
- Restricted to `http://localhost:1420` (Vite dev server)
- No authentication (local development only)
- Never exposed to network

## Extending for Phase 2/3

### Adding New Endpoints (Example: Phase 2 Query Endpoints)

**Step 1: Create Endpoint Module**

Create `packages/desktop-app/src-tauri/src/dev_server/query_endpoints.rs`:

```rust
use axum::{routing::get, Router};
use crate::dev_server::AppState;

// Implement your endpoints here
async fn query_nodes(/* ... */) -> Result</* ... */> {
    // Implementation
}

pub fn routes(state: AppState) -> Router {
    Router::new()
        .route("/api/nodes/query", get(query_nodes))
        .with_state(state)
}
```

**Step 2: Update Main Router**

In `packages/desktop-app/src-tauri/src/dev_server/mod.rs`:

```rust
// Uncomment the module
mod query_endpoints;

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .merge(node_endpoints::routes(state.clone()))
        .merge(query_endpoints::routes(state.clone())) // Add this line
        .layer(cors_layer())
}
```

**Step 3: Update TypeScript Adapter Interface**

In `packages/desktop-app/src/lib/services/backend-adapter.ts`:

```typescript
export interface BackendAdapter {
  // Existing Phase 1 methods...

  // Add Phase 2 methods
  queryNodes(query: NodeQuery): Promise<Node[]>;
}
```

**Step 4: Implement in Both Adapters**

Implement the new method in both `TauriAdapter` and `HttpAdapter`.

## Testing the Dev Server

### Manual Testing with curl

```bash
# Start the dev server
bun run dev:server

# In another terminal:

# Initialize database
curl -X POST http://localhost:3001/api/database/init

# Create a node
curl -X POST http://localhost:3001/api/nodes \
  -H "Content-Type: application/json" \
  -d '{
    "id": "test-1",
    "nodeType": "text",
    "content": "Hello World",
    "parentId": null,
    "containerNodeId": null,
    "beforeSiblingId": null,
    "properties": {}
  }'

# Get the node
curl http://localhost:3001/api/nodes/test-1

# Update the node
curl -X PATCH http://localhost:3001/api/nodes/test-1 \
  -H "Content-Type: application/json" \
  -d '{"content": "Updated!"}'

# Delete the node
curl -X DELETE http://localhost:3001/api/nodes/test-1
```

### Automated Testing

```typescript
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { createTestDatabase, cleanupTestDatabase, initializeTestDatabase } from '$tests/utils';
import { getBackendAdapter } from '$lib/services/backend-adapter';
import { TestNodeBuilder } from '$tests/utils';

describe('Backend Adapter Integration Tests', () => {
  let dbPath: string;
  const adapter = getBackendAdapter();

  beforeEach(async () => {
    dbPath = createTestDatabase('adapter-integration');
    await initializeTestDatabase(dbPath);
  });

  afterEach(async () => {
    await cleanupTestDatabase(dbPath);
  });

  it('should perform CRUD operations', async () => {
    // Create
    const node = TestNodeBuilder.text('Test Content').withId('test-1').build();
    await adapter.createNode(node);

    // Read
    const retrieved = await adapter.getNode('test-1');
    expect(retrieved).not.toBeNull();
    expect(retrieved?.content).toBe('Test Content');

    // Update
    await adapter.updateNode('test-1', { content: 'Updated!' });
    const updated = await adapter.getNode('test-1');
    expect(updated?.content).toBe('Updated!');

    // Delete
    await adapter.deleteNode('test-1');
    const deleted = await adapter.getNode('test-1');
    expect(deleted).toBeNull();
  });
});
```

## Performance Considerations

**HTTP Overhead**:
- Typical request: 2-10ms overhead vs IPC
- Negligible for development/testing
- No impact on production (not compiled)

**Database**:
- Separate test database prevents conflicts
- Test isolation enables parallel test execution
- Cleanup utilities prevent disk space issues

## Troubleshooting

### Server won't start

**Error**: `Address already in use`
- **Solution**: Check if another process is using port 3001
- **Alternative**: Use custom port: `DEV_SERVER_PORT=3002 bun run dev:server`

### Tests fail with "Connection refused"

**Solution**: Ensure dev server is running before tests:
```bash
# Terminal 1: Start dev server
bun run dev:server

# Terminal 2: Run tests
bun run test
```

### CORS errors in browser

**Check**:
1. Vite dev server is on port 1420
2. HTTP dev server CORS is configured for localhost:1420
3. Browser dev tools show request origin

### Production build includes dev server code

**Check**:
```bash
# Verify feature flag is working
cargo build --release

# Dev server should NOT be in binary
strings target/release/nodespace-app | grep "dev_server"
# Should return nothing
```

## Related Issues

- #208: Parent epic for comprehensive test coverage
- #209: This issue (HTTP dev server infrastructure)
- #210: Phase 1 tests (uses these endpoints)
- #211: Phase 2 tests (will add query endpoints)
- #212: Phase 3 tests (will add embedding endpoints)
