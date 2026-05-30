/**
 * OpenCode plugin shim — registers NodeSpace knowledge graph tools.
 *
 * OpenCode discovers plugins via its plugin directory. GraphContextAssembler
 * writes this file into the session temp dir and sets OPENCODE_PLUGIN_DIR so
 * OpenCode picks it up at startup.
 *
 * The `plugin` global is part of OpenCode's plugin runtime and is available
 * in all plugin scripts.
 */

// runCLI and NodespaceCLIError are intentionally inlined (not imported) in each
// shim. Shims are copied as standalone scripts into agent session temp dirs with
// no npm context, so module resolution is unavailable at runtime. Any change to
// runCLI must be replicated across all four shims in packages/skill/shims/.
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';

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

declare const plugin: {
  registerTool(spec: {
    name: string;
    description: string;
    parameters: Record<string, unknown>;
    execute: (args: Record<string, unknown>) => Promise<unknown>;
  }): void;
};

plugin.registerTool({
  name: 'nodespace_search_semantic',
  description: 'Search the NodeSpace knowledge graph using natural language.',
  parameters: {
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
});

plugin.registerTool({
  name: 'nodespace_get_node',
  description: 'Fetch a single NodeSpace node by its ID.',
  parameters: {
    type: 'object',
    properties: {
      node_id: { type: 'string', description: 'ID of the node to fetch.' }
    },
    required: ['node_id']
  },
  execute: async ({ node_id }) => {
    return runCLI(['node', 'get', String(node_id)]);
  }
});

plugin.registerTool({
  name: 'nodespace_create_node',
  description: 'Create a new node in the NodeSpace knowledge graph.',
  parameters: {
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
});

plugin.registerTool({
  name: 'nodespace_update_node',
  description: 'Update the content of an existing NodeSpace node.',
  parameters: {
    type: 'object',
    properties: {
      node_id: { type: 'string', description: 'ID of the node to update.' },
      content: { type: 'string', description: 'New markdown content.' }
    },
    required: ['node_id', 'content']
  },
  execute: async ({ node_id, content }) => {
    return runCLI(['node', 'update', String(node_id), '--content', String(content)]);
  }
});

plugin.registerTool({
  name: 'nodespace_get_children',
  description: 'List the direct children of a NodeSpace node.',
  parameters: {
    type: 'object',
    properties: {
      node_id: { type: 'string', description: 'ID of the parent node.' }
    },
    required: ['node_id']
  },
  execute: async ({ node_id }) => {
    return runCLI(['node', 'children', String(node_id)]);
  }
});
