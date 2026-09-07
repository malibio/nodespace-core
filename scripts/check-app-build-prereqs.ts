#!/usr/bin/env bun
/**
 * Verify the staged files `nodespace-app` needs before it will compile.
 *
 * `rust:test` covers that crate's `src/` unit tests, which means `test:all`
 * now compiles it — and compiling it runs `tauri_build::build()`, which hard
 * -fails on any declared `resources`/`externalBin` entry in tauri.conf.json
 * that doesn't physically exist. Every one of those entries is gitignored, so
 * a fresh checkout has none of them.
 *
 * build.rs's own `sync_stale_sidecar` guard rescues only `nodespaced` and
 * `nodespace`, and only when `target/<profile>/<bin>` already exists to copy
 * from — on a fresh worktree it doesn't, so the guard is a no-op and the
 * build fails. `nodespace-skill-installer` has no such guard at all.
 *
 * Left alone, that surfaces as a bare `resource path '...' doesn't exist`
 * from deep inside a build script, naming one missing file with no hint that
 * two sibling commands produce it. This checks all of them up front and says
 * what to run.
 *
 * Skipped where the crate isn't built anyway: Linux ships CLI + daemon
 * binaries only, no packaged GUI app (see `hostTriple()` in build-skill.ts),
 * so `build:skill` deliberately stages no installer there.
 */

import { existsSync } from 'node:fs';
import { arch, platform } from 'node:os';
import { join } from 'node:path';

const BIN_DIR = join(
  import.meta.dir,
  '..',
  'packages',
  'desktop-app',
  'src-tauri',
  'binaries',
);

// Kept in sync by hand with tauri.conf.json's `externalBin`, the same way
// build.rs's EXTERNAL_BIN_NAMES is — neither has a cheap way to parse it.
// Each entry names the command that produces it, so the error can be acted on.
const SIDECARS = [
  { bin: 'nodespaced', producedBy: 'bun run build:sidecars --debug' },
  { bin: 'nodespace', producedBy: 'bun run build:sidecars --debug' },
  { bin: 'nodespace-skill-installer', producedBy: 'bun run build:skill' },
];

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
  // No Tauri desktop app on this platform, so nothing stages these and
  // nothing compiles the crate that wants them.
  process.exit(0);
}

const ext = platform() === 'win32' ? '.exe' : '';
const missing = SIDECARS.filter(
  ({ bin }) => !existsSync(join(BIN_DIR, `${bin}-${triple}${ext}`)),
);

if (missing.length > 0) {
  const commands = [...new Set(missing.map((m) => m.producedBy))];
  console.error(
    `\nnodespace-app cannot compile — ${missing.length} staged sidecar${
      missing.length === 1 ? '' : 's'
    } missing:\n`,
  );
  for (const { bin } of missing) {
    console.error(`  ✗ binaries/${bin}-${triple}${ext}`);
  }
  console.error(
    `\nThese are gitignored, so every fresh checkout builds them once:\n`,
  );
  for (const command of commands) {
    console.error(`  ${command}`);
  }
  console.error(
    `\nThen re-run. (A cold sidecar build takes several minutes.)\n`,
  );
  process.exit(1);
}
