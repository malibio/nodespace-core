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
await run("cargo test -p nodespace-app (Tauri-seam integration tests, ADR-048)", () => {
  const binaryName = process.platform === "win32" ? "nodespaced.exe" : "nodespaced";
  const binary = `${process.cwd()}/target/debug/${binaryName}`;
  // --test-threads=1: every test in this suite spawns a real nodespaced
  // process, which loads a real embedding model (Metal shader compilation
  // included) before its socket binds — far more load-sensitive than
  // nodespace-core's in-process assertions (which cap concurrency the same
  // way, see rust:test above, though that suite has no equivalent per-test
  // process-spawn cost). Cargo already runs each tests/*.rs file's binary
  // sequentially, but within one binary (e.g. node_crud_tauri_seam_test.rs's
  // 5 tests) all tests run concurrently by default. `SpawnedDaemon::spawn()`
  // happens BEFORE any test acquires test-support's CONNECT_MUTEX (which
  // only serializes the health-wait/connect step, not the spawn itself), so
  // without this flag several real daemon processes can be mid-spawn at
  // once fully uncoordinated — self-inflicted CPU/GPU contention this suite
  // creates on its own, made worse by whatever else is running on the
  // machine (see #1610). Tried --test-threads=2 first; it still reproduced
  // #1610's exact daemon-health timeout under real background load, so =1
  // was needed, not just a higher timeout. Serializing daemon spawns costs
  // almost nothing here — the suite's total wall-clock is dominated by the
  // one real-inference test (~25-40s), and the rest are sub-second each —
  // so =1 trades no meaningful time for real reliability.
  return $`cargo test -p nodespace-app -- --test-threads=1`.env({
    ...process.env,
    NODESPACED_TEST_BIN: binary,
  });
});

console.log("\n✓ All tests passed — pushing.\n");
