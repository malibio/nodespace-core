# ARCHIVED: MCP Dual-Process Architecture Approach

**Status:** Superseded by ADR-021 (Single-Process Architecture)
**Date Archived:** 2025-01-19
**Reason:** Chosen approach is simpler and better suited to primary use case

---

## Original Proposal: Dual-Process with IPC

This document archives the originally proposed dual-process architecture for MCP integration, which was considered but ultimately rejected in favor of a single-process approach.

### Architecture Overview

The dual-process approach proposed running MCP as a separate process from the desktop application:

```
Desktop App Process                    MCP Server Process
┌──────────────────┐                  ┌─────────────────┐
│ Tauri Backend    │                  │ MCP stdio loop  │
│                  │                  │                 │
│ NodeService ─────┼──── Database ────┼───── NodeService│
│      ↓           │     (shared)     │        ↓        │
│ EventBus ────────┼──── IPC Socket ──┼──── EventBus    │
│      ↓           │   (bidirectional)│        ↓        │
│ Tauri Events     │                  │  (no UI layer)  │
│      ↓           │                  │                 │
│ Svelte UI        │                  │                 │
└──────────────────┘                  └─────────────────┘
```

### Key Aspects

- **Dual-mode binary** with `--mcp-server` flag
- **Separate processes** for desktop app and MCP server
- **Shared Turso database** with concurrent access (WAL mode)
- **Unix Domain Sockets** for IPC event coordination
- **Independent lifecycle** - MCP could survive desktop close

### Why This Was Rejected

**Trade-off Analysis:**

1. **Excessive Complexity**: Required implementing entire IPC layer (Unix sockets, event serialization, reconnection logic)
2. **IPC Overhead**: Every event crossed process boundary with serialization + socket I/O
3. **Harder Debugging**: Two processes, distributed logs, complex race conditions
4. **Use Case Mismatch**: Primary use case (Claude Code + open NodeSpace) doesn't require MCP to survive desktop close
5. **Duplicate Infrastructure**: EventBus logic needed in both processes

**Chosen Alternative:**

Single-process architecture with MCP as Tokio task within Tauri (see ADR-021). This provides:
- Same-process communication (zero overhead)
- Automatic reactivity via existing EventBus
- Simpler implementation and debugging
- Better performance

### Original IPC Implementation Proposal

```rust
// Desktop app: IPC server + event receiver
fn setup_desktop_ipc() -> Result<UnixListener, io::Error> {
    let socket_path = "/tmp/nodespace-events.sock";
    let _ = std::fs::remove_file(socket_path);
    let listener = UnixListener::bind(socket_path)?;

    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await?;
            tokio::spawn(handle_ipc_client(stream));
        }
    });

    Ok(listener)
}

// MCP server: IPC client + event sender
async fn setup_mcp_ipc() -> Result<UnixStream, io::Error> {
    let socket_path = "/tmp/nodespace-events.sock";
    let stream = UnixStream::connect(socket_path).await?;
    Ok(stream)
}
```

This approach would have required Issue #111 (reactive state synchronization) to implement the IPC coordination layer. With single-process architecture, Issue #111 is unnecessary.

---

**Reference:** See ADR-021 for the adopted single-process architecture.
