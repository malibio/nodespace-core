import { homedir } from 'node:os';
import { join } from 'node:path';
import type { AgentConfig } from './types.js';

const home = homedir();

// Claude Code reads its config from $CLAUDE_CONFIG_DIR when set (e.g. a separate
// `claude-ns` profile), falling back to ~/.claude. Detect + install into the
// active profile rather than always ~/.claude so the skill lands where the
// running Claude Code will actually look for it.
const claudeConfigDir = process.env.CLAUDE_CONFIG_DIR?.trim() || join(home, '.claude');

// Claude Code surfaces a skill via its YAML frontmatter (name + description).
// The shared SKILL.md body is agent-agnostic and carries none, so the installer
// prepends this Claude-Code-specific frontmatter when writing the skill.
const CLAUDE_CODE_SKILL_FRONTMATTER = `---
name: nodespace
description: >
  Read and write the NodeSpace knowledge graph — a local-first, synced working
  memory that persists across sessions and can be shared with a teammate's agent
  sessions. Use whenever the user wants to remember something for later, recall
  what was decided or discovered earlier, save notes/tasks/findings/decisions,
  search prior context at the start of a task, hand off context to a teammate, or
  asks to "check nodespace" / "what did we save". Drives the \`nodespace\` CLI
  (create/get/update/query/search/import).
allowed-tools: Bash(nodespace:*)
---
`;

export const AGENTS: AgentConfig[] = [
  {
    name: 'claude-code',
    detectionDir: claudeConfigDir,
    installDir: join(claudeConfigDir, 'skills', 'nodespace'),
    shims: ['SKILL.md', 'shims/claude-code/nodespace-hook.ts'],
    skillFrontmatter: CLAUDE_CODE_SKILL_FRONTMATTER,
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
