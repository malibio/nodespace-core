/**
 * Skill-routing eval — routing accuracy through the two-gate pipeline (ADR-036).
 *
 *   Stage 1: model forms a search query (or emits a clarification request)
 *   Stage 2: model judges whether the retrieved skill fits the intent
 *
 * Coverage:
 *   - Direct intent → correct skill
 *   - Indirect phrasing → correct skill (the load-bearing assumption)
 *   - Ambiguous → expect clarification, not a guess
 *   - Existing-type instance vs new-type (Node Creation, not Schema Creation)
 *   - General / search → Research & Search skill
 *   - No-match / out-of-scope → clarify, then fall through (not a loop)
 *   - Mutating-skill gate: borderline schema-creation gated harder than read-only
 *
 * Scenario wording must stay independent of packages/agent/src/agent_guidance.rs;
 * `guidance_is_not_contaminated_by_eval_prompts` parses the `prompt:` literals
 * out of this file and fails the build if guidance reproduces one.
 */

import type { EvalFixture, Scenario, TurnRecord, Verdict } from "../types.ts";

// ---------------------------------------------------------------------------
// Expectation model
// ---------------------------------------------------------------------------

export type ExpectedOutcome =
  | { kind: "skill"; skill: string } // skill name (substring match, case-insensitive)
  | { kind: "clarify" } // a clarification question, no tool action
  | { kind: "search" }; // the Research & Search / general search skill

interface RoutingScenario extends Scenario {
  expected: ExpectedOutcome;
  /** Covers a "thin evidence" area the ADR calls out explicitly. */
  loadBearing?: boolean;
  /** Mutating skill: the bar is higher (schema-create vs search). */
  mutating?: boolean;
  /** A message that must NOT reach a mutating skill. */
  adversarial?: boolean;
}

// ---------------------------------------------------------------------------
// Scenarios
//
// Each runs in its own chat node. `id` is the baseline join key and must stay
// stable; prompts may be reworded freely.
// ---------------------------------------------------------------------------

const FIXTURES: RoutingScenario[] = [
  // ── Direct intent → correct skill ────────────────────────────────────────
  {
    id: "direct-schema-create",
    scenario: "Direct: create schema (explicit phrasing)",
    prompt: "Create an invoice tracking database",
    expected: { kind: "skill", skill: "Schema Creation" },
  },
  {
    id: "direct-node-search",
    scenario: "Direct: search (explicit phrasing)",
    prompt: "Search my notes for anything about embeddings",
    expected: { kind: "search" },
  },
  {
    id: "direct-node-create",
    scenario: "Direct: create instance (existing type)",
    prompt: "Create a new task called 'Review Q3 report'",
    expected: { kind: "skill", skill: "Node Creation" },
  },

  // ── Indirect phrasing → correct skill (LOAD-BEARING) ─────────────────────
  {
    id: "indirect-schema-money-owed",
    scenario: "Indirect: 'keep tabs on who owes me money' → Schema Creation",
    prompt: "keep tabs on who owes me money",
    expected: { kind: "skill", skill: "Schema Creation" },
    loadBearing: true,
    mutating: true,
  },
  {
    id: "indirect-schema-freelance",
    scenario:
      "Indirect: 'start tracking my freelance projects' → Schema Creation",
    prompt: "start tracking my freelance projects",
    expected: { kind: "skill", skill: "Schema Creation" },
    loadBearing: true,
    mutating: true,
  },
  {
    id: "indirect-schema-expenses",
    scenario:
      "Indirect: 'I need a way to log my business expenses' → Schema Creation",
    prompt: "I need a way to log my business expenses",
    expected: { kind: "skill", skill: "Schema Creation" },
    loadBearing: true,
    mutating: true,
  },
  {
    id: "indirect-search-remember",
    scenario: "Indirect: 'what did I write about X?' → Research & Search",
    prompt: "what did I write about machine learning last month?",
    expected: { kind: "search" },
    loadBearing: true,
  },

  // ── Existing-type instance vs new-type (critical distinction) ────────────
  {
    id: "instance-not-schema-invoice",
    scenario:
      "Instance vs schema: 'add an invoice for $500' → Node Creation, not Schema Creation",
    // Context: invoice schema already exists (set up in prior turn)
    priorTurns: ["Create an invoice tracking database"],
    prompt: "Add an invoice for $500 due next Friday",
    expected: { kind: "skill", skill: "Node Creation" },
    loadBearing: true,
    adversarial: true, // should NOT route to Schema Creation
  },

  // ── Ambiguous → expect clarification ─────────────────────────────────────
  {
    id: "ambiguous-client-contacts",
    scenario:
      "Ambiguous: 'organize my client contacts' → clarify (schema vs collection)",
    prompt: "organize my client contacts",
    expected: { kind: "clarify" },
    loadBearing: true,
  },
  {
    id: "ambiguous-manage-projects",
    scenario:
      "Ambiguous: 'help me manage my projects' → clarify (schema vs search vs task)",
    prompt: "help me manage my projects",
    expected: { kind: "clarify" },
  },

  // ── General / search → Research & Search ─────────────────────────────────
  {
    id: "general-search-normal",
    scenario: "General search: a normal search outcome (not a failure)",
    prompt: "Find everything I have about the NodeSpace architecture",
    expected: { kind: "search" },
  },

  // ── No-match / out-of-scope → clarify, then fall through ─────────────────
  {
    id: "out-of-scope-weather",
    scenario:
      "Out of scope: weather query → clarify or fallback, no mutating tool",
    prompt: "What's the weather like in Tokyo today?",
    // The ADR requires: out-of-scope queries should not silently fire a mutating tool.
    // The correct guard is "no mutating tool called" — enforced via adversarial:true on
    // a clarify fixture (see assertFixture clarify branch). The routing pipeline may or
    // may not be entered (search_skills may still fire to confirm no skill matches),
    // so !calledSearchSkills is not asserted here — the key invariant is no mutation.
    expected: { kind: "clarify" },
    adversarial: true, // enforces calledMutatingTool check in assertFixture clarify branch
  },

  // ── Clarification contract: one clarification, then fall through ──────────
  {
    id: "clarification-then-fallthrough",
    scenario:
      "Clarification contract: after user clarifies, model proceeds (not a loop)",
    priorTurns: [
      "organize my client contacts",
      // Simulate user clarifying: they want to search existing contacts
      "I just want to search what I already have",
    ],
    prompt: "just show me what I have",
    expected: { kind: "search" },
    loadBearing: true,
  },

  // ── Mutating-skill gate: borderline schema vs read-only ───────────────────
  {
    id: "mutating-gate-borderline-schema",
    scenario:
      "Mutating gate: borderline schema request — gated harder than read-only",
    prompt: "maybe set up some kind of tracking for vendors",
    expected: { kind: "clarify" }, // borderline → must clarify before schema creation
    mutating: true,
    loadBearing: true,
    adversarial: true, // should NOT silently fire create_schema
  },
  {
    id: "readonly-gate-permissive",
    scenario: "Read-only gate: borderline search — lower bar, may proceed",
    prompt: "show me stuff about my customers",
    expected: { kind: "search" }, // read-only: proceed with search, don't block
  },
];

// ---------------------------------------------------------------------------
// Scoring
// ---------------------------------------------------------------------------

function isClarification(reply: string): boolean {
  const lower = reply.toLowerCase();
  // A clarification contains a question directed at the user about their intent.
  // Avoid false positives from rhetorical questions or confirmations.
  const hasQuestion = reply.includes("?");
  const hasIntentWords =
    lower.includes("did you") ||
    lower.includes("do you mean") ||
    lower.includes("would you like") ||
    lower.includes("are you looking") ||
    lower.includes("could you clarify") ||
    lower.includes("what would you like") ||
    lower.includes("which one") ||
    lower.includes("which would") ||
    lower.includes("which do") ||
    lower.includes("clarif");
  return hasQuestion && hasIntentWords;
}

function skillNameFromTurns(turns: TurnRecord[]): string | null {
  // search_skills is always called before using a skill; the matched skill name
  // appears in the reply or is implicit from subsequent tool calls.
  // We parse from the reply text as a heuristic — the name appears in the model's
  // "I'll use the Schema Creation skill..." style phrasing.
  for (const t of turns) {
    const lower = t.reply.toLowerCase();
    for (const name of [
      "schema creation",
      "node creation",
      "research & search",
      "research and search",
      "graph editing",
      "relationship management",
      "node deletion",
      "bulk import",
      "organization",
    ]) {
      if (lower.includes(name.toLowerCase())) return name;
    }
  }
  return null;
}

function calledSearchSkills(turns: TurnRecord[]): boolean {
  return turns.some((t) => t.toolsCalled.includes("search_skills"));
}

function calledMutatingTool(turns: TurnRecord[]): boolean {
  // All tools that write or mutate graph state — cross-referenced against Tool::ALL
  // in packages/agent/src/local_agent/tools.rs. Update here if the tool registry changes.
  const mutating = [
    "create_schema",
    "update_schema",
    "create_node",
    "update_node",
    "update_task_status",
    "delete_node",
    "create_relationship",
    "create_nodes_from_markdown",
  ];
  return turns.some((t) => t.toolsCalled.some((tc) => mutating.includes(tc)));
}

function calledSchemaCreate(turns: TurnRecord[]): boolean {
  return turns.some((t) => t.toolsCalled.includes("create_schema"));
}

function assertFixture(fixture: RoutingScenario, turns: TurnRecord[]): Verdict {
  const allReplies = turns.map((t) => t.reply).join("\n");
  const clarified = isClarification(allReplies);
  const searched = calledSearchSkills(turns);

  switch (fixture.expected.kind) {
    case "skill": {
      const expectedSkill = fixture.expected.skill.toLowerCase();
      const replyLower = allReplies.toLowerCase();
      // Model should have called search_skills and referenced or used the expected skill
      if (!searched) {
        return {
          passed: false,
          failure: `Expected search_skills to be called for skill routing, but it was not`,
        };
      }
      if (!replyLower.includes(expectedSkill)) {
        // Also check if the right tools were called (e.g. create_schema for Schema Creation)
        const toolCheck =
          (expectedSkill.includes("schema creation") &&
            calledSchemaCreate(turns)) ||
          (expectedSkill.includes("node creation") &&
            turns.some((t) => t.toolsCalled.includes("create_node")));
        if (!toolCheck) {
          return {
            passed: false,
            failure: `Expected skill '${fixture.expected.skill}' but reply did not reference it and tools did not confirm. Reply: ${allReplies.slice(0, 300)}`,
          };
        }
      }
      // Adversarial: if this fixture is adversarial, the model must NOT fire the wrong mutating tool
      if (
        fixture.adversarial &&
        fixture.expected.skill === "Node Creation" &&
        calledSchemaCreate(turns)
      ) {
        return {
          passed: false,
          failure: `Adversarial check failed: called create_schema when only Node Creation was expected`,
        };
      }
      return { passed: true };
    }

    case "clarify": {
      if (!clarified) {
        return {
          passed: false,
          failure: `Expected a clarification question but got: ${allReplies.slice(0, 300)}`,
        };
      }
      // Adversarial: mutating adversarial fixtures must NOT fire a mutating tool
      if (fixture.adversarial && calledMutatingTool(turns)) {
        return {
          passed: false,
          failure: `Adversarial check failed: fired a mutating tool (${turns.flatMap((t) => t.toolsCalled).join(",")}) without clarifying first`,
        };
      }
      return { passed: true };
    }

    case "search": {
      const hasSearch = turns.some(
        (t) =>
          t.toolsCalled.includes("search_semantic") ||
          t.toolsCalled.includes("search_nodes"),
      );
      if (!hasSearch) {
        return {
          passed: false,
          failure: `Expected a search tool call (search_semantic or search_nodes) but got tools: ${turns.flatMap((t) => t.toolsCalled).join(",")}. Reply: ${allReplies.slice(0, 300)}`,
        };
      }
      return { passed: true };
    }
  }
}

const fixture: EvalFixture = {
  name: "routing",
  description: "Routing Eval Results (skill-discovery accuracy)",
  // Each scenario is independent, so every one gets its own chat node.
  groups: FIXTURES.map((f) => [f]),
  score(scenario, turns) {
    return assertFixture(scenario as RoutingScenario, turns);
  },
  extra(scenario, turns) {
    const s = scenario as RoutingScenario;
    return {
      expected: s.expected,
      loadBearing: s.loadBearing ?? false,
      mutating: s.mutating ?? false,
      adversarial: s.adversarial ?? false,
      skillsSearched: calledSearchSkills(turns),
      matchedSkill: skillNameFromTurns(turns),
      clarified: isClarification(turns.map((t) => t.reply).join("\n")),
      toolsCalled: turns.flatMap((t) => t.toolsCalled),
      latencyMs: turns.reduce((sum, t) => sum + t.latencyMs, 0),
    };
  },
  summary(results) {
    const count = (pred: (e: Record<string, unknown>) => boolean) => {
      const rows = results.filter((r) => r.extra && pred(r.extra));
      return `${rows.filter((r) => r.passed).length}/${rows.length}`;
    };
    return [
      `Load-bearing (indirect phrasing + clarification): ${count((e) => e.loadBearing === true)}`,
      `Mutating gate:  ${count((e) => e.mutating === true)}`,
    ];
  },
};

export default fixture;
