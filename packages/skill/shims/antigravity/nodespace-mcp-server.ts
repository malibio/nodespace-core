/**
 * Antigravity CLI MCP server shim — exposes NodeSpace knowledge graph tools
 * over the Model Context Protocol.
 *
 * Antigravity CLI (`agy`) does not have a bespoke plugin tool-registration
 * runtime the way Claude Code (`hook()`), OpenCode (`plugin.registerTool()`),
 * or the old Gemini CLI (stdin/stdout JSON dispatch) do. Its native
 * tool-registration path is MCP: a plugin ships an `mcp_config.json`
 * pointing at a server command, and `agy` speaks JSON-RPC 2.0 to it over
 * stdio. This file *is* that server — a minimal, dependency-free
 * implementation of the `initialize` / `tools/list` / `tools/call` subset of
 * the MCP spec, newline-delimited JSON-RPC over stdin/stdout.
 *
 * GraphContextAssembler writes this file plus `nodespace-mcp-config.json`
 * into the session temp dir and points Antigravity's plugin `mcp_config.json`
 * at it before spawning `agy`.
 */

// runCLI and NodespaceCLIError are intentionally inlined (not imported) in each
// shim. Shims are copied as standalone scripts into agent session temp dirs with
// no npm context, so module resolution is unavailable at runtime. Any change to
// runCLI must be replicated across all four shims in packages/skill/shims/.
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { createInterface } from 'node:readline';

const execFileAsync = promisify(execFile);

class NodespaceCLIError extends Error {
  constructor(
    message: string,
    public readonly exitCode: number | null,
    public readonly stderr: string
  ) {
    super(message);
    this.name = 'NodespaceCLIError';
  }
}

async function runCLI(args: string[]): Promise<string> {
  try {
    const { stdout } = await execFileAsync('nodespace', ['--json', ...args], {
      env: process.env,
      timeout: 30_000
    });
    return stdout.trim();
  } catch (err: unknown) {
    if (
      err !== null &&
      typeof err === 'object' &&
      'code' in err &&
      (err as NodeJS.ErrnoException).code === 'ENOENT'
    ) {
      throw new NodespaceCLIError(
        'nodespace CLI not found on $PATH. Install NodeSpace and ensure the nodespace binary is accessible.',
        null,
        ''
      );
    }
    if (err !== null && typeof err === 'object' && 'stderr' in err && 'code' in err) {
      const e = err as { stderr: string; code: number | null; message: string };
      throw new NodespaceCLIError(e.message, e.code, e.stderr);
    }
    throw err;
  }
}

interface ToolDef {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
  execute: (args: Record<string, unknown>) => Promise<string>;
}

const TOOLS: ToolDef[] = [
  {
    name: 'nodespace_search_semantic',
    description: 'Search the NodeSpace knowledge graph using natural language.',
    inputSchema: {
      type: 'object',
      properties: {
        query: { type: 'string', description: 'Natural language search query.' },
        limit: { type: 'number', description: 'Maximum number of results (default 10).' }
      },
      required: ['query']
    },
    execute: async ({ query, limit }) => {
      const args = ['search', String(query)];
      if (typeof limit === 'number') args.push('--limit', String(limit));
      return runCLI(args);
    }
  },
  {
    name: 'nodespace_get_node',
    description: 'Fetch a single NodeSpace node by its ID.',
    inputSchema: {
      type: 'object',
      properties: {
        node_id: { type: 'string', description: 'ID of the node to fetch.' }
      },
      required: ['node_id']
    },
    execute: async ({ node_id }) => runCLI(['node', 'get', String(node_id)])
  },
  {
    name: 'nodespace_create_node',
    description: 'Create a new node in the NodeSpace knowledge graph.',
    inputSchema: {
      type: 'object',
      properties: {
        type: { type: 'string', description: 'Node type (e.g. "text", "task").' },
        content: { type: 'string', description: 'Markdown content of the node.' },
        parent_id: { type: 'string', description: 'Parent node ID (optional).' }
      },
      required: ['type', 'content']
    },
    execute: async ({ type, content, parent_id }) => {
      const args = ['node', 'create', '--type', String(type), '--content', String(content)];
      if (parent_id !== undefined) args.push('--parent', String(parent_id));
      return runCLI(args);
    }
  },
  {
    name: 'nodespace_update_node',
    description: 'Update the content of an existing NodeSpace node.',
    inputSchema: {
      type: 'object',
      properties: {
        node_id: { type: 'string', description: 'ID of the node to update.' },
        content: { type: 'string', description: 'New markdown content.' }
      },
      required: ['node_id', 'content']
    },
    execute: async ({ node_id, content }) =>
      runCLI(['node', 'update', String(node_id), '--content', String(content)])
  },
  {
    name: 'nodespace_get_children',
    description: 'List the direct children of a NodeSpace node.',
    inputSchema: {
      type: 'object',
      properties: {
        node_id: { type: 'string', description: 'ID of the parent node.' }
      },
      required: ['node_id']
    },
    execute: async ({ node_id }) => runCLI(['node', 'children', String(node_id)])
  }
];

interface JsonRpcRequest {
  jsonrpc: '2.0';
  id?: string | number | null;
  method: string;
  params?: Record<string, unknown>;
}

function writeResponse(id: string | number | null | undefined, result: unknown): void {
  process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id: id ?? null, result }) + '\n');
}

function writeError(id: string | number | null | undefined, code: number, message: string): void {
  process.stdout.write(
    JSON.stringify({ jsonrpc: '2.0', id: id ?? null, error: { code, message } }) + '\n'
  );
}

async function handleRequest(req: JsonRpcRequest): Promise<void> {
  switch (req.method) {
    case 'initialize':
      writeResponse(req.id, {
        protocolVersion: '2025-06-18',
        capabilities: { tools: {} },
        serverInfo: { name: 'nodespace', version: '1.0.0' }
      });
      return;
    case 'notifications/initialized':
      // No response for notifications (no `id`).
      return;
    case 'tools/list':
      writeResponse(req.id, {
        tools: TOOLS.map(({ name, description, inputSchema }) => ({
          name,
          description,
          inputSchema
        }))
      });
      return;
    case 'tools/call': {
      const params = req.params ?? {};
      const toolName = String(params.name);
      const toolArgs = (params.arguments as Record<string, unknown>) ?? {};
      const tool = TOOLS.find((t) => t.name === toolName);
      if (!tool) {
        writeError(req.id, -32602, `Unknown tool: ${toolName}`);
        return;
      }
      try {
        const text = await tool.execute(toolArgs);
        writeResponse(req.id, { content: [{ type: 'text', text }], isError: false });
      } catch (err) {
        const message =
          err instanceof NodespaceCLIError ? `[CLI_ERROR] ${err.message}` : String(err);
        writeResponse(req.id, { content: [{ type: 'text', text: message }], isError: true });
      }
      return;
    }
    default:
      if (req.id !== undefined) {
        writeError(req.id, -32601, `Method not found: ${req.method}`);
      }
  }
}

function main(): void {
  const rl = createInterface({ input: process.stdin, terminal: false });
  rl.on('line', (line) => {
    const trimmed = line.trim();
    if (!trimmed) return;
    let req: JsonRpcRequest;
    try {
      req = JSON.parse(trimmed) as JsonRpcRequest;
    } catch {
      writeError(null, -32700, 'Parse error');
      return;
    }
    void handleRequest(req);
  });
}

main();
