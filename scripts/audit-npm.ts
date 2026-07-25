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
  // happy-dom — the test-only DOM environment. Never shipped to users.
  "GHSA-37j7-fg3j-429f": "happy-dom (dev test DOM, not shipped)",
  "GHSA-6q6h-j7hj-3r64": "happy-dom (dev test DOM, not shipped)",
  "GHSA-w4gp-fjgq-3q4g": "happy-dom (dev test DOM, not shipped)",
  // brace-expansion — pulled only through eslint's dependency tree (dev linting).
  "GHSA-3jxr-9vmj-r5cp": "brace-expansion via eslint (dev tooling)",
  "GHSA-jxxr-4gwj-5jf2": "brace-expansion via eslint (dev tooling)",
  "GHSA-mh99-v99m-4gvg": "brace-expansion via eslint (dev tooling)",
  // vite / launch-editor — build-time dev server, not part of the shipped runtime.
  "GHSA-4w7w-66w2-5vf9": "vite (build/dev server, not shipped)",
  "GHSA-fx2h-pf6j-xcff": "vite (build/dev server, not shipped)",
  "GHSA-p9ff-h696-f583": "vite (build/dev server, not shipped)",
  "GHSA-v6wh-96g9-6wx3": "launch-editor via vite (build/dev, not shipped)",
  // vitest / @vitest/browser — the test runner. Not shipped.
  "GHSA-5xrq-8626-4rwp": "vitest UI (dev test runner, not shipped)",
  "GHSA-p63j-vcc4-9vmv": "@vitest/browser (dev test runner, not shipped)",
  // esbuild — build tool. Not part of the shipped runtime.
  "GHSA-67mh-4wv8-2f99": "esbuild (build tool, not shipped)",
  // cookie — pinned to ^0.6.0 by @sveltejs/kit itself; low severity, needs a
  // SvelteKit upstream bump to advance its cookie dependency past 0.7.
  "GHSA-pxg6-pf52-xh8x":
    "cookie pinned by @sveltejs/kit ^0.6.0 (low sev, upstream fix)",
  // dompurify — pinned to 3.4.7, the highest version DOMPurify still sanitizes
  // correctly under Happy-DOM (our unit-test DOM); >3.4.7 fails-open in Happy-DOM
  // and would silently drop sanitizeSvg's coverage. DOMPurify sanitizes BOTH
  // mermaid SVG (mermaid-render.ts) AND — primarily — LLM/agent chat markdown
  // (chat-markdown.svelte), so treat any future dompurify advisory seriously.
  // 3.4.7 already fixes the cross-realm / IN_PLACE / prototype-pollution issues;
  // each residual below requires a DOMPurify config option that NEITHER consumer
  // sets (setConfig / CUSTOM_ELEMENT_HANDLING / SAFE_FOR_TEMPLATES+RETURN_DOM /
  // RETURN_TRUSTED_TYPE), so none is reachable in this app's usage. Follow-up:
  // move sanitizeSvg to browser-mode tests, then bump dompurify to latest.
  "GHSA-cmwh-pvxp-8882":
    "dompurify 3.4.7 (Happy-DOM ceiling; defense-in-depth, upstream fix)",
  "GHSA-c2j3-45gr-mqc4":
    "dompurify 3.4.7 (Happy-DOM ceiling; defense-in-depth, upstream fix)",
  "GHSA-vxr8-fq34-vvx9":
    "dompurify 3.4.7 (Happy-DOM ceiling; defense-in-depth, upstream fix)",
  "GHSA-gvmj-g25r-r7wr":
    "dompurify 3.4.7 (Happy-DOM ceiling; defense-in-depth, upstream fix)",
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
