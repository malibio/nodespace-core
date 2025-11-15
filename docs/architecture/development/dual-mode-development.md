# Dual-Mode Development Architecture

NodeSpace supports two distinct development modes that can run simultaneously for different testing and debugging scenarios.

## Architecture Overview

### Tauri Desktop Mode (Production-like)

**Port**: 1420
**Command**: `bun run dev:tauri` or `bun run tauri:dev`

```
┌─────────────────────────────────────┐
│   Tauri Window (Desktop App)        │
│   http://localhost:1420              │
└──────────────┬──────────────────────┘
               │
        ┌──────▼──────┐
        │ Vite Server │
        │  Port 1420  │
        └──────┬──────┘
               │
    ┌──────────▼──────────────┐
    │   Tauri Rust Backend    │
    │   (runs in-process)     │
    ├─────────────────────────┤
    │ • Embedded RocksDB      │
    │   (file-based database) │
    │ • NodeService           │
    │ • MCP Server (HTTP)     │
    │   Port: 3100            │
    └─────────────────────────┘
```

**Characteristics**:
- ✅ **Production-like**: Matches the actual desktop app architecture
- ✅ **Embedded database**: Uses RocksDB stored in `~/.nodespace/database/`
- ✅ **No external dependencies**: Everything runs within Tauri process
- ✅ **MCP Server**: HTTP transport on port 3100 for AI agent integration
- ✅ **Fast startup**: No need to wait for external servers

### Browser Dev Mode (Debugging-optimized)

**Port**: 5173
**Command**: `bun run dev:browser` or `bun run dev`

```
┌─────────────────────────────────────┐
│   Browser (Chrome/Firefox/etc)      │
│   http://localhost:5173              │
└──────────────┬──────────────────────┘
               │
        ┌──────▼──────┐
        │ Vite Server │
        │  Port 5173  │
        └──────┬──────┘
               │
        ┌──────▼──────────┐
        │   Dev-Proxy     │
        │   Port 3001     │
        │  (HTTP Adapter) │
        └──────┬──────────┘
               │
    ┌──────────▼──────────────┐
    │   SurrealDB HTTP Server │
    │       Port 8000          │
    │   (in-memory database)   │
    └──────────────────────────┘
```

**Characteristics**:
- ✅ **Inspectable**: SurrealDB accessible via Surrealist on port 8000
- ✅ **Browser DevTools**: Full Chrome/Firefox debugging capabilities
- ✅ **Separate backend**: Dev-proxy provides HTTP API on port 3001
- ✅ **Hot reload**: Frontend changes reflect immediately
- ✅ **Network inspection**: Can monitor all API calls in browser

## Port Allocation

| Service | Tauri Mode | Browser Mode | Purpose |
|---------|------------|--------------|---------|
| **Frontend** | 1420 | 5173 | Vite dev server |
| **Backend** | Embedded RocksDB | 3001 (dev-proxy) | API/Business logic |
| **Database** | Embedded RocksDB | 8000 (SurrealDB HTTP) | Data storage |
| **MCP Server** | 3100 (HTTP) | 3100 (HTTP, shared) | AI agent protocol |

## Usage

### Running Tauri Desktop Mode

```bash
# From repository root
bun run tauri:dev

# Or from packages/desktop-app
bun run dev:tauri
```

**What happens**:
1. Vite starts on port 1420
2. Tauri builds and launches the desktop app
3. Embedded RocksDB initializes at `~/.nodespace/database/`
4. MCP server starts on port 3100
5. Desktop window opens loading `http://localhost:1420`

### Running Browser Dev Mode

```bash
# From repository root or packages/desktop-app
bun run dev

# Or explicitly
bun run dev:browser
```

**What happens**:
1. SurrealDB HTTP server starts on port 8000 (in-memory)
2. Dev-proxy starts on port 3001 (connects to SurrealDB)
3. Core schemas are seeded via dev-proxy
4. Vite starts on port 5173
5. Open `http://localhost:5173` in your browser

### Running Both Modes Simultaneously

```bash
# Terminal 1: Browser mode
bun run dev:browser

# Terminal 2: Tauri mode
bun run dev:tauri
```

**Benefits**:
- Compare behavior between modes
- Test database isolation
- Verify embedded vs HTTP database consistency
- Debug UI differences between browser and desktop

## When to Use Each Mode

### Use Tauri Desktop Mode When:

- ✅ Testing desktop-specific features (window management, native menus)
- ✅ Verifying production-like behavior
- ✅ Testing MCP integration with AI agents
- ✅ Debugging embedded database issues
- ✅ Testing offline functionality
- ✅ Final integration testing before release

### Use Browser Dev Mode When:

- ✅ Rapid UI development with hot reload
- ✅ Inspecting database state with Surrealist
- ✅ Debugging API calls with browser DevTools
- ✅ Testing responsive design in different browsers
- ✅ Network throttling and performance testing
- ✅ Debugging backend business logic with database inspection

## MCP Server Configuration

Both modes run an MCP server on **port 3100** (configurable via `MCP_PORT` environment variable).

**Tauri Mode MCP**:
```bash
# Custom MCP port
MCP_PORT=3200 bun run dev:tauri
```

**Browser Mode MCP**:
The MCP server is shared with Tauri mode by default (port 3100), but can be configured separately if needed.

## Database Locations

### Tauri Mode
- **Default**: `~/.nodespace/database/nodespace.db` (unified across platforms)
- **Custom**: Can be changed via Settings → Database Location

### Browser Mode
- **Location**: In-memory (data lost on restart)
- **Persistent option**: Use `dev:browser:db:persist` script for file-based storage

## Troubleshooting

### Port Conflicts

If ports are already in use:

```bash
# Find and kill processes
lsof -ti:1420 | xargs kill -9  # Tauri frontend
lsof -ti:5173 | xargs kill -9  # Browser frontend
lsof -ti:3001 | xargs kill -9  # Dev-proxy
lsof -ti:3100 | xargs kill -9  # MCP server
lsof -ti:8000 | xargs kill -9  # SurrealDB HTTP
```

### Database Not Initializing (Tauri Mode)

Check that the database directory is writable:
```bash
ls -la ~/.nodespace/database/
```

If missing, Tauri will create it automatically on first run.

### Vite Can't Start (Port Already in Use)

The Vite config allows port fallback, but you can force a specific port:
```bash
# Tauri mode with custom port
VITE_PORT=1421 bun run dev:tauri

# Browser mode with custom port
# Edit package.json script or run manually:
bunx vite dev --port 5174
```

## Architecture Benefits

### Isolation
- Completely independent frontend ports prevent conflicts
- Separate backend architecture enables parallel testing
- Different database modes validate storage layer properly

### Flexibility
- Choose the right mode for your debugging needs
- Switch between modes without configuration changes
- Run both simultaneously for comparison

### Production Alignment
- Tauri mode matches production architecture exactly
- Catch desktop-specific issues early
- Validate embedded database behavior before release

## Migration Notes

**Previous Setup** (deprecated):
- Single `dev` command ran all servers (SurrealDB + dev-proxy + Vite)
- Tauri mode also started external servers unnecessarily
- Port conflicts when trying to run both modes

**Current Setup** (improved):
- `dev:browser` - Full debugging stack with external servers
- `dev:tauri` - Production-like embedded database mode
- Clear separation enables simultaneous operation
- No port conflicts, better performance

## Future Enhancements

- [ ] Add development mode selector UI in the app
- [ ] Add MCP server stdio transport option for CLI tools
- [ ] Add database migration tools between modes
- [ ] Add performance comparison metrics between modes
