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
 *
 * ## Why both halves are idempotent
 *
 * Everything this script writes is an input to `nodespace-app`'s build
 * script: `tauri-build` emits a `cargo:rerun-if-changed` for every
 * `externalBin` and every `resources` entry (`copy_binaries` /
 * `copy_resources` in `tauri-build-2.6.2/src/lib.rs`). So a single rewritten
 * byte — or merely a fresher mtime on an otherwise identical file —
 * invalidates the crate and forces a full rebuild. Since `build:skill` runs
 * automatically before `dev:tauri` and `tauri:build`, an unconditional
 * rewrite taxes every pass through the desktop dev loop even when nothing
 * under `packages/skill/` changed.
 *
 * Both halves therefore no-op when they would reproduce what is already
 * staged, with each check matched to what that half actually costs:
 *
 *   - The **compile** (`bun build --compile`, ~58MB) is guarded by mtime —
 *     the same comparison shape as `build_support.rs`'s `sync_stale_sidecar`
 *     — so the expensive step is skipped outright, not run and discarded.
 *   - The **resource staging** is guarded by content. `tsc` runs without
 *     `--incremental` here, so it rewrites every `dist/*.js` on each run with
 *     byte-identical output and a fresh mtime; an mtime check would see churn
 *     that isn't really there. Comparing bytes and writing only what actually
 *     differs leaves unchanged files' mtimes — and the crate — untouched.
 */

import { $ } from 'bun';
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
} from 'node:fs';
import { dirname, join, relative, sep } from 'node:path';
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

/**
 * Every file under `root`, as paths relative to it. A `root` that does not
 * exist yields no files; a `root` that is itself a file yields the single
 * empty relative path, so a plain file and a directory tree can be fed to
 * the same sync loop.
 */
export function listFilesRecursive(root: string): string[] {
  if (!existsSync(root)) return [];
  if (!statSync(root).isDirectory()) return [''];
  const files: string[] = [];
  const walk = (dir: string): void => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const abs = join(dir, entry.name);
      if (entry.isDirectory()) walk(abs);
      else files.push(relative(root, abs));
    }
  };
  walk(root);
  return files;
}

/**
 * The most recent mtime across `paths` (files and/or directory trees), or
 * `null` if none of them exist. A missing path contributes nothing rather
 * than erroring: an input that isn't there can't have superseded anything.
 * `ignore` drops any relative path for which it returns true, so a caller can
 * exclude subtrees that don't actually feed the output.
 */
export function newestMtimeMs(
  paths: string[],
  ignore: (relativePath: string) => boolean = () => false,
): number | null {
  let newest: number | null = null;
  for (const path of paths) {
    if (!existsSync(path)) continue;
    for (const rel of listFilesRecursive(path)) {
      if (ignore(rel)) continue;
      const { mtimeMs } = statSync(join(path, rel));
      if (newest === null || mtimeMs > newest) newest = mtimeMs;
    }
  }
  return newest;
}

/**
 * Whether `outputPath` is up to date with respect to every input in
 * `inputPaths` — it exists and is at least as new as the newest of them. A
 * missing output is never fresh; an output whose inputs have all vanished is
 * (nothing remains that could have superseded it).
 */
export function isOutputFresh(
  outputPath: string,
  inputPaths: string[],
  ignore?: (relativePath: string) => boolean,
): boolean {
  if (!existsSync(outputPath)) return false;
  const newestInput = newestMtimeMs(inputPaths, ignore);
  if (newestInput === null) return true;
  return statSync(outputPath).mtimeMs >= newestInput;
}

/**
 * Mirrors `srcRoot` onto `destRoot` writing only what actually differs:
 * files whose bytes changed are copied, files no longer present in the
 * source are deleted, and byte-identical files are left completely alone,
 * mtime included (see the module doc on why that matters). Returns the count
 * of files written or removed, so the caller can report a real no-op.
 */
export function syncTreeByContent(srcRoot: string, destRoot: string): number {
  const srcFiles = new Set(listFilesRecursive(srcRoot));
  let changed = 0;

  for (const rel of listFilesRecursive(destRoot)) {
    if (srcFiles.has(rel)) continue;
    rmSync(join(destRoot, rel));
    changed += 1;
  }

  for (const rel of srcFiles) {
    const src = join(srcRoot, rel);
    const dest = join(destRoot, rel);
    if (existsSync(dest) && readFileSync(dest).equals(readFileSync(src))) continue;
    mkdirSync(dirname(dest), { recursive: true });
    copyFileSync(src, dest);
    changed += 1;
  }

  return changed;
}

/**
 * Removes directories under `keepRoot` that no longer hold any file, deepest
 * first, leaving `keepRoot` itself in place. `syncTreeByContent` deletes
 * files but leaves their parents behind; an emptied directory is harmless to
 * the bundle but confusing to find on disk.
 */
export function pruneEmptyDirs(dir: string, keepRoot: string = dir): void {
  if (!existsSync(dir) || !statSync(dir).isDirectory()) return;
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.isDirectory()) pruneEmptyDirs(join(dir, entry.name), keepRoot);
  }
  if (dir !== keepRoot && readdirSync(dir).length === 0) {
    rmSync(dir, { recursive: true });
  }
}

// Host triple: matches whatever Tauri/rustc expects on this platform for its
// externalBin lookup, same triples the sidecar cargo builds already use
// (release.yml's per-platform steps, build-sidecars.ts locally). Compiled
// natively on each CI runner's own platform — no cross-compilation. Only
// macOS and Windows ship a Tauri desktop app at all (Linux is CLI+daemon
// binaries only, no packaged GUI app, so no skill installer to run there).
export function hostTriple(): string | null {
  if (platform() === 'darwin') {
    return `${arch() === 'arm64' ? 'aarch64' : 'x86_64'}-apple-darwin`;
  }
  if (platform() === 'win32') {
    return 'x86_64-pc-windows-msvc';
  }
  return null;
}

/**
 * What `bun build --compile` actually bundles into the standalone installer:
 * everything reachable from `src/install.ts`, plus the manifests that shape
 * how those get resolved. Deliberately NOT SKILL.md, references/ or shims/ —
 * the binary reads those at runtime from `--resource-root`, so they are
 * staged resources, never compile inputs, and counting them here would
 * recompile 58MB every time a doc line moved.
 */
export function compileInputs(skillDir: string): string[] {
  return [join(skillDir, 'src'), join(skillDir, 'package.json'), join(skillDir, 'tsconfig.json')];
}

/**
 * Paths under `compileInputs` that don't actually reach the bundle:
 * `src/tests/` is unreachable from `src/install.ts` and excluded by
 * packages/skill's own tsconfig, so editing a skill test should not cost a
 * 58MB recompile.
 */
export function isNotACompileInput(relativePath: string): boolean {
  return relativePath === 'tests' || relativePath.startsWith(`tests${sep}`);
}

/**
 * Entries staged into `resources/skill/`. Anything else found there is a
 * leftover from an earlier layout and gets dropped.
 *
 * `references/` carries the on-demand tier of the skill (the full CLI
 * reference). It is part of the shipped artifact, not a dev-only doc: SKILL.md
 * points at it by relative path, so omitting it leaves the body referring to a
 * file that isn't there.
 */
export const STAGED_ENTRIES = ['dist', 'shims', 'SKILL.md', 'references', 'package.json'];

async function main(): Promise<void> {
  console.log('Building packages/skill...');
  await $`bun run --cwd ${SKILL_DIR} build`;

  if (!existsSync(join(SKILL_DIR, 'dist', 'install.js'))) {
    throw new Error(
      `packages/skill build did not produce dist/install.js (expected at ${join(SKILL_DIR, 'dist', 'install.js')})`,
    );
  }

  console.log(`Staging skill resources -> ${RESOURCE_DIR}`);
  mkdirSync(RESOURCE_DIR, { recursive: true });

  // Drop anything already staged that is no longer one of STAGED_ENTRIES, so
  // a since-removed entry never lingers in the bundle across rebuilds.
  for (const entry of readdirSync(RESOURCE_DIR)) {
    if (!STAGED_ENTRIES.includes(entry)) {
      rmSync(join(RESOURCE_DIR, entry), { recursive: true, force: true });
    }
  }

  let stagedChanges = 0;
  for (const entry of STAGED_ENTRIES) {
    stagedChanges += syncTreeByContent(join(SKILL_DIR, entry), join(RESOURCE_DIR, entry));
  }
  pruneEmptyDirs(RESOURCE_DIR);
  console.log(
    stagedChanges === 0
      ? '  Staged resources already current — nothing rewritten.'
      : `  Updated ${stagedChanges} staged file(s).`,
  );

  const triple = hostTriple();

  if (!triple) {
    console.log('Skipping compiled skill-installer binary (no Tauri desktop app on this platform).');
  } else {
    mkdirSync(BIN_DIR, { recursive: true });
    const ext = platform() === 'win32' ? '.exe' : '';
    const outfile = join(BIN_DIR, `nodespace-skill-installer-${triple}${ext}`);

    if (isOutputFresh(outfile, compileInputs(SKILL_DIR), isNotACompileInput)) {
      console.log(`Standalone skill installer is current -> ${outfile} (skipping compile).`);
    } else {
      console.log(`Compiling standalone skill installer -> ${outfile}`);
      await $`bun build --compile ${join(SKILL_DIR, 'src', 'install.ts')} --outfile ${outfile}`.quiet();
      if (platform() !== 'win32') {
        chmodSync(outfile, 0o755);
      }
    }
  }

  console.log('Done.');
}

if (import.meta.main) {
  await main();
}
