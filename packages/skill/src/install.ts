#!/usr/bin/env node
// Invoked directly via `bun <path-to-this-file> <command> [agent]` — the
// desktop app's bundled installer runs it this way (see
// packages/desktop-app/src-tauri/src/skill_setup.rs), and it's the same
// invocation for a manual run from a source checkout. Never `npx`/`npm`:
// `@nodespaceai/skill` is not published to npm (the public
// NodeSpaceAI/nodespace-skill repo is the distribution channel for external
// harnesses that want the skill outside the app — see packages/skill/README.md).
//
// Also compiled directly (`bun build --compile`) into a standalone binary
// with no bun/node dependency at all — see the desktop app's
// `resolve_installer_path`/`run_skill_installer` in skill_setup.rs. `main()`
// stays behind `import.meta.main` so this module is safely importable for
// unit tests (of `extractResourceRoot`) without triggering the CLI's own
// argv parsing and `process.exit` calls as a side effect of the import.
import { install, uninstall } from './installer.js';
import type { AgentName } from './types.js';
import { AGENTS } from './agents.js';

/**
 * Pull `--resource-root <path>` out of argv, wherever it appears, and return
 * the remaining positional args separately. Needed for the compiled
 * standalone-binary distribution (see `installer.ts`'s `PACKAGE_ROOT` doc
 * comment): a `bun build --compile` executable has no source-relative
 * sibling directory containing SKILL.md/shims/references the way a plain
 * `dist/install.js` does, so the caller (skill_setup.rs, for the compiled
 * binary; a person, for a manual run) must say where those files actually
 * are. Absent entirely for the existing `bun`/`node` + dist/install.js
 * invocation, which keeps resolving PACKAGE_ROOT from its own file location
 * exactly as before.
 */
export function extractResourceRoot(argv: string[]): { rest: string[]; resourceRoot?: string } {
  const rest: string[] = [];
  let resourceRoot: string | undefined;
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === '--resource-root') {
      resourceRoot = argv[++i];
    } else {
      rest.push(argv[i]);
    }
  }
  return { rest, resourceRoot };
}

function main(): void {
  const { rest: positional, resourceRoot } = extractResourceRoot(process.argv.slice(2));
  const command = positional[0];
  const agentArg = positional[1] as AgentName | undefined;

  const validAgents = AGENTS.map(a => a.name);

  function isValidAgent(name: string): name is AgentName {
    return validAgents.includes(name as AgentName);
  }

  function printUsage(): void {
    console.log(`Usage: bun install.js <command> [agent] [--resource-root <path>]

Commands:
  install [agent]    Install NodeSpace skill for detected (or specified) agents
  uninstall [agent]  Remove NodeSpace skill from detected (or specified) agents

Agents: ${validAgents.join(', ')}

--resource-root <path>  Where SKILL.md/shims/references actually live. Only
                         needed by the compiled standalone binary distribution
                         — a plain dist/install.js run finds these next to
                         itself automatically.

Examples:
  bun install.js install
  bun install.js install claude-code
  bun install.js uninstall
  nodespace-skill-installer install --resource-root /path/to/resources/skill`);
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
    const results = resourceRoot ? install(targetAgents, resourceRoot) : install(targetAgents);

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
}

if (import.meta.main) {
  main();
}
