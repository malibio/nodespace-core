#!/usr/bin/env bun
/**
 * Pin nodespace-website's install.sh (the `curl -fsSL https://nodespace.ai/
 * install.sh | sh` bootstrap installer) to a published nodespace-core
 * release.
 *
 * install.sh is a committed static file in NodeSpaceAI/nodespace-website
 * (see nodespace-core#2146) -- it is not generated from a template here.
 * What this script owns is exactly one line inside that file:
 *
 *   NODESPACE_CLI_VERSION="v0.2.0"
 *
 * install.sh deliberately does NOT fetch "latest" from GitHub Releases --
 * release.yml's `on: release: types: [created]` trigger fires the instant
 * a release is published, before any of its build-and-upload jobs even
 * start, so "latest" can point at a release with a missing or partial
 * asset set for several minutes. This script is the thing that advances
 * the pin, and it only runs after this repo's own release build jobs
 * report success (see release.yml's sync-install-script job, a sibling of
 * sync-homebrew-cask -- both exist because a hand-maintained downstream
 * artifact silently drifted before, see #2114/#2154 for the cask's
 * version).
 *
 * Usage:
 *   bun run scripts/publish-install-script.ts <version>            # dry run --
 *                                                                   # prints the diff, pushes nothing
 *   bun run scripts/publish-install-script.ts <version> --push      # pushes to nodespace-website's
 *                                                                    # main branch (requires
 *                                                                    # WEBSITE_DEPLOY_TOKEN)
 *
 * `--push` requires WEBSITE_DEPLOY_TOKEN: a PAT (or fine-grained token)
 * with `contents: write` on NodeSpaceAI/nodespace-website, set as a repo
 * secret once this is wired into the release flow -- `secrets.GITHUB_TOKEN`
 * is scoped to nodespace-core only and cannot push cross-repo (same
 * constraint HOMEBREW_TAP_TOKEN documents for the cask sync).
 */

import { $ } from "bun";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

export const CORE_REPO = "NodeSpaceAI/nodespace-core";
export const WEBSITE_REPO = "NodeSpaceAI/nodespace-website";

// The CLI targets release.yml's build-headless job actually produces and
// uploads for every release. x86_64-apple-darwin is deliberately excluded
// -- v0.2.0 shipped without it despite SHA256SUMS listing a checksum for
// it (a real release-asset/checksum-file mismatch, filed separately; see
// this repo's issue tracker), and install.sh's own direct-download path
// already degrades to a clear error message on unsupported platforms
// rather than assuming every target always exists.
const REQUIRED_HEADLESS_TARGETS = [
  "aarch64-apple-darwin",
  "aarch64-unknown-linux-gnu",
  "x86_64-unknown-linux-gnu",
];

export function normalizeTag(version: string): string {
  return version.startsWith("v") ? version : `v${version}`;
}

// Trailing whitespace is matched with `[ \t]*`, not `\s*` -- `\s` also
// matches the newline itself in `/m` mode, so a greedy `\s*$` swallows the
// line's own trailing `\n` into the match and `.replace()` then drops it,
// silently merging this line into the next one. Caught by
// publish-install-script.test.ts's idempotency/exact-shape assertions.
const VERSION_PIN_RE = /^NODESPACE_CLI_VERSION="v?[^"]+"[ \t]*$/m;

/** Pure string transform -- no network -- so it's unit-testable without
 * touching install.sh's actual current content. Throws rather than
 * silently no-op-ing if the marker line isn't found, so a future rename
 * of the variable in install.sh fails this script loudly instead of
 * quietly leaving the pin stale. */
export function pinVersion(installShContent: string, version: string): string {
  if (!VERSION_PIN_RE.test(installShContent)) {
    throw new Error(
      'could not find a NODESPACE_CLI_VERSION="..." pin line in install.sh -- ' +
        "has it been renamed or restructured? Refusing to guess where the pin goes.",
    );
  }
  return installShContent.replace(VERSION_PIN_RE, `NODESPACE_CLI_VERSION="${normalizeTag(version)}"`);
}

export interface AssetCheckResult {
  missing: string[];
}

/** Confirms the release actually has the headless CLI binaries install.sh
 * will try to download before pinning to it -- pinning to a version whose
 * assets never finished uploading would be worse than staying on the
 * previous pin. Missing is reported, not silently ignored -- callers
 * decide whether to proceed (see main()'s --push gate). */
export async function checkReleaseAssets(version: string): Promise<AssetCheckResult> {
  const tag = normalizeTag(version);
  const out = await $`gh release view ${tag} --repo ${CORE_REPO} --json assets`.text();
  const parsed = JSON.parse(out) as { assets: { name: string }[] };
  const names = new Set(parsed.assets.map((a) => a.name));
  const missing = REQUIRED_HEADLESS_TARGETS.filter((t) => !names.has(`nodespace-${t}`));
  return { missing };
}

export async function fetchWebsiteInstallScript(): Promise<string> {
  const res = await fetch(`https://raw.githubusercontent.com/${WEBSITE_REPO}/main/install.sh`);
  if (!res.ok) {
    throw new Error(`failed to fetch ${WEBSITE_REPO}'s install.sh: HTTP ${res.status}`);
  }
  return res.text();
}

async function pushInstallScriptUpdate(version: string, content: string, token: string): Promise<void> {
  const workDir = mkdtempSync(join(tmpdir(), "nodespace-website-push-"));
  try {
    const authUrl = `https://x-access-token:${token}@github.com/${WEBSITE_REPO}.git`;
    try {
      await $`git clone --depth 1 ${authUrl} ${workDir}`.quiet();
    } catch (err) {
      // Same defense-in-depth as update-homebrew-cask.ts's pushCaskUpdate:
      // git itself redacts credentials from its own stderr on an auth
      // failure, but scrub the raw token out of whatever the shell
      // wrapper's error carries anyway -- this must never surface it.
      const message = (err instanceof Error ? err.message : String(err)).replaceAll(token, "***");
      throw new Error(`git clone of ${WEBSITE_REPO} failed: ${message}`);
    }

    writeFileSync(join(workDir, "install.sh"), content);

    await $`git -C ${workDir} add install.sh`.quiet();
    const staged = await $`git -C ${workDir} diff --cached --quiet`.quiet().nothrow();
    if (staged.exitCode === 0) {
      console.log("install.sh is already pinned to this version -- nothing to push.");
      return;
    }

    const tag = normalizeTag(version);
    await $`git -C ${workDir} -c user.name="nodespace-release-bot" -c user.email="release-bot@nodespace.app" commit -m ${`Pin install.sh to ${tag} (automated release sync)`}`.quiet();
    await $`git -C ${workDir} push origin HEAD:main`.quiet();
    console.log(`Pushed install.sh version pin (${tag}) to ${WEBSITE_REPO}.`);
  } finally {
    // Also closes the window where a temp credential URL sits in
    // workDir/.git/config in cleartext -- removed on every path.
    rmSync(workDir, { recursive: true, force: true });
  }
}

function usage(): void {
  console.log(`Usage:
  bun run scripts/publish-install-script.ts <version> [--push]`);
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  const command = args[0];

  if (!command || !/^v?\d+\.\d+\.\d+(-[a-zA-Z0-9.]+)?$/.test(command)) {
    usage();
    process.exit(1);
  }

  const push = args.includes("--push");
  const token = process.env.WEBSITE_DEPLOY_TOKEN;
  if (push && !token) {
    console.error(
      "WEBSITE_DEPLOY_TOKEN is not set -- required for --push (a PAT with contents:write on " +
        `${WEBSITE_REPO}). Running without --push shows what would change.`,
    );
    process.exit(1);
  }

  const { missing } = await checkReleaseAssets(command);
  if (missing.length > 0) {
    console.error(
      `✗ ${normalizeTag(command)} is missing expected headless CLI assets, refusing to pin: ` +
        missing.map((t) => `nodespace-${t}`).join(", "),
    );
    process.exit(1);
  }

  const current = await fetchWebsiteInstallScript();
  const updated = pinVersion(current, command);

  if (current === updated) {
    console.log(`install.sh is already pinned to ${normalizeTag(command)} -- nothing to do.`);
    return;
  }

  console.log(`--- NODESPACE_CLI_VERSION diff ---`);
  console.log(current.match(VERSION_PIN_RE)?.[0] ?? "(no existing pin found)");
  console.log("->");
  console.log(updated.match(VERSION_PIN_RE)?.[0]);

  if (!push) {
    console.log("(dry run -- pass --push with WEBSITE_DEPLOY_TOKEN set to publish this)");
    return;
  }
  await pushInstallScriptUpdate(command, updated, token as string);
}

if (import.meta.main) {
  // Matches scripts/test-gate.ts's / update-homebrew-cask.ts's convention:
  // a bare uncaught rejection here would otherwise print a raw Bun stack
  // trace instead of an operator-facing message.
  try {
    await main();
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    console.error(`✗ ${message}`);
    process.exit(1);
  }
}
