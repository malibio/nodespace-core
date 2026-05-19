import { homedir } from 'node:os';
import { join } from 'node:path';
import type { AgentConfig } from './types.js';

const home = homedir();

export const AGENTS: AgentConfig[] = [
  {
    name: 'claude-code',
    detectionDir: join(home, '.claude'),
    installDir: join(home, '.claude', 'skills', 'nodespace'),
    shims: ['SKILL.md', 'shims/claude-code/nodespace-hook.ts'],
  },
  {
    name: 'codex',
    detectionDir: join(home, '.codex'),
    installDir: join(home, '.codex', 'skills', 'nodespace'),
    shims: ['SKILL.md', 'shims/codex/nodespace-plugin.ts'],
  },
  {
    name: 'gemini',
    detectionDir: join(home, '.gemini'),
    installDir: join(home, '.gemini', 'skills', 'nodespace'),
    shims: ['SKILL.md', 'shims/gemini/nodespace-handler.ts', 'shims/gemini/nodespace-tools.json'],
  },
  {
    name: 'opencode',
    detectionDir: join(home, '.opencode'),
    installDir: join(home, '.opencode', 'skills', 'nodespace'),
    shims: ['SKILL.md', 'shims/opencode/nodespace-plugin.ts'],
  },
];
