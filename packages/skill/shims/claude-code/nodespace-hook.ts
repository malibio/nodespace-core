/**
 * Claude Code hooks shim — registers NodeSpace knowledge graph tools.
 *
 * Claude Code discovers this file via the `CLAUDE.md` directive written by
 * GraphContextAssembler. The `hook()` function is part of Claude Code's
 * extension runtime and is available as a global in hook scripts.
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

declare function hook(
  name: string,
  handler: (args: Record<string, unknown>) => Promise<string>
): void;

hook('nodespace_search_semantic', async ({ query, limit }) => {
  const args = ['search', String(query)];
  if (typeof limit === 'number') args.push('--limit', String(limit));
  return runCLI(args);
});

hook('nodespace_get_node', async ({ node_id }) => {
  return runCLI(['node', 'get', String(node_id)]);
});

hook('nodespace_create_node', async ({ type, content, parent_id }) => {
  const args = ['node', 'create', '--type', String(type), '--content', String(content)];
  if (parent_id !== undefined) args.push('--parent', String(parent_id));
  return runCLI(args);
});

hook('nodespace_update_node', async ({ node_id, content }) => {
  return runCLI(['node', 'update', String(node_id), '--content', String(content)]);
});

hook('nodespace_get_children', async ({ node_id }) => {
  return runCLI(['node', 'children', String(node_id)]);
});
