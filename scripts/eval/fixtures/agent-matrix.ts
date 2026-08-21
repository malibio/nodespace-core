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
 * THE DOMAIN IS PART OF THE MEASUREMENT, NOT DECORATION
 *
 * Model-lock decisions (ADR-046, ADR-056) and every model re-evaluation since
 * are scored off this matrix, so whatever domain the scenarios are written in
 * is the domain native-model selection is actually being decided on. That made
 * the original scenario set — equipment checkouts, album and venue trackers —
 * an active problem rather than a cosmetic one: it selected models on their
 * ability to track laser cutters, while the product's claim is context
 * infrastructure for AI-native development.
 *
 * The scenarios are therefore written in NodeSpace's own working domain:
 * feature write-ups and their sign-off state, the calls a team makes about how
 * a system is built, planning cycles, work tied to the decision that
 * constrains it. See ../../../../nodespace-docs/strategy/{vision,beliefs,
 * principles}.md for the framing this tracks.
 *
 * The TOOL MECHANICS are unchanged by that re-theme and are the reason the
 * scenario set was re-themed in place rather than duplicated into a second
 * fixture: `noExtraTypes`, `minProperties`, `noRetry`+`minCalls` and
 * `toolSequence` are properties of the expectation model, not of the
 * vocabulary, so re-theming keeps every one of them — and keeps every scenario
 * `id`, so a pre-re-theme baseline still joins against a post-re-theme run.
 *
 * WINNABILITY IS A HARD CONSTRAINT ON WORDING
 *
 * Each chain builds its own schema in its first scenario, and every later
 * prompt in that chain must name only values that schema can actually hold. A
 * prompt asking for a field the type has nowhere to put is unwinnable: the
 * model degrades reasonably (folding the value into the node's text) and
 * scores red for it, and the fixture is then measuring itself rather than the
 * model. See scenario 9's note and #1846.
 *
 * Scenario wording must stay independent of packages/agent/src/agent_guidance.rs.
 * `guidance_is_not_contaminated_by_eval_prompts` enforces it by parsing the
 * `prompt:` literals out of this file: a prompt that also appears in guidance
 * turns a passing scenario into proof that the model can copy a memorized
 * example, and prompt tuning then has a degenerate solution. The dev-workflow
 * domain raises that risk rather than lowering it — the seeded skill
 * descriptions already name Spec, ADR and Ticket — so prompts here are written
 * around those terms and checked against the guard, not assumed clear of it.
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
  //
  // `minCalls` additionally requires the tool to have fired at least that many
  // times. Without it this expectation is satisfied by a turn in which the tool
  // never fired at all — the loop that detects a repeat never executes — so a
  // model that stopped and asked the user instead of searching scores
  // identically to one that searched correctly. That is the failure mode on the
  // read side, so a scenario testing for it must opt in. Left off by default
  // rather than made unconditional: changing the shared semantics would also
  // re-score existing scenarios, and single-run matrix numbers are not
  // decision-grade enough to absorb that silently.
  | { kind: "noRetry"; tool: string; minCalls?: number }
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
      // Checked after the retry loop, not before: a scenario that opts in wants
      // both "fired at least minCalls times" and "never twice in a row", and the
      // retry failure is the more specific diagnosis of the two.
      if (expect.minCalls !== undefined) {
        const count = actions.filter((t) => t === expect.tool).length;
        if (count < expect.minCalls) {
          return {
            passed: false,
            failure: `Expected at least ${expect.minCalls} '${expect.tool}' call(s), got ${count} (tools: ${actions.join(",")})`,
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

export interface MatrixScenario extends Scenario {
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
  // Scenario 9 is deliberately last: it needs the spec that scenario 4 creates,
  // and referring to it by name keeps its own resolution a direct string match
  // rather than the indirect reference scenario 6 exercises.
  [
    {
      id: "3",
      scenario: "3. Schema creation",
      // Every field a later scenario keys on must be implied here, or that
      // scenario is unwinnable by construction and scores a correct refusal as
      // a failure. Two are load-bearing downstream:
      //   - drafted vs signed off → the state scenario 6 sets.
      //   - the day count         → the value scenario 4 supplies, and the
      //     discriminator scenarios 6 ("the five-day one") and 7 ("longer than
      //     forty days") both resolve against.
      // The day-count clause is deliberate: scenario 6 exists to test
      // resolve_query on an *indirect* reference. Re-keying it to the spec's
      // own name would make the referent a direct string match that plain
      // search_nodes resolves, and the assertion would pass while testing less.
      prompt:
        "I want somewhere to keep the feature write-ups my team drafts, whether each has been signed off, and how many days we think it takes",
      expect: { kind: "noExtraTypes" },
    },
    {
      id: "4",
      scenario: "4. Instance creation",
      // `minProperties` is what makes scenarios 6 and 7 winnable *in principle*:
      // both discriminate on the day count this turn is supposed to store.
      // Without it, create_node persisting a bare shell scores green here and
      // the failure surfaces two scenarios later as an unresolvable reference —
      // indistinguishable from the model declining a genuinely ambiguous one.
      // 1, not 2, so this asserts "the particulars reached storage" rather than
      // pinning which of the state or the estimate the model chose to record.
      prompt: "Put one down for offline sync, still a draft, we reckon five days",
      expect: { kind: "toolOnce", tool: "create_node", minProperties: 1 },
    },
    {
      id: "5",
      scenario: "5. List/query",
      prompt: "What write-ups are on the books?",
      expect: { kind: "toolOnce", tool: "search_nodes" },
    },
    {
      id: "6",
      scenario: "6. Update",
      // resolve_query performs the search internally and returns the resolved
      // node directly (see ADR-064 rule 4) — the model acts on it via
      // update_node without a separate search_nodes call of its own.
      prompt: "The five-day one got signed off — mark it that way",
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
      prompt: "Is anything on our plate longer than forty days?",
      expect: { kind: "noRetry", tool: "search_nodes" },
    },
    {
      id: "9",
      scenario: "9. Set property on existing node",
      // Distinct from scenario 6, which tests resolving an INDIRECT reference
      // ("the five-day one") and happens to update it. Here the referent is a
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
      // Scenario 3 builds the write-up type from a prompt mentioning only
      // sign-off and a day count, so those two fields are all that exist. An
      // earlier draft of the equipment-themed ancestor of this scenario asked
      // to set a DUE DATE — a field the schema has nowhere to put — which made
      // the scenario unwinnable: the model folded the date into the node's text
      // (a reasonable degradation, and it reported it honestly) and scored red
      // for it. A scenario that reds out correct behavior measures the fixture,
      // not the model. Same trap as the album/artist case in #1846.
      //
      // The day count is chosen over the sign-off state because scenario 6
      // already owns that transition; re-testing it here would score the same
      // model behavior twice. "eight" is unambiguous — no relative-date or unit
      // inference stands between the request and the write, so a red here means
      // the value did not reach `properties`, which is the one thing this
      // scenario is for.
      prompt: "Correction: offline sync is eight days, not five",
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
      prompt: "Start keeping the calls we make on how the system is built",
      expect: { kind: "toolOnce", tool: "create_schema" },
    },
    {
      id: "8b",
      scenario: "8b. Create type: second",
      prompt: "I also need somewhere for the two-week cycles we plan",
      expect: { kind: "toolOnce", tool: "create_schema" },
    },
    {
      id: "8c",
      scenario: "8c. Instance: first type",
      // minProperties: 1 requires the particular this prompt supplies — who
      // made the call — to actually reach storage. Without it, create_node
      // persisting a bare shell (no such property — unwinnable if the type's
      // own schema has no field for it, see #1846) scores identically to one
      // that recorded it.
      prompt: "Put down that we went with event-based cache clearing, Priya's call",
      expect: { kind: "toolOnce", tool: "create_node", minProperties: 1 },
    },
    {
      id: "8d",
      scenario: "8d. Instance: second type",
      prompt: "New cycle: Harbour, it wraps up on the 30th",
      expect: { kind: "toolOnce", tool: "create_node" },
    },
    {
      id: "8e",
      scenario: "8e. Query across types",
      prompt: "Run through those calls for me",
      expect: { kind: "toolOnce", tool: "search_nodes" },
    },
  ],
  // Core-type schema fields (scenario 10) shares its own chat node.
  //
  // Every group above builds its own CUSTOM type first, which is precisely why
  // this gap went unmeasured: a custom type's fields reach the model through
  // the RELEVANT ENTITY TYPES block, and that block excludes core types by
  // construction. So `task`'s own defined fields — due_date, priority,
  // assignee — were invisible from every direction, and the matrix could not
  // see it because no scenario ever acted on a core type.
  //
  // These use `task` deliberately and create it with `properties` unset beyond
  // the minimum, so the fields under test are defined-but-unset — the exact
  // state where "field exists" and "field does not exist" were
  // indistinguishable.
  [
    {
      id: "10a",
      scenario: "10a. Core-type instance creation",
      // Winnability: due_date, priority and assignee are all defined on the
      // seeded core task schema, so 10b and 10c are answerable in principle.
      // This turn deliberately supplies NONE of them — the following scenarios
      // are about writing and filtering a field that has no value yet, which is
      // only a real test if it starts unset.
      prompt: "Add a task to swap the image resizer over to the new pipeline",
      expect: { kind: "toolOnce", tool: "create_node" },
    },
    {
      id: "10b",
      scenario: "10b. Set a defined-but-unset core field",
      // The reported failure: the model asked the user "what field name is
      // used on this task node that tracks dates?" for `due_date` — a field
      // defined on the core task schema all along. It was not being obtuse;
      // get_node returned only populated properties, so the field genuinely
      // was not visible to it, and the "use the node's own existing property
      // keys" rule then made declining the correct move.
      //
      // The prompt says "due date" in the user's words and never names the
      // key, so a pass requires the field list to have reached the model
      // rather than the key having been handed over in the prompt.
      //
      // minProperties: 1 is what makes this scenario mean anything: the whole
      // defect is a turn that ends without the value reaching `properties`.
      // Without it, a content-only update_node — or the model narrating the
      // change it did not make — scores identically to a real write.
      prompt: "Set that task's due date to 6 August 2026",
      expect: { kind: "toolOnce", tool: "update_node", minProperties: 1 },
    },
    {
      id: "10c",
      scenario: "10c. Filter core type by enum field",
      // The read-side half of the same root cause. Observed: the model asked
      // the user to confirm that `status` was the field and `open` a legal
      // value — both defined on the core task schema (status is required, with
      // core values open / in_progress / done / cancelled).
      //
      // `noRetry` rather than `toolOnce`: an empty or narrowing result may
      // legitimately prompt one follow-up search, so a hard count of 1 would
      // red out correct behavior. What it must not do is loop blindly.
      //
      // `minCalls: 1` covers the other half, and is the half this scenario
      // exists for: the reported failure is the model stopping to interrogate
      // the user rather than searching, which shows up as the search never
      // firing. Bare `noRetry` scores that outcome GREEN — its repeat-detecting
      // loop never executes over zero calls — so without `minCalls` this
      // scenario would pass on the exact production behavior it was added to
      // catch, which is worse than not measuring it at all.
      prompt: "How many tasks are still open?",
      expect: { kind: "noRetry", tool: "search_nodes", minCalls: 1 },
    },
  ],
  // Relationship traversal (scenario 11) shares its own chat node.
  //
  // The matrix's one structural blind spot until now: every group above acts on
  // a node's OWN fields, so nothing measured whether the model can record a
  // link between two nodes or follow one back. That is the half of the data
  // model the product's own framing rests on — a decision means nothing without
  // the work it constrains — and `create_relationship`/`get_related_nodes` were
  // never once exercised end-to-end despite both being registered tools.
  //
  // Kept as its own group rather than appended to the chain above because it
  // needs TWO nodes of DIFFERENT types to exist before the link is askable, and
  // building those inside another group would silently re-score that group's
  // create_node behavior a third time.
  [
    {
      id: "11a",
      scenario: "11a. Link setup: first node",
      // Not scored for relationship behavior — it exists so 11c has two real,
      // differently-typed endpoints to connect. `text` is the fallback the
      // model reaches for when no custom type fits, and that is fine here:
      // what 11c asserts is the LINK, and create_relationship takes two ids
      // regardless of what types they carry.
      prompt: "Note that we settled on server-side rendering for the reports page",
      expect: { kind: "toolOnce", tool: "create_node" },
    },
    {
      id: "11b",
      scenario: "11b. Link setup: second node",
      prompt: "Add a task to rebuild the reports page",
      expect: { kind: "toolOnce", tool: "create_node" },
    },
    {
      id: "11c",
      scenario: "11c. Record a link between two nodes",
      // The failure this is built to catch is NOT "no tool fired" — it is the
      // model expressing the link by writing prose into one of the two nodes,
      // which reports success and records nothing traversable. Asserting
      // create_relationship by name is what separates those two outcomes.
      //
      // `toolOnce` rather than `toolSequence`, deliberately, even though the
      // model must recover both endpoint ids before it can link them: the
      // lookup has three legitimate spellings (search_nodes, search_semantic,
      // get_node) and pinning any one of them into a sequence would red out
      // the other two. The retrieval is not left unmodelled by that choice —
      // 11a and 11b created both endpoints in this same chat, so the ids are
      // recoverable — it is simply not the thing being scored here. What is
      // being scored is that the link was recorded as an EDGE and exactly once.
      //
      // The known cost, stated rather than hidden: this cannot distinguish a
      // create_relationship carrying two real ids from one carrying two
      // invented ones. Catching that needs the tool result, not the tool name,
      // and `ToolCallRecord` does not carry the endpoint ids today. A run's
      // trace file does, so a suspicious pass is checkable by hand.
      //
      // No minProperties: create_relationship's payload is two ids and a
      // relation name, none of which are schema field values, so `fieldCount`
      // does not apply to it — asserting it would fail on a correct call. The
      // `minProperties is never asserted on create_relationship` invariant in
      // the test file pins that.
      prompt: "That rebuild has to follow what we settled on for that page — connect the two",
      expect: {
        kind: "toolOnce",
        tool: "create_relationship",
      },
    },
    {
      id: "11d",
      scenario: "11d. Traverse a link back",
      // The read half. This is the query the product's framing is built on —
      // "what constrains this piece of work" — and it is only answerable by
      // following the edge 11c recorded, not by matching text.
      //
      // `noRetry` with `minCalls: 1` rather than `toolOnce`, for the same
      // reason 10c uses it: a first lookup may legitimately be followed by one
      // narrowing call, but the tool must fire at least once. The failure mode
      // worth catching is the model answering from the conversation it can
      // still see in its own history instead of reading the graph — which
      // shows up precisely as the traversal never firing.
      //
      // NOTE ON DEPENDENCE: if 11c failed to record an edge, this scenario can
      // still pass — it asserts that the traversal was ATTEMPTED, not that it
      // came back non-empty. That is deliberate. Making it conditional on 11c's
      // success would fold two independent behaviors into one score and make a
      // link-side regression read as two failures instead of one.
      prompt: "What did we settle on that the rebuild has to respect?",
      expect: { kind: "noRetry", tool: "get_related_nodes", minCalls: 1 },
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
