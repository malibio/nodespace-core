/**
 * Gemini CLI tool handler — dispatches NodeSpace tool calls by name.
 *
 * Gemini CLI reads tool definitions from `nodespace-tools.json` and invokes
 * the handler script listed there for each tool call. The handler receives
 * `{ name, args }` on stdin as JSON and must write the result JSON to stdout.
 *
 * GraphContextAssembler writes both files into the session temp dir and sets
 * GEMINI_TOOLS_DIR to that directory before spawning Gemini CLI.
 */

import { searchSemantic, getNode, createNode, updateNode, getChildren, ToolError }
  from '@nodespace/agent-tools';

interface ToolCall {
  name: string;
  args: Record<string, unknown>;
}

async function dispatch(call: ToolCall): Promise<unknown> {
  const { name, args } = call;
  switch (name) {
    case 'nodespace_search_semantic':
      return searchSemantic(
        String(args.query),
        typeof args.limit === 'number' ? args.limit : undefined
      );
    case 'nodespace_get_node':
      return getNode(String(args.node_id));
    case 'nodespace_create_node':
      return createNode(
        String(args.type),
        String(args.content),
        args.parent_id !== undefined ? String(args.parent_id) : undefined
      );
    case 'nodespace_update_node':
      return updateNode(String(args.node_id), String(args.content));
    case 'nodespace_get_children':
      return getChildren(String(args.node_id));
    default:
      throw new ToolError('UNKNOWN_TOOL', `Unknown tool: ${name}`);
  }
}

async function main(): Promise<void> {
  const chunks: Buffer[] = [];
  for await (const chunk of process.stdin) {
    chunks.push(chunk as Buffer);
  }
  const call = JSON.parse(Buffer.concat(chunks).toString('utf-8')) as ToolCall;

  try {
    const result = await dispatch(call);
    process.stdout.write(JSON.stringify({ result }));
  } catch (err) {
    const message = err instanceof ToolError
      ? `[${err.code}] ${err.message}`
      : String(err);
    process.stdout.write(JSON.stringify({ error: message }));
    process.exit(1);
  }
}

main();
