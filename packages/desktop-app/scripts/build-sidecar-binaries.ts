#!/usr/bin/env bun

/**
 * Build sidecar binaries (nodespaced, nodespace CLI) from Rust source
 * and copy them into src-tauri/binaries/ with the platform triple suffix
 * that Tauri's externalBin mechanism expects.
 *
 * Run automatically by dev:tauri and tauri:build so the committed binaries
 * are never stale.
 */

import { $ } from 'bun';
import { existsSync, mkdirSync } from 'node:fs';
import { copyFileSync, chmodSync } from 'node:fs';
import { join } from 'node:path';

const WORKSPACE_ROOT = join(import.meta.dir, '../../..');
const BINARIES_DIR = join(import.meta.dir, '../src-tauri/binaries');

const PACKAGES = [
  { crate: 'nodespace-daemon', bin: 'nodespaced' },
  { crate: 'nodespace-cli', bin: 'nodespace' },
];

async function getTargetTriple(): Promise<string> {
  const result = await $`rustc -vV`.quiet().text();
  const match = result.match(/^host:\s+(.+)$/m);
  if (!match) throw new Error('Could not determine host target triple from rustc -vV');
  return match[1].trim();
}

async function build(profile: 'debug' | 'release'): Promise<void> {
  const args = profile === 'release' ? ['--release'] : [];
  const crates = PACKAGES.flatMap(({ crate }) => ['-p', crate]);

  console.log(`Building sidecar binaries (${profile})...`);
  await $`cargo build ${crates} ${args}`.cwd(WORKSPACE_ROOT);
}

function copyBinaries(triple: string, profile: 'debug' | 'release'): void {
  const targetDir = join(WORKSPACE_ROOT, 'target', profile);

  if (!existsSync(BINARIES_DIR)) mkdirSync(BINARIES_DIR, { recursive: true });

  for (const { bin } of PACKAGES) {
    const src = join(targetDir, bin);
    const dest = join(BINARIES_DIR, `${bin}-${triple}`);

    if (!existsSync(src)) {
      throw new Error(`Built binary not found: ${src}`);
    }

    copyFileSync(src, dest);
    chmodSync(dest, 0o755);
    console.log(`  ${bin}-${triple} → src-tauri/binaries/`);
  }
}

async function main(): Promise<void> {
  const profile = process.argv.includes('--release') ? 'release' : 'debug';

  const triple = await getTargetTriple();
  console.log(`Target triple: ${triple}`);

  await build(profile);
  copyBinaries(triple, profile);

  console.log('Sidecar binaries up to date.');
}

main().catch((err) => {
  console.error('build-sidecar-binaries failed:', err.message ?? err);
  process.exit(1);
});
