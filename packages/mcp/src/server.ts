#!/usr/bin/env node
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { z } from 'zod';

import {
  NodespaceCLIError,
  createNode,
  deleteNode,
  getNode,
  listNodes,
  searchNodes,
  updateNode
} from './tools.js';

const args = process.argv.slice(2);
if (args[0] === 'install' || args[0] === 'uninstall') {
  process.argv = [process.argv[0], process.argv[1], args[0]];
  await import('./install.js');
  process.exit(0);
}

const server = new McpServer({
  name: 'nodespace',
  version: '0.1.0'
});

function textContent(text: string) {
  return { content: [{ type: 'text' as const, text }] };
}

function errorContent(err: unknown) {
  const message =
    err instanceof NodespaceCLIError
      ? err.message + (err.stderr ? `\nstderr: ${err.stderr}` : '')
      : String(err);
  return { content: [{ type: 'text' as const, text: `Error: ${message}` }], isError: true };
}

server.registerTool(
  'nodespace_create_node',
  {
    description: 'Create a new node in the NodeSpace knowledge graph with the given type and content.',
    inputSchema: {
      node_type: z.string().describe('Node type, e.g. text, task, date'),
      content: z.string().describe('Content of the node (plain text or markdown)'),
      parent_id: z.string().optional().describe('Parent node ID — omit to create a root node')
    }
  },
  async ({ node_type, content, parent_id }) => {
    try {
      return textContent(await createNode(node_type, content, parent_id));
    } catch (err) {
      return errorContent(err);
    }
  }
);

server.registerTool(
  'nodespace_get_node',
  {
    description: 'Retrieve a node from the NodeSpace knowledge graph by its ID.',
    inputSchema: { node_id: z.string().describe('Node UUID') }
  },
  async ({ node_id }) => {
    try {
      return textContent(await getNode(node_id));
    } catch (err) {
      return errorContent(err);
    }
  }
);

server.registerTool(
  'nodespace_search',
  {
    description: 'Semantic search across the NodeSpace knowledge graph.',
    inputSchema: {
      query: z.string().describe('Free-text search query'),
      limit: z.number().int().positive().optional().describe('Maximum number of results (default 10)')
    }
  },
  async ({ query, limit }) => {
    try {
      return textContent(await searchNodes(query, limit));
    } catch (err) {
      return errorContent(err);
    }
  }
);

server.registerTool(
  'nodespace_list_nodes',
  {
    description: 'List nodes of a given type from the NodeSpace knowledge graph.',
    inputSchema: {
      node_type: z.string().describe('Node type to list, e.g. task, date, text'),
      limit: z.number().int().positive().optional().describe('Maximum number of results (default 50)')
    }
  },
  async ({ node_type, limit }) => {
    try {
      return textContent(await listNodes(node_type, limit));
    } catch (err) {
      return errorContent(err);
    }
  }
);

server.registerTool(
  'nodespace_update_node',
  {
    description: 'Update the content of an existing node in the NodeSpace knowledge graph.',
    inputSchema: {
      node_id: z.string().describe('Node UUID'),
      content: z.string().describe('New content for the node')
    }
  },
  async ({ node_id, content }) => {
    try {
      return textContent(await updateNode(node_id, content));
    } catch (err) {
      return errorContent(err);
    }
  }
);

server.registerTool(
  'nodespace_delete_node',
  {
    description: 'Delete a node from the NodeSpace knowledge graph by its ID.',
    inputSchema: { node_id: z.string().describe('Node UUID to delete') }
  },
  async ({ node_id }) => {
    try {
      return textContent(await deleteNode(node_id));
    } catch (err) {
      return errorContent(err);
    }
  }
);

const transport = new StdioServerTransport();
await server.connect(transport);
