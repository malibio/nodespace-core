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
  parseTurnOutput,
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

describe("parseTurnOutput", () => {
  test("parses tool calls, reply, and latency from ordinary output", () => {
    const out = [
      "[tools offered] create_node, search_nodes",
      "[tool] create_node [fields=3] {}",
      "assistant> Done — created the node.",
    ].join("\n");
    const turn = parseTurnOutput(out, 1234);
    // The marker was already present in this fixture but never asserted, which
    // is part of why an always-empty `toolsOffered` reached committed traces
    // unnoticed.
    expect(turn.toolsOffered).toBe("create_node, search_nodes");
    expect(turn.toolsCalled).toEqual(["create_node"]);
    expect(turn.toolCalls).toEqual([
      { name: "create_node", isError: false, fieldCount: 3 },
    ]);
    expect(turn.reply).toBe("Done — created the node.");
    expect(turn.latencyMs).toBe(1234);
  });

  test("preserves a multiline raw generation intact (regression)", () => {
    const text = "line one\nline two\nline three";
    const out = `[raw] iteration=0 ${JSON.stringify(text)}\nassistant> ok`;
    const turn = parseTurnOutput(out, 100);
    expect(turn.rawOutput).toBe(`[iteration 0] ${text}`);
  });

  test("does not truncate raw output containing a literal [tool] marker (regression)", () => {
    const text = "I'll use [tool] create_node to do that.";
    const out = [
      `[raw] iteration=0 ${JSON.stringify(text)}`,
      "[tool] create_node [fields=1] {}",
      "assistant> done",
    ].join("\n");
    const turn = parseTurnOutput(out, 100);
    expect(turn.rawOutput).toBe(`[iteration 0] ${text}`);
    // The real tool call after it must still be parsed, not swallowed.
    expect(turn.toolsCalled).toEqual(["create_node"]);
  });

  test("joins multiple iterations of raw output in order", () => {
    const out = [
      `[raw] iteration=0 ${JSON.stringify("first\nresponse")}`,
      `[raw] iteration=1 ${JSON.stringify("second response")}`,
      "assistant> done",
    ].join("\n");
    const turn = parseTurnOutput(out, 100);
    expect(turn.rawOutput).toBe(
      "[iteration 0] first\nresponse\n[iteration 1] second response",
    );
  });

  test("rawOutput is undefined when no [raw] lines are present", () => {
    const turn = parseTurnOutput("assistant> hi", 100);
    expect(turn.rawOutput).toBeUndefined();
  });

  test("parses routing decision and stage2 injection markers", () => {
    const out = ["[routing] query", "[stage2 injected] true", "assistant> ok"].join(
      "\n",
    );
    const turn = parseTurnOutput(out, 100);
    expect(turn.routingDecision).toBe("query");
    expect(turn.stage2CandidatesInjected).toBe(true);
  });

  test("parses the routed-skills marker", () => {
    const out = [
      "[routed skills] equipment_log, checkout_flow",
      "assistant> ok",
    ].join("\n");
    const turn = parseTurnOutput(out, 100);
    expect(turn.routedSkills).toBe("equipment_log, checkout_flow");
  });

  test("routedSkills is undefined (not empty) when the marker is absent", () => {
    // "not recorded" must stay distinguishable from "nothing routed" — a turn
    // from an older results file has no marker at all, and reporting that as
    // an empty routed set would assert something the run never observed.
    const turn = parseTurnOutput("assistant> hi", 100);
    expect(turn.routedSkills).toBeUndefined();
  });

  test("parses stage2 injected as false, distinct from absent", () => {
    const out = ["[stage2 injected] false", "assistant> ok"].join("\n");
    const turn = parseTurnOutput(out, 100);
    expect(turn.stage2CandidatesInjected).toBe(false);
  });

  test("marks emptyGeneration from the marker", () => {
    const out = ["[empty-generation]", "assistant> (no assistant reply)"].join(
      "\n",
    );
    const turn = parseTurnOutput(out, 100);
    expect(turn.emptyGeneration).toBe(true);
  });

  test("emptyGeneration is undefined (not false) when the marker is absent", () => {
    const turn = parseTurnOutput("assistant> hi", 100);
    expect(turn.emptyGeneration).toBeUndefined();
  });
});
