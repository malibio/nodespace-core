#!/usr/bin/env bun

/**
 * Local pre-push test gate.
 *
 * Runs the full test pyramid (frontend, skill, Rust, e2e) before code leaves
 * the machine. This repo has no CI runner for tests — this hook is the only
 * gate. See ADR-047.
 *
 * This gate only activates once Husky has wired it in via the `prepare`
 * script (i.e. after `bun install`). It is a local convenience, not a
 * server-side enforcement backstop — a push from a machine that never ran
 * `bun install` is not gated.
 *
 * Bypass: git push --no-verify. Reserved for WIP Handoff Commits (see
 * CLAUDE.md) — multi-session work, approaching context limits, a natural
 * breakpoint, or before a risky change. Not a general-purpose escape hatch
 * for "the suite is slow right now."
 */

import { $ } from "bun";

async function run(label: string, cmd: () => Promise<unknown>) {
  console.log(`\n▶ ${label}`);
  try {
    await cmd();
  } catch {
    console.error(`\n✗ ${label} failed — push blocked.`);
    console.error("  Fix the failure, or if this is a WIP Handoff Commit (see CLAUDE.md),");
    console.error("  bypass with: git push --no-verify\n");
    process.exit(1);
  }
}

await run("bun run test:all (frontend + skill + Rust)", () => $`bun run test:all`);
await run("cargo build --bin nodespaced (e2e harness daemon)", () => $`cargo build --bin nodespaced`);
await run("bun run test:e2e (headless daemon round-trip)", () => {
  const binaryName = process.platform === "win32" ? "nodespaced.exe" : "nodespaced";
  const binary = `${process.cwd()}/target/debug/${binaryName}`;
  return $`bun run test:e2e`.env({ ...process.env, NODESPACED_BINARY: binary });
});

console.log("\n✓ All tests passed — pushing.\n");
