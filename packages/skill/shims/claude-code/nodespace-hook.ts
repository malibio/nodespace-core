/**
 * Claude Code hooks shim — registers NodeSpace knowledge graph tools.
 *
 * Claude Code discovers this file via the `CLAUDE.md` directive written by
 * GraphContextAssembler. The `hook()` function is part of Claude Code's
 * extension runtime and is available as a global in hook scripts.
 */

import { searchSemantic, getNode, createNode, updateNode, getChildren, ToolError }
  from '@nodespace/agent-tools';

declare function hook(
  name: string,
  handler: (args: Record<string, unknown>) => Promise<string>
): void;

hook('nodespace_search_semantic', async ({ query, limit }) => {
  try {
    const result = await searchSemantic(
      String(query),
      typeof limit === 'number' ? limit : undefined
    );
    return JSON.stringify(result);
  } catch (err) {
    if (err instanceof ToolError) {
      throw new Error(`[${err.code}] ${err.message}`);
    }
    throw err;
  }
});

hook('nodespace_get_node', async ({ node_id }) => {
  try {
    const result = await getNode(String(node_id));
    return JSON.stringify(result);
  } catch (err) {
    if (err instanceof ToolError) {
      throw new Error(`[${err.code}] ${err.message}`);
    }
    throw err;
  }
});

hook('nodespace_create_node', async ({ type, content, parent_id }) => {
  try {
    const result = await createNode(
      String(type),
      String(content),
      parent_id !== undefined ? String(parent_id) : undefined
    );
    return JSON.stringify(result);
  } catch (err) {
    if (err instanceof ToolError) {
      throw new Error(`[${err.code}] ${err.message}`);
    }
    throw err;
  }
});

hook('nodespace_update_node', async ({ node_id, content }) => {
  try {
    const result = await updateNode(String(node_id), String(content));
    return JSON.stringify(result);
  } catch (err) {
    if (err instanceof ToolError) {
      throw new Error(`[${err.code}] ${err.message}`);
    }
    throw err;
  }
});

hook('nodespace_get_children', async ({ node_id }) => {
  try {
    const result = await getChildren(String(node_id));
    return JSON.stringify(result);
  } catch (err) {
    if (err instanceof ToolError) {
      throw new Error(`[${err.code}] ${err.message}`);
    }
    throw err;
  }
});
