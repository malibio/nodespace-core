#!/usr/bin/env bun
/**
 * npm dependency-advisory gate (the `bun audit` half of the CI audit workflow).
 *
 * Runs `bun audit` and FAILS on any advisory that is not in the ALLOWLIST below.
 * Every allowlisted entry is a dev/build/test-only dependency (never shipped to
 * users) or a framework-pinned low-severity issue whose only fix is an upstream
 * major bump — each tracked for follow-up. A NEW advisory, or any advisory in a
 * production dependency, is NOT allowlisted and fails the gate. Production
 * advisories are fixed via `overrides` in the root package.json, not ignored.
 *
 * Mirrors the Rust side's `.cargo/audit.toml`.
 */

// GHSA id -> justification. Keep this list minimal; prefer fixing via overrides.
const ALLOWLIST: Record<string, string> = {
  // cookie — pinned to ^0.6.0 by @sveltejs/kit itself; low severity, needs a
  // SvelteKit upstream bump to advance its cookie dependency past 0.7.
  "GHSA-pxg6-pf52-xh8x":
    "cookie pinned by @sveltejs/kit ^0.6.0 (low sev, upstream fix)",
};

const proc = Bun.spawnSync(["bun", "audit", "--json"]);
const out = proc.stdout.toString().trim();

// Empty stdout: `bun audit` exits non-zero both when advisories are found AND
// when it fails to run (registry unreachable, tool error, etc.). Only treat an
// empty result as clean when the tool itself succeeded — otherwise FAIL CLOSED
// so a broken audit never passes the gate green having scanned nothing.
if (!out) {
  if (proc.exitCode !== 0) {
    console.error(
      "bun audit failed to run (no JSON output):\n" + proc.stderr.toString(),
    );
    process.exit(1);
  }
  console.log("✅ bun audit clean (no advisories).");
  process.exit(0);
}

let data: Record<string, unknown>;
try {
  data = JSON.parse(out);
} catch {
  console.error("bun audit did not produce parseable JSON:\n" + out);
  process.exit(1);
}

const unexpected: string[] = [];
for (const [pkg, advs] of Object.entries(data)) {
  const list = Array.isArray(advs) ? advs : [advs];
  for (const a of list as Array<{
    url?: string;
    severity?: string;
    title?: string;
  }>) {
    const id = (a.url ?? "").split("/").pop() ?? "";
    if (!ALLOWLIST[id]) {
      unexpected.push(
        `  ${a.severity ?? "?"}  ${pkg}  ${id}  ${a.title ?? ""}`,
      );
    }
  }
}

if (unexpected.length > 0) {
  console.error(
    `\n❌ ${unexpected.length} un-allowlisted npm advisory(ies):\n${unexpected.join("\n")}`,
  );
  console.error(
    "\nFix it (prefer a pinned version via `overrides` in the root package.json). " +
      "Only if it is genuinely dev/build/test-only, add it to ALLOWLIST in scripts/audit-npm.ts with a justification.",
  );
  process.exit(1);
}

console.log(
  `✅ bun audit clean (${Object.keys(ALLOWLIST).length} accepted dev/build-only advisories allowlisted).`,
);
process.exit(0);
