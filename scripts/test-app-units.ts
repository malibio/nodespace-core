#!/usr/bin/env bun
/**
 * Run `nodespace-app`'s `src/` unit tests, once its build prerequisites are
 * known to be in place.
 *
 * `rust:test` covers that crate's unit tests, which means `test:all` compiles
 * it — and compiling it runs `tauri_build::build()`, which hard-fails on any
 * declared `externalBin` or `resources` entry in tauri.conf.json that isn't
 * staged: a missing sidecar is `ResourcePathNotFound`, and a `resources` glob
 * matching nothing is `GlobPathNotFound`. Both are gitignored, so a fresh
 * checkout satisfies neither.
 *
 * build.rs's own `sync_stale_sidecar` guard rescues only `nodespaced` and
 * `nodespace`, and only when `target/<profile>/<bin>` already exists to copy
 * from — on a fresh worktree it doesn't, so the guard is a no-op and the
 * build fails. `nodespace-skill-installer` has no such guard at all, and the
 * `resources` globs have none either: the one under `resources/models`
 * survives only because a tracked `.gitkeep` keeps it non-empty, while the one
 * under `resources/skill` has no tracked file at all and is staged solely by
 * `build:skill`.
 *
 * Left alone, that surfaces from deep inside a build script naming one
 * missing path, with no hint which command produces it. This checks every
 * required path up front — read from tauri.conf.json, not restated here — and
 * says what to run.
 *
 * The check and the cargo invocation live together because the answer to
 * "are the prerequisites there?" has three outcomes, not two: ready, missing
 * (build them), and unbuildable-on-this-platform (skip). Linux ships CLI +
 * daemon binaries only, no packaged GUI app — `build:skill` stages no
 * installer there and `build-sidecars.ts` hardcodes an `-apple-darwin`
 * triple, so the sidecars cannot be produced at all and the crate cannot
 * compile. Expressing that three-way result as a shell `&&` chain in
 * package.json would either fail the suite on Linux or swallow real failures.
 */

import { Glob } from 'bun';
import { existsSync, readFileSync } from 'node:fs';
import { arch, platform } from 'node:os';
import { join } from 'node:path';

const TAURI_DIR = join(
  import.meta.dir,
  '..',
  'packages',
  'desktop-app',
  'src-tauri',
);

/**
 * Which command produces a given staged path. Matched longest-prefix-first,
 * so the more specific skill-installer entry wins over the `binaries/` one.
 * This is the only hand-maintained mapping left — the *set* of required paths
 * is read from tauri.conf.json rather than restated, since a fourth
 * hand-synced copy of that list (after the config itself, build.rs's
 * EXTERNAL_BIN_NAMES, and release tooling) is exactly the drift this script
 * would otherwise invite.
 */
const PRODUCERS: { prefix: string; command: string }[] = [
  { prefix: 'binaries/nodespace-skill-installer', command: 'bun run build:skill' },
  { prefix: 'resources/skill', command: 'bun run build:skill' },
  { prefix: 'binaries/', command: 'bun run build:sidecars --debug' },
  { prefix: 'resources/models', command: 'bun run download:models' },
];

const producerFor = (path: string): string =>
  [...PRODUCERS]
    .sort((a, b) => b.prefix.length - a.prefix.length)
    .find(({ prefix }) => path.startsWith(prefix))?.command ??
  'see tauri.conf.json';

const hostTriple = (): string | null => {
  if (platform() === 'darwin') {
    return `${arch() === 'arm64' ? 'aarch64' : 'x86_64'}-apple-darwin`;
  }
  if (platform() === 'win32') {
    return 'x86_64-pc-windows-msvc';
  }
  return null;
};

/**
 * The staged paths `tauri_build::build()` will insist on, read from the
 * config it reads. `externalBin` entries gain the host triple and exe suffix
 * and must exist as files; `resources` entries are globs that must match at
 * least one file (an empty match is `GlobPathNotFound`, a hard error just
 * like a missing file).
 */
const requiredPaths = (triple: string): { path: string; kind: 'file' | 'glob' }[] => {
  const config = JSON.parse(
    readFileSync(join(TAURI_DIR, 'tauri.conf.json'), 'utf8'),
  );
  const bundle = config.bundle ?? {};
  const ext = platform() === 'win32' ? '.exe' : '';

  return [
    ...(bundle.externalBin ?? []).map((bin: string) => ({
      path: `${bin}-${triple}${ext}`,
      kind: 'file' as const,
    })),
    ...(bundle.resources ?? []).map((pattern: string) => ({
      path: pattern,
      kind: 'glob' as const,
    })),
  ];
};

const triple = hostTriple();
if (!triple) {
  console.log(
    'Skipping nodespace-app unit tests (no Tauri desktop app on this platform).',
  );
  process.exit(0);
}

const missing = requiredPaths(triple).filter(({ path, kind }) => {
  if (kind === 'file') {
    return !existsSync(join(TAURI_DIR, path));
  }
  // A glob that matches nothing is `GlobPathNotFound` — as fatal as a missing
  // file, and the reason the `resources/skill` glob breaks a fresh checkout
  // while the `resources/models` one survives on a tracked `.gitkeep`.
  // `dot: true` because that .gitkeep is the *only* thing keeping the models
  // glob non-empty, and tauri's glob crate matches dotfiles where Bun's
  // skips them by default — without it this reports a false missing path.
  const first = new Glob(path)
    .scanSync({ cwd: TAURI_DIR, dot: true })
    .next();
  return first.done === true;
});

if (missing.length > 0) {
  const commands = [...new Set(missing.map(({ path }) => producerFor(path)))];
  console.error(
    `\nnodespace-app cannot compile — ${missing.length} staged path${
      missing.length === 1 ? '' : 's'
    } missing:\n`,
  );
  for (const { path } of missing) {
    console.error(`  ✗ ${path}`);
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

// --test-threads=2 matches the rest of `rust:test`. The =1 cap the pre-push
// gate applies to this crate is for its `tests/*.rs` targets, each of which
// spawns a real nodespaced; these unit tests are in-process and need no such
// serialization (the one that touches process-global env takes its own lock).
const result = Bun.spawnSync(
  [
    'cargo',
    'test',
    '--lib',
    '--bins',
    '-p',
    'nodespace-app',
    '-p',
    'nodespace-app-test-support',
    '--',
    '--test-threads=2',
  ],
  { stdio: ['inherit', 'inherit', 'inherit'] },
);

process.exit(result.exitCode ?? 1);
