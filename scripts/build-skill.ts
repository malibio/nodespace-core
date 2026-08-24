#!/usr/bin/env bun
/**
 * Build packages/skill and stage its runtime output where the desktop app
 * expects to find it.
 *
 * packages/skill ships as a Tauri-bundled resource, not an npm package fetched
 * at runtime (see packages/desktop-app/src-tauri/src/skill_setup.rs) — this
 * script is the one place that produces the artifact both paths depend on:
 *
 *   1. `tsc` compiles src/ -> dist/ (packages/skill/package.json's own `build`
 *      script), which also makes `packages/skill/dist/install.js` resolvable
 *      directly for dev-mode / source-checkout runs.
 *   2. dist/, shims/, SKILL.md, references/, and package.json (for its `"type": "module"`
 *      marker, so a standalone dist/install.js still parses as ESM) are
 *      copied into packages/desktop-app/src-tauri/resources/skill/, which
 *      tauri.conf.json declares as a bundled `resources` entry. Tauri copies
 *      declared resources into the build cache for `tauri dev` too, so this
 *      single staged copy serves both dev and packaged builds via
 *      `BaseDirectory::Resource`.
 *
 * Run automatically before `dev:tauri` and `tauri:build` (see
 * packages/desktop-app/package.json) so the installer is never stale.
 */

import { $ } from 'bun';
import { cpSync, existsSync, mkdirSync, rmSync } from 'node:fs';
import { join } from 'node:path';

const WORKSPACE_ROOT = join(import.meta.dir, '..');
const SKILL_DIR = join(WORKSPACE_ROOT, 'packages', 'skill');
const RESOURCE_DIR = join(
  WORKSPACE_ROOT,
  'packages',
  'desktop-app',
  'src-tauri',
  'resources',
  'skill',
);

console.log('Building packages/skill...');
await $`bun run --cwd ${SKILL_DIR} build`;

if (!existsSync(join(SKILL_DIR, 'dist', 'install.js'))) {
  throw new Error(
    `packages/skill build did not produce dist/install.js (expected at ${join(SKILL_DIR, 'dist', 'install.js')})`,
  );
}

console.log(`Staging skill resources -> ${RESOURCE_DIR}`);
// Clear stale content first so a since-removed shim/dist file never lingers
// in the bundle across rebuilds.
rmSync(RESOURCE_DIR, { recursive: true, force: true });
mkdirSync(RESOURCE_DIR, { recursive: true });

// `references/` carries the on-demand tier of the skill (the full CLI
// reference). It is part of the shipped artifact, not a dev-only doc: SKILL.md
// points at it by relative path, so omitting it leaves the body referring to a
// file that isn't there.
for (const entry of ['dist', 'shims', 'SKILL.md', 'references', 'package.json']) {
  cpSync(join(SKILL_DIR, entry), join(RESOURCE_DIR, entry), { recursive: true });
}

console.log('Done.');
