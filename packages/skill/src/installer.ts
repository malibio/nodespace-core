import { existsSync, mkdirSync, copyFileSync, rmSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, dirname, basename } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { AGENTS } from './agents.js';
import type { AgentName, InstallResult, UninstallResult } from './types.js';

const __dirname = dirname(fileURLToPath(import.meta.url));
// Walk up past dist/ if running from compiled output; src/ stays at package root.
const PACKAGE_ROOT = join(__dirname, '..');

function detectAgents(): AgentName[] {
  return AGENTS
    .filter(agent => existsSync(agent.detectionDir))
    .map(agent => agent.name);
}

/**
 * Check whether `nodespace` resolves on $PATH by running `nodespace --version`.
 * Returns true if the binary is found and exits 0, false otherwise.
 * Safe to call without a running daemon — `--version` is handled by clap
 * before any socket connection is attempted.
 */
export function isNodespaceBinaryOnPath(): boolean {
  try {
    execFileSync('nodespace', ['--version'], { stdio: 'ignore', timeout: 3000 });
    return true;
  } catch {
    return false;
  }
}

export function install(targetAgents?: AgentName[], packageRoot = PACKAGE_ROOT): InstallResult[] {
  if (!isNodespaceBinaryOnPath()) {
    process.stderr.write(
      'WARNING: `nodespace` is not on $PATH. The skill will be installed, but the CLI\n' +
      'must be installed and on $PATH before agents can use NodeSpace.\n' +
      'Install it with `curl -fsSL https://nodespace.ai/install.sh | sh`, via the\n' +
      'NodeSpace DMG, or `brew install --cask nodespaceai/nodespace/nodespace`.\n',
    );
  }

  const detected = targetAgents ?? detectAgents();
  const results: InstallResult[] = [];

  for (const agentName of detected) {
    const config = AGENTS.find(a => a.name === agentName);
    if (!config) continue;

    const installed: string[] = [];
    for (const shim of config.shims) {
      const src = join(packageRoot, shim);
      // Shim paths flatten to a basename (`shims/codex/x.ts` installs as
      // `x.ts`), but `references/` must keep its directory: SKILL.md links to
      // `references/cli.md` by relative path, so flattening it would leave the
      // body pointing at a file that isn't where it says.
      const relative = shim.startsWith('references/') ? shim : basename(shim);
      const dest = join(config.installDir, relative);
      if (existsSync(src)) {
        mkdirSync(dirname(dest), { recursive: true });
        // SKILL.md gets the agent's frontmatter prepended — a skill is
        // discovered by its YAML `name` + `description` under the Agent Skills
        // standard; everything else is copied verbatim.
        if (config.skillFrontmatter && basename(shim) === 'SKILL.md') {
          writeFileSync(dest, config.skillFrontmatter + '\n' + readFileSync(src, 'utf8'), 'utf8');
        } else {
          copyFileSync(src, dest);
        }
        installed.push(dest);
      }
    }

    results.push({ agent: agentName, installed });
  }

  return results;
}

export function uninstall(targetAgents?: AgentName[]): UninstallResult[] {
  const agents = targetAgents ?? AGENTS.map(a => a.name);
  const results: UninstallResult[] = [];

  for (const agentName of agents) {
    const config = AGENTS.find(a => a.name === agentName);
    if (!config || !existsSync(config.installDir)) continue;

    const removed: string[] = [];
    for (const shim of config.shims) {
      const dest = join(config.installDir, basename(shim));
      if (existsSync(dest)) {
        rmSync(dest);
        removed.push(dest);
      }
    }

    const remaining = readdirSync(config.installDir);
    if (remaining.length === 0) {
      rmSync(config.installDir, { recursive: true });
    }

    results.push({ agent: agentName, removed });
  }

  return results;
}
