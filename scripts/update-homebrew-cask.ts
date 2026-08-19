#!/usr/bin/env bun
/**
 * Sync the Homebrew cask (NodeSpaceAI/homebrew-nodespace) to a published
 * nodespace-core release.
 *
 * This repo has no hosted CI (by design) — release-cutting is the local
 * `bun run release <version>` flow (scripts/release.ts), which tags and
 * publishes the GitHub release that `.github/workflows/release.yml` then
 * builds assets for. Nothing in that chain used to touch the tap: the cask
 * was hand-edited in a separate repo, silently drifted for two months
 * (v0.1.6 while v0.2.0 shipped), and nothing caught it. This script is the
 * step that was missing.
 *
 * It never hand-types a digest: every sha256 is computed locally with
 * node:crypto against the actual bytes downloaded from the release asset
 * URL GitHub reports for that file.
 *
 * Usage:
 *   bun run scripts/update-homebrew-cask.ts <version>            # dry run — prints the
 *                                                                 # cask + diff, pushes nothing
 *   bun run scripts/update-homebrew-cask.ts <version> --push      # pushes to homebrew-nodespace's
 *                                                                 # main branch (requires
 *                                                                 # HOMEBREW_TAP_TOKEN)
 *   bun run scripts/update-homebrew-cask.ts drift-check           # compares the tap's live cask
 *                                                                 # version against the latest
 *                                                                 # published release; exits 1
 *                                                                 # (loud) on mismatch
 *
 * `--push` requires HOMEBREW_TAP_TOKEN: a PAT (or fine-grained token) with
 * `contents: write` on NodeSpaceAI/homebrew-nodespace, set as a repo secret
 * once this is wired into the release flow. `secrets.GITHUB_TOKEN` is scoped
 * to nodespace-core only and cannot push cross-repo.
 *
 * Where this hooks in: after `scripts/release.ts` creates a release and
 * `release.yml` finishes uploading assets (DMGs included), run
 *   bun run scripts/update-homebrew-cask.ts <version> --push
 * See release.yml's `upload-checksums` job for the shape of "wait for assets,
 * then do one more thing" — this script is a natural sibling job there, or a
 * manual step run right after `bun run release:watch` reports the build
 * finished, until it is wired into CI directly.
 */

import { $ } from "bun";
import { createHash } from "node:crypto";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

export const CORE_REPO = "NodeSpaceAI/nodespace-core";
export const TAP_REPO = "NodeSpaceAI/homebrew-nodespace";

// The .app bundle's CLI binary lives at Contents/MacOS/nodespace in the
// current Tauri packaging layout (verified by mounting the real v0.2.0 .dmg
// — the previous hand-maintained cask pointed at Contents/Resources/bin/
// instead, which doesn't exist and breaks `brew install --cask`). Re-verify
// this path if the Tauri bundle/externalBin layout ever changes.
const CASK_BINARY_PATH = "Contents/MacOS/nodespace";

// release.yml builds with MACOSX_DEPLOYMENT_TARGET=14.0 (Metal GPU
// embeddings require Sonoma+, see #990).
const MIN_MACOS = "sonoma";

export interface ReleaseAsset {
  name: string;
  url: string;
}

export interface ArchDigest {
  arch: "arm" | "intel";
  fileName: string;
  sha256: string;
}

// The per-architecture .dmg filename suffix release.yml / tauri-action
// produce -- e.g. NodeSpace_0.2.0_aarch64.dmg. Used both to look up the real
// release asset (with the resolved version) and to build the cask's url
// stanza (with Ruby's `#{version}` interpolation, so a future manual glance
// at the file matches how casks are conventionally written).
const ARCH_SUFFIX: Record<"arm" | "intel", string> = { arm: "aarch64", intel: "x64" };

export function normalizeVersion(version: string): string {
  return version.replace(/^v/, "");
}

/** Pure hashing helper — kept separate from network I/O so it's unit-testable. */
export function sha256Hex(bytes: Uint8Array): string {
  return createHash("sha256").update(bytes).digest("hex");
}

export async function fetchReleaseAssets(version: string): Promise<ReleaseAsset[]> {
  const tag = version.startsWith("v") ? version : `v${version}`;
  const out = await $`gh release view ${tag} --repo ${CORE_REPO} --json assets`.text();
  const parsed = JSON.parse(out) as { assets: ReleaseAsset[] };
  return parsed.assets;
}

/** Downloads `url` to `destPath` and returns its sha256 — the digest always
 * comes from bytes that were actually fetched, never typed or copied. */
export async function downloadAndHash(url: string, destPath: string): Promise<string> {
  const res = await fetch(url);
  if (!res.ok) {
    throw new Error(`failed to download ${url}: HTTP ${res.status}`);
  }
  const bytes = new Uint8Array(await res.arrayBuffer());
  await Bun.write(destPath, bytes);
  return sha256Hex(bytes);
}

export interface ArchDigestResult {
  arm?: ArchDigest;
  intel?: ArchDigest;
  missing: string[];
}

/** Resolves per-architecture .dmg digests from the actual published release
 * assets. Missing architectures are reported, not fatal — a release that
 * only ships one macOS architecture (as v0.2.0 does — see
 * NodeSpaceAI/nodespace-core#2154) still produces a valid, honest cask. */
export async function resolveArchDigests(
  version: string,
  workDir: string,
): Promise<ArchDigestResult> {
  const v = normalizeVersion(version);
  const assets = await fetchReleaseAssets(version);
  const wanted: Record<"arm" | "intel", string> = {
    arm: `NodeSpace_${v}_${ARCH_SUFFIX.arm}.dmg`,
    intel: `NodeSpace_${v}_${ARCH_SUFFIX.intel}.dmg`,
  };

  const result: ArchDigestResult = { missing: [] };
  for (const [arch, fileName] of Object.entries(wanted) as ["arm" | "intel", string][]) {
    const asset = assets.find((a) => a.name === fileName);
    if (!asset) {
      result.missing.push(fileName);
      continue;
    }
    const sha256 = await downloadAndHash(asset.url, join(workDir, fileName));
    result[arch] = { arch, fileName, sha256 };
  }
  return result;
}

const CASK_HEADER = (version: string) => `cask "nodespace" do
  version "${version}"`;

// Stanza order matters to `brew style`/`brew audit --cask`: livecheck comes
// right after name/desc/homepage, before depends_on -- verified against
// `brew style --fix`'s own canonical reordering.
const LIVECHECK_BLOCK = `
  livecheck do
    url :url
    strategy :github_latest
  end`;

const CASK_FOOTER = (binaryPath: string) => `
  app "NodeSpace.app"
  binary "#{appdir}/NodeSpace.app/${binaryPath}"

  zap trash: [
    "~/.nodespace/bin",
    "~/.nodespace/logs",
    "~/Library/LaunchAgents/app.nodespace.daemon.plist",
  ]
end
`;

// The url stanza uses Ruby's `#{version}` interpolation for the filename
// too (matching the resolved digest's fileName, just not hardcoded to the
// current version) -- conventional cask style, and means a reader diffing
// this file sees the same pattern regardless of which version generated it.
const urlFileName = (arch: "arm" | "intel") => `NodeSpace_#{version}_${ARCH_SUFFIX[arch]}.dmg`;

/** Renders the full Casks/nodespace.rb content for the digests actually
 * available. Two architectures -> on_arm/on_intel branches; exactly one ->
 * a single top-level url/sha256 scoped with `depends_on arch:`. Never
 * renders a URL for an architecture with no confirmed digest. */
export function renderCask(version: string, digests: ArchDigestResult): string {
  const v = normalizeVersion(version);
  if (!digests.arm && !digests.intel) {
    throw new Error(
      `no per-architecture digest available for v${v} — cannot render a cask (missing: ${digests.missing.join(", ")})`,
    );
  }

  const nameBlock = `  name "NodeSpace"
  desc "AI-native local-first knowledge management"
  homepage "https://nodespace.app/"`;

  if (digests.arm && digests.intel) {
    return `${CASK_HEADER(v)}

  on_arm do
    sha256 "${digests.arm.sha256}"
    url "https://github.com/${CORE_REPO}/releases/download/v#{version}/${urlFileName("arm")}"
  end
  on_intel do
    sha256 "${digests.intel.sha256}"
    url "https://github.com/${CORE_REPO}/releases/download/v#{version}/${urlFileName("intel")}"
  end

${nameBlock}
${LIVECHECK_BLOCK}

  depends_on macos: :${MIN_MACOS}
${CASK_FOOTER(CASK_BINARY_PATH)}`;
  }

  const only = (digests.arm ?? digests.intel) as ArchDigest;
  const archNote =
    only.arch === "arm"
      ? `  # v${v} ships an Apple Silicon build only -- ${digests.missing.join(", ")} is not\n  # in this release. See NodeSpaceAI/nodespace-core#2154.\n`
      : `  # v${v} ships an Intel build only -- ${digests.missing.join(", ")} is not\n  # in this release.\n`;

  return `${CASK_HEADER(v)}
  sha256 "${only.sha256}"

${archNote}  url "https://github.com/${CORE_REPO}/releases/download/v#{version}/${urlFileName(only.arch)}"
${nameBlock}
${LIVECHECK_BLOCK}

  depends_on macos: :${MIN_MACOS}
  depends_on arch:  :${only.arch === "arm" ? "arm64" : "x86_64"}
${CASK_FOOTER(CASK_BINARY_PATH)}`;
}

/** Pure comparison — no network. Exported separately from checkTapDrift so
 * the decision logic is unit-testable without hitting GitHub. */
export function isVersionDrifted(tapVersion: string, latestReleaseVersion: string): boolean {
  return normalizeVersion(tapVersion) !== normalizeVersion(latestReleaseVersion);
}

export interface DriftCheckResult {
  ok: boolean;
  tapVersion: string;
  latestReleaseVersion: string;
}

export async function fetchTapCaskVersion(): Promise<string> {
  const res = await fetch(
    `https://raw.githubusercontent.com/${TAP_REPO}/main/Casks/nodespace.rb`,
  );
  if (!res.ok) {
    throw new Error(`failed to fetch tap cask: HTTP ${res.status}`);
  }
  const text = await res.text();
  const m = text.match(/^\s*version\s+"([^"]+)"/m);
  if (!m) throw new Error("could not find a version stanza in the tap's Casks/nodespace.rb");
  return m[1];
}

/** The `github_latest` API-backed release, not a raw tag scan -- avoids
 * picking up unrelated tags (e.g. this repo's `review-*` tooling tags). */
export async function fetchLatestReleaseVersion(): Promise<string> {
  const out =
    await $`gh release list --repo ${CORE_REPO} --json tagName,isLatest --jq '.[] | select(.isLatest) | .tagName'`.text();
  const tag = out.trim();
  if (!tag) throw new Error("could not determine the latest release from `gh release list`");
  return tag;
}

export async function checkTapDrift(): Promise<DriftCheckResult> {
  const [tapVersion, latestReleaseVersion] = await Promise.all([
    fetchTapCaskVersion(),
    fetchLatestReleaseVersion(),
  ]);
  return {
    ok: !isVersionDrifted(tapVersion, latestReleaseVersion),
    tapVersion,
    latestReleaseVersion,
  };
}

async function pushCaskUpdate(version: string, caskContent: string, token: string): Promise<void> {
  const workDir = mkdtempSync(join(tmpdir(), "homebrew-nodespace-push-"));
  try {
    const authUrl = `https://x-access-token:${token}@github.com/${TAP_REPO}.git`;
    await $`git clone --depth 1 ${authUrl} ${workDir}`.quiet();
    writeFileSync(join(workDir, "Casks", "nodespace.rb"), caskContent);

    await $`git -C ${workDir} add Casks/nodespace.rb`.quiet();
    const staged = await $`git -C ${workDir} diff --cached --quiet`.quiet().nothrow();
    if (staged.exitCode === 0) {
      console.log("Tap cask already matches -- nothing to push.");
      return;
    }

    const v = normalizeVersion(version);
    await $`git -C ${workDir} -c user.name="nodespace-release-bot" -c user.email="release-bot@nodespace.app" commit -m ${`Update cask to v${v} (automated release sync)`}`.quiet();
    await $`git -C ${workDir} push origin HEAD:main`.quiet();
    console.log(`Pushed cask update for v${v} to ${TAP_REPO}.`);
  } finally {
    rmSync(workDir, { recursive: true, force: true });
  }
}

function usage(): void {
  console.log(`Usage:
  bun run scripts/update-homebrew-cask.ts <version> [--push]
  bun run scripts/update-homebrew-cask.ts drift-check`);
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  const command = args[0];

  if (!command) {
    usage();
    process.exit(1);
  }

  if (command === "drift-check") {
    const r = await checkTapDrift();
    if (r.ok) {
      console.log(`Tap in sync: v${normalizeVersion(r.tapVersion)}`);
      return;
    }
    console.error(
      `TAP DRIFT: homebrew-nodespace's cask reports v${normalizeVersion(r.tapVersion)}, ` +
        `but the latest published release is v${normalizeVersion(r.latestReleaseVersion)}.\n` +
        `Fix: bun run scripts/update-homebrew-cask.ts ${r.latestReleaseVersion} --push`,
    );
    process.exit(1);
  }

  if (!/^v?\d+\.\d+\.\d+(-[a-zA-Z0-9.]+)?$/.test(command)) {
    usage();
    process.exit(1);
  }

  const push = args.includes("--push");
  const token = process.env.HOMEBREW_TAP_TOKEN;
  if (push && !token) {
    console.error(
      "HOMEBREW_TAP_TOKEN is not set -- required for --push (a PAT with contents:write on " +
        `${TAP_REPO}). Running without --push shows what would change.`,
    );
    process.exit(1);
  }

  const workDir = mkdtempSync(join(tmpdir(), "nodespace-cask-assets-"));
  try {
    const digests = await resolveArchDigests(command, workDir);
    if (digests.missing.length > 0) {
      console.warn(
        `⚠ missing release assets, cask will omit those architectures: ${digests.missing.join(", ")}`,
      );
    }
    const content = renderCask(command, digests);
    console.log("--- Casks/nodespace.rb ---");
    console.log(content);

    if (!push) {
      console.log("(dry run -- pass --push with HOMEBREW_TAP_TOKEN set to publish this)");
      return;
    }
    await pushCaskUpdate(command, content, token as string);
  } finally {
    rmSync(workDir, { recursive: true, force: true });
  }
}

if (import.meta.main) {
  await main();
}
