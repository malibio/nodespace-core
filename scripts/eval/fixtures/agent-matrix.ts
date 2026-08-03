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
 * Under ADR-038 routing happens in a separate stage before the acting turn, so
 * assertions check for the TARGET tool tolerating routing calls, never raw
 * tool count.
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
  //
  // `minProperties` additionally requires the call to have persisted at least
  // that many schema field values. Without it a create_node that recorded none
  // of the user's particulars — no cost, no date, no status — scores identically
  // to one that recorded them all, because the tool name is all that is checked.
  // Set it on any scenario whose prompt supplies particulars a later scenario
  // depends on, or that chain silently keys on a value nothing ever stored.
  | { kind: "toolOnce"; tool: string; minProperties?: number }
  // Tools fired in this order as a subsequence (ignoring routing tools) — other
  // tools may appear between/around them, but these must appear in this order.
  //
  // `minProperties` carries the same meaning as on `toolOnce`, applied to
  // `propertiesOn` (defaulting to the last tool in the sequence). A
  // resolve-then-act chain can call exactly the right tools in exactly the
  // right order and still drop the user's request: an update_node that
  // resolved the correct node but sent only its id changes nothing, yet
  // scores identically to one that persisted the state change, because the
  // tool name is all that is checked. Set it on any chain whose final call
  // must carry a value for the turn to have accomplished anything.
  | {
      kind: "toolSequence";
      tools: string[];
      minProperties?: number;
      propertiesOn?: string;
    }
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
// Stage-1 routing calls (ADR-038). These are not actions the scenario is
// asserting on, so they are filtered out before the action-tool check.
// `search_skills` remains listed because the tool still exists for external
// agents; the local model is no longer offered it, so it should not appear in
// a local trace — tolerating it costs nothing and avoids a false failure if it
// ever does.
const ROUTING_TOOLS = ["route_query", "route_clarify", "search_skills"];

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
 * Check that a call persisted at least `min` schema field values.
 *
 * The instance-side counterpart to `schemaCallsAreSound`, and the same failure
 * shape one level down: that one catches a schema with no fields to record
 * against, this catches a record with no field values in it. A create_node
 * carrying only `content` and `node_type` succeeds, persists a bare shell, and
 * scores green on tool name alone — while every later scenario that keys on one
 * of those missing values becomes unwinnable, and looks like a model failure
 * rather than a fixture that never stored the value.
 *
 * `fieldCount` is absent (rather than 0) on results recorded before the tool
 * reported it, so absence is treated as unknown and passes — for the same
 * reason `schemaCallsAreSound` does it: a stale baseline must not read as a
 * fresh failure.
 */
function callPersistedProperties(
  calls: ToolCallRecord[],
  tool: string,
  min: number,
): Verdict {
  for (const c of calls) {
    if (c.name !== tool) continue;
    // An errored call to the TARGET tool is a failure, not something to skip.
    // Once the tool-boundary gate rejects a no-op update_node, the reproducing
    // shape arrives here as `isError` — skipping it made the scenario score
    // green on exactly the defect it was added to catch. `schemaCallsAreSound`
    // already treats isError this way; this is the missing instance-side half.
    if (c.isError) {
      return {
        passed: false,
        failure:
          `${tool} was rejected, so the requested change never reached storage — ` +
          `the turn did not accomplish what the prompt asked for`,
      };
    }
    // The write reported that it had no properties to persist. That is a
    // complete success for a plain note or a rename, but this assertion is
    // only set on scenarios whose prompt DOES supply a value to store, so
    // here it means the value never made it into `properties`.
    if (c.contentOnly) {
      return {
        passed: false,
        failure:
          `${tool} changed only content and persisted no property values, but the ` +
          `prompt supplied a value to store — the requested state change was not recorded`,
      };
    }
    if (c.fieldCount === undefined) continue;
    if (c.fieldCount < min) {
      return {
        passed: false,
        failure:
          `${tool} succeeded but persisted ${c.fieldCount} property value(s), ` +
          `expected at least ${min} — the node was created without the ` +
          `particulars the prompt supplied, so anything keyed on them cannot resolve`,
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
      const schemaVerdict = schemaCallsAreSound(toolCalls);
      if (!schemaVerdict.passed) return schemaVerdict;
      if (expect.minProperties !== undefined) {
        return callPersistedProperties(
          toolCalls,
          expect.tool,
          expect.minProperties,
        );
      }
      return { passed: true };
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
      if (expect.minProperties !== undefined) {
        return callPersistedProperties(
          toolCalls,
          expect.propertiesOn ?? expect.tools[expect.tools.length - 1],
          expect.minProperties,
        );
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
  // Single-custom-type CRUD chain (scenarios 3-7, then 9) shares one chat node.
  // Scenario 9 is deliberately last: it needs the laser cutter that scenario 4
  // creates, and referring to it by name keeps its own resolution a direct
  // string match rather than the indirect reference scenario 6 exercises.
  [
    {
      id: "3",
      scenario: "3. Schema creation",
      // Every field a later scenario keys on must be implied here, or that
      // scenario is unwinnable by construction and scores a correct refusal as
      // a failure. Two are load-bearing downstream:
      //   - checked-out vs returned → the status scenario 6 sets.
      //   - replacement cost        → the value scenario 4 supplies, and the
      //     discriminator scenarios 6 ("the 2400 one") and 7 ("worth 90000")
      //     both resolve against.
      // The cost clause is deliberate: scenario 6 exists to test resolve_query
      // on an *indirect* reference. Re-keying it to the item name ("the laser
      // cutter") would make the referent a direct string match that plain
      // search_nodes resolves, and the assertion would pass while testing less.
      prompt:
        "I want to keep a record of the equipment my team checks out, whether it's been returned, and what each item costs to replace",
      expect: { kind: "noExtraTypes" },
    },
    {
      id: "4",
      scenario: "4. Instance creation",
      // `minProperties` is what makes scenarios 6 and 7 winnable *in principle*:
      // both discriminate on the replacement cost this turn is supposed to
      // store. Without it, create_node persisting a bare shell scores green
      // here and the failure surfaces two scenarios later as an unresolvable
      // reference — indistinguishable from the model declining a genuinely
      // ambiguous one. 1, not 2, so this asserts "the particulars reached
      // storage" rather than pinning which of the date or cost the model chose
      // to record.
      prompt:
        "Log a laser cutter checked out on the 12th, replacement cost 2400",
      expect: { kind: "toolOnce", tool: "create_node", minProperties: 1 },
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
      // minProperties: 1 requires the requested state change to actually reach
      // update_node. Resolving the right node and then calling update_node with
      // only its id changes nothing, and without this scores as a pass.
      expect: {
        kind: "toolSequence",
        tools: ["resolve_query", "update_node"],
        minProperties: 1,
      },
    },
    {
      id: "7",
      scenario: "7. Empty-result query",
      prompt: "Do we have anything worth 90000 sitting out?",
      expect: { kind: "noRetry", tool: "search_nodes" },
    },
    {
      id: "9",
      scenario: "9. Set property on existing node",
      // Distinct from scenario 6, which tests resolving an INDIRECT reference
      // ("the 2400 one") and happens to update it. Here the referent is a
      // direct string match, so nothing is being tested about resolution —
      // the whole assertion is that the *value the prompt supplies* reaches
      // storage.
      //
      // This is the shape that reached production returning `updated: true`
      // with `property_count: 0`: the model resolved the right node, called
      // update_node, echoed the node's existing title back as `content`, and
      // sent no properties at all. The tool reported success, and the model
      // reported the write as done with a fabricated value. minProperties is
      // what makes that outcome score red rather than green — without it, a
      // call that persists nothing is indistinguishable from one that
      // persisted the value, because the tool name is all that is checked.
      //
      // WINNABILITY (the constraint an earlier draft of this scenario broke):
      // the prompt must name a value this chain's schema can actually hold.
      // Scenario 3 builds Equipment Item from a prompt mentioning only
      // returned-ness and replacement cost, so those two fields are all that
      // exist. A first draft here asked to set a DUE DATE — a field the schema
      // has nowhere to put — which made the scenario unwinnable: the model
      // folded the date into the node's text (a reasonable degradation, and it
      // reported it honestly as "updated with a note") and scored red for it.
      // A scenario that reds out correct behavior measures the fixture, not the
      // model. Same trap as the album/artist case in #1846.
      //
      // `replacementCost` is chosen over `isReturned` because scenario 6
      // already owns the returned-ness transition; re-testing it here would
      // score the same model behavior twice. "1800" is unambiguous — no
      // relative-date or unit inference stands between the request and the
      // write, so a red here means the value did not reach `properties`, which
      // is the one thing this scenario is for.
      prompt: "Correction: the laser cutter's replacement cost is 1800, not 2400",
      expect: {
        kind: "toolOnce",
        tool: "update_node",
        minProperties: 1,
      },
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
      // minProperties: 1 requires the artist this prompt supplies to actually
      // reach storage. Without it, create_node persisting a bare shell (no
      // artist property — unwinnable if album_tracker's schema itself has no
      // artist field, see #1846) scores identically to one that recorded it.
      prompt: "Put down Kind of Blue, it's by Miles Davis",
      expect: { kind: "toolOnce", tool: "create_node", minProperties: 1 },
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
      // Per turn, not `turns[0]` alone: a scenario's turns can be offered
      // different tool surfaces (routing runs per turn and scopes Stage 2's
      // tools from that turn's candidates), so collapsing to the first turn
      // reports a surface later turns never saw. Same for the routed skill.
      //
      // Anticipatory today — every caller currently passes a single scored
      // turn, so these arrays hold one element. Kept per-turn because the
      // collapsing is what would be silently wrong the moment a scenario scores
      // more than one turn, and that is invisible in the results file.
      toolsOffered: turns.map((t) => t.toolsOffered),
      routedSkills: turns.map((t) => t.routedSkills ?? ""),
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
