// Covers the offline, deterministic parts of scripts/publish-install-script.ts:
// the version-pin string transform and tag normalization. The
// GitHub-talking functions (checkReleaseAssets, fetchWebsiteInstallScript,
// pushInstallScriptUpdate) are intentionally not exercised here -- this
// suite runs as part of `bun run test:scripts` / `test:all` (the pre-push
// gate), which must stay fast and deterministic, not depend on network or
// `gh` auth. See scripts/update-homebrew-cask.test.ts for the same pattern
// applied to the sibling cask-sync script.
import { describe, expect, test } from "bun:test";
import { normalizeTag, pinVersion } from "./publish-install-script";

describe("normalizeTag", () => {
  test("adds a leading v when missing", () => {
    expect(normalizeTag("0.2.0")).toBe("v0.2.0");
  });

  test("leaves an already-prefixed tag untouched", () => {
    expect(normalizeTag("v0.2.0")).toBe("v0.2.0");
  });
});

describe("pinVersion", () => {
  const fixture = [
    "#!/bin/sh",
    "set -eu",
    "",
    "NODESPACE_CLI_VERSION=\"v0.1.6\"",
    "",
    'NS_REPO="NodeSpaceAI/nodespace-core"',
    "",
  ].join("\n");

  test("replaces the pin line with a normalized tag", () => {
    const updated = pinVersion(fixture, "0.2.0");
    expect(updated).toContain('NODESPACE_CLI_VERSION="v0.2.0"');
    expect(updated).not.toContain('NODESPACE_CLI_VERSION="v0.1.6"');
  });

  test("accepts a version already carrying a leading v", () => {
    const updated = pinVersion(fixture, "v0.3.0");
    expect(updated).toContain('NODESPACE_CLI_VERSION="v0.3.0"');
  });

  test("leaves every other line untouched", () => {
    const updated = pinVersion(fixture, "0.2.0");
    expect(updated).toContain("#!/bin/sh");
    expect(updated).toContain('NS_REPO="NodeSpaceAI/nodespace-core"');
  });

  test("is idempotent -- pinning to the version already present is a no-op string-wise", () => {
    const once = pinVersion(fixture, "0.1.6");
    expect(once).toBe(fixture);
  });

  test("throws rather than silently no-op-ing when the pin marker is missing", () => {
    const noMarker = "#!/bin/sh\necho hello\n";
    expect(() => pinVersion(noMarker, "0.2.0")).toThrow(/could not find/);
  });

  test("matches the real install.sh pin line shape", () => {
    // Guards against the regex and the actual committed line in
    // nodespace-website's install.sh silently drifting apart.
    const real = 'NODESPACE_CLI_VERSION="v0.2.0"\n';
    const updated = pinVersion(real, "0.3.0");
    expect(updated).toBe('NODESPACE_CLI_VERSION="v0.3.0"\n');
  });
});
