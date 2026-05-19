# @nodespaceai/mcp

MCP server exposing [NodeSpace](https://nodespace.ai) knowledge graph operations as AI tools via the `nodespace` CLI.

Enables Claude desktop, Cline, Cursor, and any other MCP-compatible agent to read and write your NodeSpace knowledge graph.

## Requirements

- `nodespace` CLI on your `$PATH` (installed with the NodeSpace desktop app)
- Node.js 18+

## Quick start

### Automatic (recommended)

```bash
npx @nodespaceai/mcp install
```

This writes the correct entry to `~/Library/Application Support/Claude/claude_desktop_config.json` and merges safely with any existing config. Restart Claude desktop to activate.

### Manual

Add to your `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "nodespace": {
      "command": "npx",
      "args": ["@nodespaceai/mcp"]
    }
  }
}
```

## Uninstall

```bash
npx @nodespaceai/mcp uninstall
```

## Tools

| Tool | Description |
|------|-------------|
| `nodespace_create_node` | Create a node with a given type and content |
| `nodespace_get_node` | Retrieve a node by ID |
| `nodespace_search` | Semantic search across the knowledge graph |
| `nodespace_list_nodes` | List nodes by type |
| `nodespace_update_node` | Update node content |
| `nodespace_delete_node` | Delete a node |

## How it works

Each MCP tool call invokes `nodespace --json <subcommand>` and returns the JSON output as the tool result. No gRPC dependency — the MCP server is a thin stdio wrapper around the `nodespace` CLI.

## License

MIT
