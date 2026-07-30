/**
 * Agent-behavior eval — end-to-end tool-call behavior.
 *
 * Asserts a structured, machine-checkable expectation per scenario rather than
 * capturing prose for a human to read: the right tool, the right number of
 * times, in the right order.
 *
 * This is the third eval layer, distinct from the other two:
 *   - chat::parser::tests           — tool-call PARSING shape (fixtures)
 *   - scripts/eval/fixtures/routing — skill ROUTING accuracy (which skill fires)
 *   - this                          — END-TO-END behavior (right tool, right
 *                                     count, right effect)
 *
 * Under ADR-038 the model calls search_skills before acting, so assertions
 * check for the TARGET tool tolerating search_skills, never raw tool count.
 *
 * Scenario wording must stay independent of packages/agent/src/agent_guidance.rs.
 * `guidance_is_not_contaminated_by_eval_prompts` enforces it by parsing the
 * `prompt:` literals out of this file: a prompt that also appears in guidance
 * turns a passing scenario into proof that the model can copy a memorized
 * example, and prompt tuning then has a degenerate solution.
 */

import type {
  EvalFixture,
  Scenario,
  ToolCallRecord,
  TurnRecord,
  Verdict,
} from "../types.ts";

// ---------------------------------------------------------------------------
// Structured expectation model
// ---------------------------------------------------------------------------

export type Expectation =
  // No graph-action tool fired at all (routing tools tolerated).
  | { kind: "noTools" }
  // The named tool fired exactly once (ignoring routing tools).
  | { kind: "toolOnce"; tool: string }
  // Tools fired in this order as a subsequence (ignoring routing tools) — other
  // tools may appear between/around them, but these must appear in this order.
  | { kind: "toolSequence"; tools: string[] }
  // The named tool did not fire more than once in a row (no blind retry loop).
  | { kind: "noRetry"; tool: string }
  // Exactly one create_schema call in this turn (no proactive related-type creation).
  | { kind: "noExtraTypes" };

/**
 * Tools that participate in ADR-038 pull-model routing but are not any
 * scenario's target action — tolerated anywhere in the sequence, never
 * asserted as "extra".
 *
 * Cross-referenced against Tool::ALL in
 * packages/agent/src/local_agent/tools.rs — update here if the registry changes.
 */
const ROUTING_TOOLS = ["search_skills"];

export function actionTools(toolsCalled: string[]): string[] {
  return toolsCalled.filter((t) => !ROUTING_TOOLS.includes(t));
}

/**
 * Check that a create_schema call actually produced a usable type.
 *
 * Counting the tool name is not enough, and the gap is not hypothetical: the
 * model has called create_schema with a title_template and no fields, been
 * rejected outright by title-template validation, and still scored a pass
 * because the name appeared once.
 *
 * Two distinct ways to call create_schema and end up with nothing usable:
 *   - the call is REJECTED (is_error) — nothing persisted at all;
 *   - the call SUCCEEDS with an empty field list. A call carrying neither
 *     `fields` nor `description` is valid by design and persists a type with no
 *     properties, against which the user cannot record anything. This one is
 *     invisible to any check that only looks at whether the call failed.
 *
 * `fieldCount` is absent (rather than 0) on results recorded before it was
 * captured, so absence is treated as unknown and passes — a stale baseline must
 * not read as a fresh failure.
 */
function schemaCallsAreSound(calls: ToolCallRecord[]): Verdict {
  for (const c of calls) {
    if (c.name !== "create_schema") continue;
    if (c.isError) {
      return {
        passed: false,
        failure:
          "create_schema was called but REJECTED — no schema persisted (the call " +
          "scores as a pass on tool name alone)",
      };
    }
    if (c.fieldCount === 0) {
      return {
        passed: false,
        failure:
          "create_schema succeeded but persisted a type with NO fields — nothing " +
          "can be recorded against it",
      };
    }
  }
  return { passed: true };
}

/**
 * Decide whether a turn met its expectation.
 *
 * Pure and daemon-free so it is unit-testable without a model — see
 * scripts/eval/fixtures/agent-matrix.test.ts, which runs in `bun run test:all`.
 */
export function assertExpectation(
  expect: Expectation,
  toolsCalled: string[],
  toolCalls: ToolCallRecord[] = [],
): Verdict {
  const actions = actionTools(toolsCalled);

  switch (expect.kind) {
    case "noTools": {
      if (actions.length > 0) {
        return {
          passed: false,
          failure: `Expected no graph-action tools, but got: ${actions.join(",")}`,
        };
      }
      return { passed: true };
    }

    case "toolOnce": {
      const count = actions.filter((t) => t === expect.tool).length;
      if (count !== 1) {
        return {
          passed: false,
          failure: `Expected '${expect.tool}' exactly once, got ${count} (tools: ${actions.join(",")})`,
        };
      }
      // Scenarios 8a/8b target create_schema through this branch, so the
      // count-only hole this closes for noExtraTypes exists here too.
      return schemaCallsAreSound(toolCalls);
    }

    case "toolSequence": {
      let idx = 0;
      for (const t of actions) {
        if (t === expect.tools[idx]) idx++;
        if (idx === expect.tools.length) break;
      }
      if (idx !== expect.tools.length) {
        return {
          passed: false,
          failure: `Expected sequence [${expect.tools.join(",")}] as a subsequence, got: ${actions.join(",")}`,
        };
      }
      return { passed: true };
    }

    case "noRetry": {
      let runLength = 0;
      for (const t of actions) {
        runLength = t === expect.tool ? runLength + 1 : 0;
        if (runLength > 1) {
          return {
            passed: false,
            failure: `Expected no repeated '${expect.tool}' calls (retry loop), got: ${actions.join(",")}`,
          };
        }
      }
      return { passed: true };
    }

    case "noExtraTypes": {
      const count = actions.filter((t) => t === "create_schema").length;
      if (count !== 1) {
        return {
          passed: false,
          failure: `Expected exactly one create_schema (no extra related types), got ${count} (tools: ${actions.join(",")})`,
        };
      }
      return schemaCallsAreSound(toolCalls);
    }
  }
}

// ---------------------------------------------------------------------------
// Scenarios
//
// Each group shares a chat node so later scenarios see earlier turns. Ids are
// the baseline join key and must stay stable; prompts may be reworded freely.
// ---------------------------------------------------------------------------

interface MatrixScenario extends Scenario {
  expect: Expectation;
}

const GROUPS: MatrixScenario[][] = [
  [
    {
      id: "1",
      scenario: "1. Greeting",
      prompt: "Hi there",
      expect: { kind: "noTools" },
    },
  ],
  [
    {
      id: "2",
      scenario: "2. Capability",
      prompt: "What can you do?",
      expect: { kind: "noTools" },
    },
  ],
  // Single-custom-type CRUD chain (scenarios 3-7) shares one chat node.
  [
    {
      id: "3",
      scenario: "3. Schema creation",
      // Mentions checked-out vs returned so the schema plausibly carries a
      // status field — scenario 6 then tests resolve_query routing on an
      // indirect reference, not whether the model guessed a status value.
      prompt:
        "I want to keep a record of the equipment my team checks out and whether it's been returned",
      expect: { kind: "noExtraTypes" },
    },
    {
      id: "4",
      scenario: "4. Instance creation",
      prompt:
        "Log a laser cutter checked out on the 12th, replacement cost 2400",
      expect: { kind: "toolOnce", tool: "create_node" },
    },
    {
      id: "5",
      scenario: "5. List/query",
      prompt: "What equipment is on the books?",
      expect: { kind: "toolOnce", tool: "search_nodes" },
    },
    {
      id: "6",
      scenario: "6. Update",
      // resolve_query performs the search internally and returns the resolved
      // node directly (see ADR-064 rule 4) — the model acts on it via
      // update_node without a separate search_nodes call of its own.
      prompt: "The 2400 one came back — set it to returned",
      expect: { kind: "toolSequence", tools: ["resolve_query", "update_node"] },
    },
    {
      id: "7",
      scenario: "7. Empty-result query",
      prompt: "Do we have anything worth 90000 sitting out?",
      expect: { kind: "noRetry", tool: "search_nodes" },
    },
  ],
  // Multi-custom-type CRUD (scenario 8) shares its own chat node.
  [
    {
      id: "8a",
      scenario: "8a. Create type: first",
      prompt: "Start tracking albums I mean to listen to",
      expect: { kind: "toolOnce", tool: "create_schema" },
    },
    {
      id: "8b",
      scenario: "8b. Create type: second",
      prompt: "I also need a tracker for the venues I book",
      expect: { kind: "toolOnce", tool: "create_schema" },
    },
    {
      id: "8c",
      scenario: "8c. Instance: first type",
      prompt: "Put down Kind of Blue, it's by Miles Davis",
      expect: { kind: "toolOnce", tool: "create_node" },
    },
    {
      id: "8d",
      scenario: "8d. Instance: second type",
      prompt:
        "New venue: the Blue Note, they can be reached at booking@example.com",
      expect: { kind: "toolOnce", tool: "create_node" },
    },
    {
      id: "8e",
      scenario: "8e. Query across types",
      prompt: "Run through the albums for me",
      expect: { kind: "toolOnce", tool: "search_nodes" },
    },
  ],
];

const fixture: EvalFixture = {
  name: "agent-matrix",
  description: "Agent Eval Results (end-to-end tool-call behavior)",
  groups: GROUPS,
  score(scenario, turns) {
    const toolsCalled = turns.flatMap((t) => t.toolsCalled);
    const toolCalls = turns.flatMap((t) => t.toolCalls ?? []);
    return assertExpectation(
      (scenario as MatrixScenario).expect,
      toolsCalled,
      toolCalls,
    );
  },
  extra(scenario, turns: TurnRecord[]) {
    return {
      expect: (scenario as MatrixScenario).expect,
      toolsOffered: turns[0]?.toolsOffered ?? "",
      toolsCalled: turns.flatMap((t) => t.toolsCalled),
      // Recorded so a failure carries its evidence: which call errored, and how
      // many fields it actually persisted. Reading a results file should not
      // require re-running the eval to find out why a scenario failed.
      toolCalls: turns.flatMap((t) => t.toolCalls ?? []),
      latencyMs: turns.reduce((sum, t) => sum + t.latencyMs, 0),
    };
  },
};

export default fixture;
