import { existsSync, mkdirSync, copyFileSync, rmSync, readdirSync } from 'node:fs';
import { join, dirname, basename } from 'node:path';
import { fileURLToPath } from 'node:url';
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

export function install(targetAgents?: AgentName[], packageRoot = PACKAGE_ROOT): InstallResult[] {
  const detected = targetAgents ?? detectAgents();
  const results: InstallResult[] = [];

  for (const agentName of detected) {
    const config = AGENTS.find(a => a.name === agentName);
    if (!config) continue;

    const installed: string[] = [];
    for (const shim of config.shims) {
      const src = join(packageRoot, shim);
      const dest = join(config.installDir, basename(shim));
      if (existsSync(src)) {
        mkdirSync(config.installDir, { recursive: true });
        copyFileSync(src, dest);
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
