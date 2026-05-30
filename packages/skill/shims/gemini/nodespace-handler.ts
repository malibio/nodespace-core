/**
 * Gemini CLI tool handler — dispatches NodeSpace tool calls by name.
 *
 * Gemini CLI reads tool definitions from `nodespace-tools.json` and invokes
 * the handler script listed there for each tool call. The handler receives
 * `{ name, args }` on stdin as JSON and must write the result JSON to stdout.
 *
 * GraphContextAssembler writes both files into the session temp dir and sets
 * GEMINI_TOOLS_DIR to that directory before spawning Gemini CLI.
 *
 * Protocol (Gemini stdio contract):
 *   stdin:  `{ "name": "<tool>", "args": { ... } }`
 *   stdout (success): `{ "result": <value> }`
 *   stdout (error):   `{ "error": "<message>" }` + exit code 1
 */

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

interface ToolCall {
  name: string;
  args: Record<string, unknown>;
}

async function dispatch(call: ToolCall): Promise<unknown> {
  const { name, args } = call;
  switch (name) {
    case 'nodespace_search_semantic': {
      const cliArgs = ['search', String(args.query)];
      if (typeof args.limit === 'number') cliArgs.push('--limit', String(args.limit));
      return runCLI(cliArgs);
    }
    case 'nodespace_get_node':
      return runCLI(['node', 'get', String(args.node_id)]);
    case 'nodespace_create_node': {
      const cliArgs = ['node', 'create', '--type', String(args.type), '--content', String(args.content)];
      if (args.parent_id !== undefined) cliArgs.push('--parent', String(args.parent_id));
      return runCLI(cliArgs);
    }
    case 'nodespace_update_node':
      return runCLI(['node', 'update', String(args.node_id), '--content', String(args.content)]);
    case 'nodespace_get_children':
      return runCLI(['node', 'children', String(args.node_id)]);
    default:
      throw new NodespaceCLIError(`Unknown tool: ${name}`, null, '');
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
    const message = err instanceof NodespaceCLIError
      ? `[CLI_ERROR] ${err.message}`
      : String(err);
    process.stdout.write(JSON.stringify({ error: message }));
    process.exit(1);
  }
}

main();
