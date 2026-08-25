#!/usr/bin/env node
// Invoked directly via `bun <path-to-this-file> <command> [agent]` — the
// desktop app's bundled installer runs it this way (see
// packages/desktop-app/src-tauri/src/skill_setup.rs), and it's the same
// invocation for a manual run from a source checkout. Never `npx`/`npm`:
// `@nodespaceai/skill` is not published to npm (the public
// NodeSpaceAI/nodespace-skill repo is the distribution channel for external
// harnesses that want the skill outside the app — see packages/skill/README.md).
import { install, uninstall } from './installer.js';
import type { AgentName } from './types.js';
import { AGENTS } from './agents.js';

const command = process.argv[2];
const agentArg = process.argv[3] as AgentName | undefined;

const validAgents = AGENTS.map(a => a.name);

function isValidAgent(name: string): name is AgentName {
  return validAgents.includes(name as AgentName);
}

function printUsage(): void {
  console.log(`Usage: bun install.js <command> [agent]

Commands:
  install [agent]    Install NodeSpace skill for detected (or specified) agents
  uninstall [agent]  Remove NodeSpace skill from detected (or specified) agents

Agents: ${validAgents.join(', ')}

Examples:
  bun install.js install
  bun install.js install claude-code
  bun install.js uninstall`);
}

if (!command || command === '--help' || command === '-h') {
  printUsage();
  process.exit(0);
}

if (agentArg && !isValidAgent(agentArg)) {
  console.error(`Unknown agent: ${agentArg}`);
  console.error(`Valid agents: ${validAgents.join(', ')}`);
  process.exit(1);
}

const targetAgents = agentArg ? [agentArg] : undefined;

if (command === 'install') {
  const results = install(targetAgents);

  if (results.length === 0) {
    console.log('No supported agents detected.');
    console.log(`Checked: ${validAgents.join(', ')}`);
    console.log('To install manually, specify an agent: bun install.js install <agent>');
    process.exit(0);
  }

  let hadPartialFailure = false;
  for (const result of results) {
    if (result.installed.length > 0) {
      console.log(`✓ ${result.agent}: installed ${result.installed.length} file(s)`);
      for (const file of result.installed) {
        console.log(`  → ${file}`);
      }
    } else {
      console.error(`⚠ ${result.agent}: detected but no files to install (package may be incomplete)`);
      hadPartialFailure = true;
    }
  }
  if (hadPartialFailure) {
    process.exit(1);
  }
} else if (command === 'uninstall') {
  const results = uninstall(targetAgents);

  if (results.length === 0) {
    console.log('No installed NodeSpace skills found.');
    process.exit(0);
  }

  for (const result of results) {
    if (result.removed.length > 0) {
      console.log(`✓ ${result.agent}: removed ${result.removed.length} file(s)`);
    } else {
      console.log(`  ${result.agent}: nothing to remove`);
    }
  }
} else {
  console.error(`Unknown command: ${command}`);
  printUsage();
  process.exit(1);
}
