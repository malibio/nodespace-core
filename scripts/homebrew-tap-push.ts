#!/usr/bin/env bun
/**
 * Shared "clone, write, commit, push" mechanics for pushing an automated
 * update to NodeSpaceAI/homebrew-nodespace (the tap repo).
 *
 * Used by both update-homebrew-cask.ts (Casks/nodespace.rb) and
 * update-homebrew-formula.ts (Formula/nodespace-cli.rb) -- pulled out here
 * so the credential-scrubbing safety net (covering every step, not just the
 * clone -- see pushFilesToTap's inner try/catch) can't drift out of sync
 * between two copies of the same ~35 lines. Both scripts still own their
 * own rendering, digest resolution, and drift-check logic; this file is
 * only the network/git plumbing they share.
 */

import { $ } from "bun";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

export const TAP_REPO = "NodeSpaceAI/homebrew-nodespace";

export interface TapFile {
  /** Path relative to the tap repo root, e.g. "Casks/nodespace.rb" or
   * "Formula/nodespace-cli.rb". */
  relPath: string;
  content: string;
}

/** Clones TAP_REPO fresh, writes each of `files`, and pushes a single
 * commit to main if anything actually changed. Returns whether a push
 * happened, so callers can print an accurate "already in sync" vs "pushed"
 * message without a second network round trip to check. */
export async function pushFilesToTap(
  files: TapFile[],
  commitMessage: string,
  token: string,
): Promise<boolean> {
  const workDir = mkdtempSync(join(tmpdir(), "homebrew-nodespace-push-"));
  try {
    // Defense in depth, covering every git/fs step below (not just the
    // clone): git itself redacts credentials from its own stderr on an
    // auth failure (verified against a real bad-token clone), and none of
    // the commands after the clone reference `authUrl` directly (they act
    // on the already-cloned `workDir`, resolving "origin" from its
    // .git/config) -- but scrub `token` out of ANY error that escapes this
    // block regardless, in case a wrapper error ever echoes more than
    // expected. This must never surface the raw token.
    try {
      const authUrl = `https://x-access-token:${token}@github.com/${TAP_REPO}.git`;
      await $`git clone --depth 1 ${authUrl} ${workDir}`.quiet();

      for (const file of files) {
        const dest = join(workDir, file.relPath);
        mkdirSync(dirname(dest), { recursive: true });
        writeFileSync(dest, file.content);
        await $`git -C ${workDir} add ${file.relPath}`.quiet();
      }

      const staged = await $`git -C ${workDir} diff --cached --quiet`.quiet().nothrow();
      if (staged.exitCode === 0) {
        return false;
      }

      await $`git -C ${workDir} -c user.name="nodespace-release-bot" -c user.email="release-bot@nodespace.app" commit -m ${commitMessage}`.quiet();
      await $`git -C ${workDir} push origin HEAD:main`.quiet();
      return true;
    } catch (err) {
      const message = (err instanceof Error ? err.message : String(err)).replaceAll(
        token,
        "***",
      );
      throw new Error(`push to ${TAP_REPO} failed: ${message}`);
    }
  } finally {
    // Also closes the residual window where a temp credential URL sits in
    // workDir/.git/config in cleartext -- removed as soon as this function
    // returns or throws, on every path.
    rmSync(workDir, { recursive: true, force: true });
  }
}
