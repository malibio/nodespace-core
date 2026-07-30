/**
 * Unit tests for runner.ts's pure scoring/reporting helpers — the uniformity
 * guard, empty-generation exclusion, and raw-output trace assembly.
 *
 * Runs via `bun run test:scripts` (and so under `bun run test:all`). Deliberate
 * exception to the project-wide "never use `bun test`" rule: this file touches
 * no DOM, and cannot run under Vitest anyway (imports `bun:test`, and scripts/
 * is outside every Vitest project glob).
 *
 * These specifically exist because the diagnostic behavior they protect —
 * "fail loudly on impossible uniformity" and "exclude degenerate empty
 * generations from the denominator" — is itself the thing that stops a false
 * result from looking like a real one. A bug in the guard is exactly the kind
 * of harness defect this eval exists to prevent, so it needs a test that runs
 * with no model and no daemon.
 */

import { describe, expect, test } from "bun:test";
import {
  buildTraceLines,
  checkUniformity,
  partitionExcluded,
} from "./runner.ts";
import { EnvironmentError } from "./preflight.ts";
import type { ScenarioResult } from "./types.ts";

function result(overrides: Partial<ScenarioResult> = {}): ScenarioResult {
  return {
    id: "1",
    scenario: "test scenario",
    prompt: "hi",
    passed: true,
    turns: [
      {
        toolsOffered: "",
        toolsCalled: [],
        reply: "hello",
        latencyMs: 100,
      },
    ],
    ...overrides,
  };
}

describe("partitionExcluded", () => {
  test("scored results with no exclusions pass through unchanged", () => {
    const results = [result({ id: "1" }), result({ id: "2", passed: false })];
    const { scored, excludedCount } = partitionExcluded(results);
    expect(scored).toHaveLength(2);
    expect(excludedCount).toBe(0);
  });

  test("excludes scenarios flagged as empty generations", () => {
    const results = [
      result({ id: "1" }),
      result({ id: "2", excludedAsEmptyGeneration: true, passed: false }),
      result({ id: "3" }),
    ];
    const { scored, excludedCount } = partitionExcluded(results);
    expect(scored.map((r) => r.id)).toEqual(["1", "3"]);
    expect(excludedCount).toBe(1);
  });

  test("all-excluded run scores zero total, not zero-of-N", () => {
    const results = [
      result({ id: "1", excludedAsEmptyGeneration: true, passed: false }),
      result({ id: "2", excludedAsEmptyGeneration: true, passed: false }),
    ];
    const { scored, excludedCount } = partitionExcluded(results);
    expect(scored).toHaveLength(0);
    expect(excludedCount).toBe(2);
  });
});

describe("checkUniformity", () => {
  test("below the minimum scenario count, never flags — even 0/1 or 1/1", () => {
    expect(checkUniformity(0, 1)).toBeNull();
    expect(checkUniformity(1, 1)).toBeNull();
    expect(checkUniformity(0, 3)).toBeNull();
    expect(checkUniformity(3, 3)).toBeNull();
  });

  test("a genuine partial result at or above the minimum is not flagged", () => {
    expect(checkUniformity(2, 4)).toBeNull();
    expect(checkUniformity(9, 12)).toBeNull();
  });

  test("all-failed at or above the minimum is flagged as an environment error", () => {
    const err = checkUniformity(0, 4);
    expect(err).toBeInstanceOf(EnvironmentError);
    expect(err?.message).toContain("FAILED");
    expect(err?.message).toContain("0/4");
  });

  test("all-passed at or above the minimum is flagged as an environment error", () => {
    const err = checkUniformity(12, 12);
    expect(err).toBeInstanceOf(EnvironmentError);
    expect(err?.message).toContain("PASSED");
    expect(err?.message).toContain("12/12");
  });

  test("a custom minimum is honored", () => {
    expect(checkUniformity(0, 2, 2)).toBeInstanceOf(EnvironmentError);
    expect(checkUniformity(0, 2, 3)).toBeNull();
  });

  test("total of zero never divides or flags", () => {
    expect(checkUniformity(0, 0)).toBeNull();
  });
});

describe("buildTraceLines", () => {
  test("produces no lines when no turn captured raw output", () => {
    const results = [result({ id: "1" }), result({ id: "2" })];
    expect(buildTraceLines(results)).toEqual([]);
  });

  test("includes only turns that captured raw output, in order", () => {
    const results = [
      result({
        id: "1",
        turns: [
          {
            toolsOffered: "",
            toolsCalled: ["create_node"],
            reply: "done",
            latencyMs: 50,
            rawOutput: "[iteration 0] create_node(...)",
            routingDecision: "query",
            stage2CandidatesInjected: true,
          },
        ],
      }),
      result({ id: "2" }), // no rawOutput — contributes nothing
    ];
    const lines = buildTraceLines(results);
    expect(lines).toHaveLength(1);
    expect(lines[0]).toMatchObject({
      scenarioId: "1",
      turnIndex: 0,
      isPriorContext: false,
      rawOutput: "[iteration 0] create_node(...)",
      routingDecision: "query",
      stage2CandidatesInjected: true,
      toolsCalled: ["create_node"],
    });
  });

  test("marks all but the last turn as prior context", () => {
    const results = [
      result({
        id: "1",
        turns: [
          {
            toolsOffered: "",
            toolsCalled: [],
            reply: "context turn",
            latencyMs: 10,
            rawOutput: "raw 0",
          },
          {
            toolsOffered: "",
            toolsCalled: [],
            reply: "scored turn",
            latencyMs: 20,
            rawOutput: "raw 1",
          },
        ],
      }),
    ];
    const lines = buildTraceLines(results);
    expect(lines).toHaveLength(2);
    expect(lines[0].isPriorContext).toBe(true);
    expect(lines[1].isPriorContext).toBe(false);
  });
});
