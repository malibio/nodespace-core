// Covers the offline, deterministic parts of scripts/update-homebrew-cask.ts:
// digest hashing, cask rendering for both single- and dual-architecture
// releases, and the tap-drift comparison predicate. The GitHub-talking
// functions (fetchReleaseAssets, downloadAndHash, checkTapDrift,
// pushCaskUpdate) are intentionally not exercised here -- this suite runs as
// part of `bun run test:scripts` / `test:all` (the pre-push gate), which
// must stay fast and deterministic, not depend on network or `gh` auth.
import { describe, expect, test } from "bun:test";
import {
  type ArchDigestResult,
  isVersionDrifted,
  normalizeVersion,
  renderCask,
  sha256Hex,
} from "./update-homebrew-cask";

describe("normalizeVersion", () => {
  test("strips a leading v", () => {
    expect(normalizeVersion("v0.2.0")).toBe("0.2.0");
  });

  test("leaves a bare version untouched", () => {
    expect(normalizeVersion("0.2.0")).toBe("0.2.0");
  });
});

describe("sha256Hex", () => {
  test("matches `shasum -a 256` for a known input", () => {
    // `printf 'nodespace-test-fixture' | shasum -a 256`
    const bytes = new TextEncoder().encode("nodespace-test-fixture");
    expect(sha256Hex(bytes)).toBe(
      "cd028627062b027682af9676d7b1901b1b4ea0aea9f055f780a74ed3f252ad18",
    );
  });
});

describe("renderCask", () => {
  const armDigest = {
    arch: "arm" as const,
    fileName: "NodeSpace_0.2.0_aarch64.dmg",
    sha256: "a".repeat(64),
  };
  const intelDigest = {
    arch: "intel" as const,
    fileName: "NodeSpace_0.2.0_x64.dmg",
    sha256: "b".repeat(64),
  };

  test("renders a single top-level url/sha256 with depends_on arch when only one architecture is available", () => {
    const digests: ArchDigestResult = { arm: armDigest, missing: ["NodeSpace_0.2.0_x64.dmg"] };
    const cask = renderCask("v0.2.0", digests);

    expect(cask).toContain('version "0.2.0"');
    expect(cask).toContain(`sha256 "${armDigest.sha256}"`);
    expect(cask).toContain("NodeSpace_#{version}_aarch64.dmg");
    expect(cask).toContain("depends_on arch:  :arm64");
    expect(cask).not.toContain("on_arm do");
    expect(cask).not.toContain(intelDigest.sha256);
  });

  test("renders on_arm/on_intel branches when both architectures are available", () => {
    const digests: ArchDigestResult = { arm: armDigest, intel: intelDigest, missing: [] };
    const cask = renderCask("v0.2.0", digests);

    expect(cask).toContain("on_arm do");
    expect(cask).toContain("on_intel do");
    expect(cask).toContain(`sha256 "${armDigest.sha256}"`);
    expect(cask).toContain(`sha256 "${intelDigest.sha256}"`);
    expect(cask).not.toContain("depends_on arch:");
    // `brew style` requires a blank line between the sha256 and url stanzas
    // even nested inside on_arm/on_intel (Cask/StanzaGrouping) -- verified
    // against a real `brew style` run in an isolated Homebrew prefix.
    expect(cask).toMatch(/sha256 "[a-f0-9]{64}"\n\n {4}url /);
  });

  test("always points the binary stanza at Contents/MacOS/nodespace", () => {
    const digests: ArchDigestResult = { arm: armDigest, missing: [] };
    expect(renderCask("v0.2.0", digests)).toContain(
      'binary "#{appdir}/NodeSpace.app/Contents/MacOS/nodespace"',
    );
  });

  test("always includes a github_latest livecheck block", () => {
    const digests: ArchDigestResult = { arm: armDigest, missing: [] };
    const cask = renderCask("v0.2.0", digests);
    expect(cask).toContain("livecheck do");
    expect(cask).toContain("strategy :github_latest");
  });

  test("throws when no architecture has a digest", () => {
    const digests: ArchDigestResult = {
      missing: ["NodeSpace_0.2.0_aarch64.dmg", "NodeSpace_0.2.0_x64.dmg"],
    };
    expect(() => renderCask("v0.2.0", digests)).toThrow(/no per-architecture digest/);
  });
});

describe("isVersionDrifted", () => {
  test("false when versions match, with or without a leading v", () => {
    expect(isVersionDrifted("0.2.0", "v0.2.0")).toBe(false);
    expect(isVersionDrifted("v0.2.0", "0.2.0")).toBe(false);
  });

  test("true when the tap is behind the latest release", () => {
    expect(isVersionDrifted("v0.1.6", "v0.2.0")).toBe(true);
  });
});
