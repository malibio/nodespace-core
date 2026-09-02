#!/usr/bin/env bun
/**
 * Build packages/skill and stage its runtime output where the desktop app
 * expects to find it.
 *
 * packages/skill ships two ways, both produced here (see
 * packages/desktop-app/src-tauri/src/skill_setup.rs's `Installer` enum for
 * which one a given launch actually uses):
 *
 *   1. **The compiled standalone binary** (`bun build --compile`) — the
 *      preferred path: no external bun/node dependency at all, so a packaged
 *      app's end user never hits "runtime not found". Staged as
 *      `binaries/nodespace-skill-installer-{triple}`, declared as an
 *      `externalBin` sidecar in tauri.conf.json (same mechanism as
 *      `nodespaced`/`nodespace`).
 *   2. **The plain JS build** (`tsc` compiles src/ -> dist/, packages/skill/
 *      package.json's own `build` script) — the fallback for a platform this
 *      hasn't been wired up for yet, or a dev/source checkout. Needs `bun` or
 *      `node` on $PATH to run. Staged (dist/, shims/, SKILL.md, references/,
 *      package.json for its `"type": "module"` marker) into
 *      packages/desktop-app/src-tauri/resources/skill/, a `resources` entry
 *      in tauri.conf.json — also where the compiled binary's
 *      `--resource-root` points, since it can't infer that from its own
 *      compiled-executable location the way dist/install.js can from
 *      `import.meta.url`.
 *
 * Tauri copies declared resources/externalBin into the build cache for
 * `tauri dev` too, so this single staging step serves both dev and packaged
 * builds. Run automatically before `dev:tauri` and `tauri:build` (see
 * packages/desktop-app/package.json) so neither is ever stale.
 *
 * The compiled binary is only produced for the CURRENT host here — this
 * script is for local development, same scope as `build-sidecars.ts`
 * (real cross-platform release builds compile natively on each platform's
 * own CI runner; see `.github/workflows/release.yml`).
 */

import { $ } from 'bun';
import { chmodSync, cpSync, existsSync, mkdirSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { arch, platform } from 'node:os';

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
const BIN_DIR = join(WORKSPACE_ROOT, 'packages', 'desktop-app', 'src-tauri', 'binaries');

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

// Host triple: matches whatever Tauri/rustc expects on this platform for its
// externalBin lookup, same triples the sidecar cargo builds already use
// (release.yml's per-platform steps, build-sidecars.ts locally). Compiled
// natively on each CI runner's own platform — no cross-compilation. Only
// macOS and Windows ship a Tauri desktop app at all (Linux is CLI+daemon
// binaries only, no packaged GUI app, so no skill installer to run there).
const hostTriple = (): string | null => {
  if (platform() === 'darwin') {
    return `${arch() === 'arm64' ? 'aarch64' : 'x86_64'}-apple-darwin`;
  }
  if (platform() === 'win32') {
    return 'x86_64-pc-windows-msvc';
  }
  return null;
};
const triple = hostTriple();

if (!triple) {
  console.log('Skipping compiled skill-installer binary (no Tauri desktop app on this platform).');
} else {
  mkdirSync(BIN_DIR, { recursive: true });
  const ext = platform() === 'win32' ? '.exe' : '';
  const outfile = join(BIN_DIR, `nodespace-skill-installer-${triple}${ext}`);

  console.log(`Compiling standalone skill installer -> ${outfile}`);
  await $`bun build --compile ${join(SKILL_DIR, 'src', 'install.ts')} --outfile ${outfile}`.quiet();
  if (platform() !== 'win32') {
    chmodSync(outfile, 0o755);
  }
}

console.log('Done.');
