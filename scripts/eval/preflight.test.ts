/**
 * Unit tests for preflight.ts's pure parsing helpers.
 *
 * Runs via `bun run test:scripts` (and so under `bun run test:all`). Deliberate
 * exception to the project-wide "never use `bun test`" rule: this file touches
 * no DOM, and cannot run under Vitest anyway (imports `bun:test`, and scripts/
 * is outside every Vitest project glob).
 */

import { describe, expect, test } from "bun:test";
import { extractSeedEntries } from "./preflight.ts";

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
