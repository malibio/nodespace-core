/**
 * Unit tests for preflight.ts's pure parsing helpers.
 *
 * Runs via `bun run test:scripts` (and so under `bun run test:all`). Deliberate
 * exception to the project-wide "never use `bun test`" rule: this file touches
 * no DOM, and cannot run under Vitest anyway (imports `bun:test`, and scripts/
 * is outside every Vitest project glob).
 */

import { describe, expect, test } from "bun:test";
import { awaitSkillIndex, extractSeedEntries, seededSkillCount } from "./preflight.ts";

describe("extractSeedEntries", () => {
  test("extracts key and version from seeded nodes", () => {
    const json = JSON.stringify({
      nodes: [
        {
          properties: { _seed: { key: "node-creation", version: "3" } },
        },
        {
          properties: { _seed: { key: "graph-editing", version: "1" } },
        },
      ],
    });
    expect(extractSeedEntries(json)).toEqual([
      { key: "node-creation", version: "3" },
      { key: "graph-editing", version: "1" },
    ]);
  });

  test("skips nodes with no _seed metadata", () => {
    const json = JSON.stringify({
      nodes: [
        { properties: { _seed: { key: "a", version: "1" } } },
        { properties: {} },
        { properties: { _seed: {} } },
      ],
    });
    expect(extractSeedEntries(json)).toEqual([{ key: "a", version: "1" }]);
  });

  test("falls back to a placeholder when version is missing", () => {
    const json = JSON.stringify({
      nodes: [{ properties: { _seed: { key: "a" } } }],
    });
    expect(extractSeedEntries(json)).toEqual([
      { key: "a", version: "(no version)" },
    ]);
  });

  test("returns an empty list for no nodes", () => {
    expect(extractSeedEntries(JSON.stringify({ nodes: [] }))).toEqual([]);
  });

  test("returns an empty list rather than throwing on unparseable JSON", () => {
    expect(extractSeedEntries("not json")).toEqual([]);
  });

  test("returns an empty list when the nodes key is absent", () => {
    expect(extractSeedEntries(JSON.stringify({}))).toEqual([]);
  });
});

describe("awaitSkillIndex", () => {
  // Embeddings run on a ~30s debounce, so a daemon started against a purged
  // database serves its first turns with an EMPTY skill index. Stage-2 then
  // fails open to the full tool surface with no skill guidance, and the
  // malformed write that follows cascades through the rest of its group —
  // measured live: create_schema missing its required top-level `name`, then
  // create_node naming a type that was never created.
  const env = {
    nsBin: "x",
    socket: "s",
    log: "l",
    model: "m",
    timeoutMs: 1,
    aichat: "a",
  };
  const noSleep = () => {};

  test("returns immediately when every seeded skill is already retrievable", () => {
    let probes = 0;
    awaitSkillIndex(
      env,
      60_000,
      () => {
        probes++;
        return 8;
      },
      noSleep,
      undefined,
      8,
    );
    expect(probes).toBe(1);
  });

  test("waits for an index that populates late, then proceeds", () => {
    const counts = [0, 0, 8];
    let i = 0;
    let slept = 0;
    expect(() =>
      awaitSkillIndex(
        env,
        60_000,
        () => counts[Math.min(i++, counts.length - 1)]!,
        () => {
          slept++;
        },
        undefined,
        8,
      ),
    ).not.toThrow();
    expect(slept).toBe(2);
  });

  test("throws when the index never populates", () => {
    // A clock driven by the sleep callback, so the timeout is reached without
    // waiting in real time.
    let t = 0;
    expect(() =>
      awaitSkillIndex(
        env,
        10_000,
        () => 0,
        () => {
          t += 4_000;
        },
        () => t,
        8,
      ),
    ).toThrow(/seeded skills are semantically retrievable/);
  });

  // The denominator is read from the database, not hardcoded, so adding a
  // ninth skill cannot silently make the gate under-wait. A run whose index
  // holds 8 of 9 must still be treated as not ready.
  test("waits for every seeded skill, not a hardcoded eight", () => {
    let t = 0;
    expect(() =>
      awaitSkillIndex(
        env,
        10_000,
        () => 8,
        () => {
          t += 4_000;
        },
        () => t,
        9,
      ),
    ).toThrow(/Only 8 of 9/);
  });

  test("treats a partially-populated index as not ready", () => {
    // 7 of 8 is the quieter form of the same defect: if the missing one is
    // Graph Editing, every update scenario scores against a surface that never
    // offered update_node.
    let t = 0;
    expect(() =>
      awaitSkillIndex(
        env,
        10_000,
        () => 7,
        () => {
          t += 4_000;
        },
        () => t,
        8,
      ),
    ).toThrow(/Only 7 of 8/);
  });
});

describe("seededSkillCount", () => {
  const env = {
    nsBin: "x",
    socket: "s",
    log: "l",
    model: "m",
    timeoutMs: 1,
    aichat: "a",
  };

  test("uses the enumerated row count as the denominator", () => {
    expect(seededSkillCount(env, () => 9)).toBe(9);
  });

  // A CLI hiccup is not evidence about how many skills exist, so it degrades
  // to the known seed count rather than to a denominator of zero (which would
  // wave the gate through against an empty index).
  test("falls back to the known seed count when enumeration fails", () => {
    expect(seededSkillCount(env, () => null)).toBe(8);
  });

  // A SUCCESSFUL enumeration returning zero is the opposite situation: the
  // rows are inserted synchronously at startup, so zero of them means seeding
  // never ran. Substituting the fallback made the gate poll for eight skills
  // that cannot appear, burning the full 120s timeout before reporting a
  // generic "index not ready" — the wrong fault, reported late.
  test("reports broken seeding instead of waiting for skills that cannot appear", () => {
    expect(() => seededSkillCount(env, () => 0)).toThrow(/zero skill nodes/);
  });
});
