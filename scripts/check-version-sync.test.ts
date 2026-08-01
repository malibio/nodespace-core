// Enforces the app-version single source of truth (nodespace-core#1686). If any of
// the three app-version fields drifts from the canonical tauri.conf.json, this test
// fails under `bun test scripts/` (part of `test:all`) so the pre-push gate catches
// it before a stale-versioned build can ship.
import { describe, expect, test } from "bun:test";
import { CANONICAL, checkAppVersionSync } from "./check-version-sync";

describe("app version single source of truth (#1686)", () => {
  test("all app-version fields equal the canonical tauri.conf.json", () => {
    const r = checkAppVersionSync();
    // A readable failure lists exactly which field drifted.
    expect(r.mismatches).toEqual([]);
    expect(r.ok).toBe(true);
  });

  test("canonical version is a valid semver x.y.z", () => {
    const r = checkAppVersionSync();
    expect(r.canonical).toMatch(/^\d+\.\d+\.\d+$/);
  });

  test("the guard covers all three known version fields", () => {
    const r = checkAppVersionSync();
    expect(Object.keys(r.versions).sort()).toEqual(
      ["Cargo.toml", "package.json", "tauri.conf.json"].sort(),
    );
    expect(r.versions[CANONICAL]).toBeDefined();
  });
});
