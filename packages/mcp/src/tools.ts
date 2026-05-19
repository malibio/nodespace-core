import { execFile } from 'node:child_process';
import { promisify } from 'node:util';

const execFileAsync = promisify(execFile);

export class NodespaceCLIError extends Error {
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

export async function createNode(
  nodeType: string,
  content: string,
  parentId?: string
): Promise<string> {
  const args = ['node', 'create', '--type', nodeType, '--content', content];
  if (parentId) args.push('--parent', parentId);
  return runCLI(args);
}

export async function getNode(nodeId: string): Promise<string> {
  return runCLI(['node', 'get', nodeId]);
}

export async function searchNodes(query: string, limit?: number): Promise<string> {
  const args = ['search', query];
  if (limit !== undefined) args.push('--limit', String(limit));
  return runCLI(args);
}

export async function listNodes(nodeType: string, limit?: number): Promise<string> {
  const args = ['search', '--type', nodeType, '--limit', String(limit ?? 50)];
  return runCLI(args);
}

export async function updateNode(nodeId: string, content: string): Promise<string> {
  return runCLI(['node', 'update', nodeId, '--content', content]);
}

export async function deleteNode(nodeId: string): Promise<string> {
  return runCLI(['node', 'delete', nodeId]);
}
