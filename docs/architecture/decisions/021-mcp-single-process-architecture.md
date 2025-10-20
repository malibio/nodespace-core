# ADR-021: MCP Single-Process Architecture

**Status:** Accepted
**Date:** 2025-01-19
**Deciders:** Development Team + Senior Architect Review
**Supersedes:** Dual-process MCP architecture (archived)

## Context

NodeSpace requires MCP (Model Context Protocol) integration to enable AI agents like Claude Code to programmatically access and modify nodes. The key question was whether to implement MCP as:

1. **Separate process** communicating with desktop app via IPC (Unix Domain Sockets)
2. **In-process Tokio task** running within the Tauri application

## Decision

We will implement MCP as an **async Tokio task running within the Tauri application process**, sharing the NodeService instance and using Tauri events for UI reactivity.

## Rationale

### Primary Use Case Alignment

Our primary use case is:
> "User has NodeSpace open, AI agent (Claude Code) imports markdown/creates nodes, user sees updates in real-time"

This scenario requires:
- Desktop app running (UI visible)
- MCP accessible to AI agent
- Real-time UI updates when MCP modifies data

**Single-process architecture perfectly matches this use case** - when the desktop app is running, MCP is available. When the app closes, the user isn't viewing anything anyway.

### Technical Analysis

**Advantages of Single-Process:**

1. **Simplicity**: No IPC layer to implement (~800 fewer lines of code)
2. **Zero Latency**: Same-process function calls vs socket I/O
3. **Automatic Reactivity**: Existing EventBus + Tauri events handle UI updates
4. **Easier Debugging**: Single process, unified logs, simpler mental model
5. **Memory Efficiency**: Single NodeService instance, no duplication
6. **Proven Pattern**: Standard Tauri background task pattern
7. **No IPC Failure Modes**: No socket binding failures, connection drops, or serialization errors

**Disadvantages of Dual-Process (Rejected):**

1. **High Complexity**: Requires Unix Domain Sockets, event serialization, reconnection logic
2. **IPC Overhead**: Every event crosses process boundary with serialization + socket I/O
3. **Harder Debugging**: Two processes, distributed logs, complex race conditions
4. **Duplicate Infrastructure**: EventBus logic needed in both processes
5. **Speculative Future-Proofing**: "MCP survives desktop close" is not a current requirement

### Senior Architect Recommendation

The senior-architect-reviewer agent provided comprehensive analysis and strongly recommended single-process architecture, citing:

- 70% reduction in implementation complexity
- Better alignment with primary use case
- Leverages existing EventBus infrastructure
- Standard pattern for Tauri background tasks
- Easier maintenance and debugging

## Implementation

### Architecture

```
Tauri Application (Single Process)
┌────────────────────────────────────────────┐
│  Tauri Runtime                              │
│  ├─ Svelte Frontend (reactive stores)       │
│  └─ Event System (existing EventBus)        │
│                                              │
│  Rust Backend (Tokio async)                 │
│  ├─ NodeService (shared Arc<T>)             │
│  ├─ Database Connection Pool                │
│  ├─ Tauri Command Handlers                  │
│  └─ MCP stdio Task (background)             │
│      ├─ stdin reader (async)                │
│      ├─ JSON-RPC handler                    │
│      └─ stdout writer (async)               │
└────────────────────────────────────────────┘
```

### Key Components

**MCP Task Spawning:**
```rust
// In lib.rs setup()
tauri::async_runtime::spawn(async move {
    run_mcp_server(node_service, app_handle).await
});
```

**Reactivity Flow:**
```
MCP Handler → NodeService → Database → Tauri Event → EventBus → Svelte Stores → UI Update
```

**No IPC Needed:** Same-process communication via shared NodeService instance (Arc<T>)

## Consequences

### Positive

1. ✅ **Faster Development**: ~4 days vs 7-10 days for dual-process
2. ✅ **Simpler Codebase**: ~670 LOC vs ~1200+ LOC
3. ✅ **Better Performance**: No IPC serialization overhead
4. ✅ **Easier Debugging**: Single process, unified logs
5. ✅ **Automatic Reactivity**: Existing EventBus "just works"
6. ✅ **Lower Risk**: Fewer failure modes, simpler mental model

### Negative

1. ❌ **Lifecycle Coupling**: MCP stops when desktop app closes
   - **Mitigation**: Matches primary use case; can add headless mode later if needed
2. ❌ **Resource Sharing**: MCP and UI share process resources
   - **Mitigation**: Async I/O prevents blocking; Rust memory safety prevents leaks
3. ❌ **Less Isolation**: MCP panic could theoretically crash desktop app
   - **Mitigation**: Rust error handling + catch_unwind for critical sections

### Deferred Decisions

If we later need "headless MCP server" (e.g., for CI/CD, server deployments):
1. Add `--headless` flag to run Tauri without window
2. Still single-process, just no UI layer
3. Simpler than maintaining dual-process architecture speculatively

## Related Decisions

- **Issue #112**: MCP stdio server implementation (updated scope)
- **Issue #111**: Reactive state synchronization (closed as Won't Do - automatic with single-process)
- **Archived**: `/docs/architecture/decisions/archived/mcp-dual-process-approach.md`

## References

- MCP Integration Documentation: `/docs/architecture/business-logic/mcp-integration.md`
- Senior Architect Review: Conducted 2025-01-19 via agent consultation
- MCP Specification: https://modelcontextprotocol.io
- Tauri Async Runtime: https://tauri.app/develop/calling-rust/
