/**
 * Codex plugin shim — registers NodeSpace knowledge graph tools.
 *
 * Codex discovers plugins via its plugin directory. GraphContextAssembler
 * writes this file into the session temp dir and sets CODEX_PLUGIN_DIR so
 * Codex picks it up at startup.
 *
 * The `definePlugin` / `registerTool` globals are part of Codex's plugin
 * runtime and are available in all plugin scripts.
 */

import { searchSemantic, getNode, createNode, updateNode, getChildren, ToolError }
  from '@nodespace/agent-tools';

declare function definePlugin(spec: {
  name: string;
  version: string;
  tools: Array<{
    name: string;
    description: string;
    parameters: Record<string, unknown>;
    handler: (args: Record<string, unknown>) => Promise<unknown>;
  }>;
}): void;

definePlugin({
  name: 'nodespace',
  version: '0.1.0',
  tools: [
    {
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
      handler: async ({ query, limit }) => {
        try {
          return await searchSemantic(
            String(query),
            typeof limit === 'number' ? limit : undefined
          );
        } catch (err) {
          if (err instanceof ToolError) throw new Error(`[${err.code}] ${err.message}`);
          throw err;
        }
      }
    },
    {
      name: 'nodespace_get_node',
      description: 'Fetch a single NodeSpace node by its ID.',
      parameters: {
        type: 'object',
        properties: {
          node_id: { type: 'string', description: 'ID of the node to fetch.' }
        },
        required: ['node_id']
      },
      handler: async ({ node_id }) => {
        try {
          return await getNode(String(node_id));
        } catch (err) {
          if (err instanceof ToolError) throw new Error(`[${err.code}] ${err.message}`);
          throw err;
        }
      }
    },
    {
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
      handler: async ({ type, content, parent_id }) => {
        try {
          return await createNode(
            String(type),
            String(content),
            parent_id !== undefined ? String(parent_id) : undefined
          );
        } catch (err) {
          if (err instanceof ToolError) throw new Error(`[${err.code}] ${err.message}`);
          throw err;
        }
      }
    },
    {
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
      handler: async ({ node_id, content }) => {
        try {
          return await updateNode(String(node_id), String(content));
        } catch (err) {
          if (err instanceof ToolError) throw new Error(`[${err.code}] ${err.message}`);
          throw err;
        }
      }
    },
    {
      name: 'nodespace_get_children',
      description: 'List the direct children of a NodeSpace node.',
      parameters: {
        type: 'object',
        properties: {
          node_id: { type: 'string', description: 'ID of the parent node.' }
        },
        required: ['node_id']
      },
      handler: async ({ node_id }) => {
        try {
          return await getChildren(String(node_id));
        } catch (err) {
          if (err instanceof ToolError) throw new Error(`[${err.code}] ${err.message}`);
          throw err;
        }
      }
    }
  ]
});
