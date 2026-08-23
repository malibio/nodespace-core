/**
 * Unit tests for preflight.ts's pure parsing helpers.
 *
 * Runs via `bun run test:scripts` (and so under `bun run test:all`). Deliberate
 * exception to the project-wide "never use `bun test`" rule: this file touches
 * no DOM, and cannot run under Vitest anyway (imports `bun:test`, and scripts/
 * is outside every Vitest project glob).
 */

import { describe, expect, test } from "bun:test";
import { awaitSkillIndex, extractSeedEntries } from "./preflight.ts";

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
      ),
    ).toThrow(/seeded skills are semantically retrievable/);
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
      ),
    ).toThrow(/Only 7 of 8/);
  });
});
