#!/usr/bin/env bun
/**
 * routing-eval.ts — repeatable fixture harness for skill-discovery routing accuracy.
 *
 * Tests the two-gate routing pipeline from ADR-036:
 *   Stage 1: model forms a search query (or emits clarification request)
 *   Stage 2: model judges whether the retrieved skill fits the intent
 *
 * Coverage mirrors issue #1357 acceptance criteria:
 *   - Direct intent → correct skill
 *   - Indirect phrasing → correct skill (the load-bearing assumption)
 *   - Ambiguous → expect clarification, not a guess
 *   - Existing-type instance vs new-type (Node Creation, not Schema Creation)
 *   - General / search → Research & Search skill
 *   - No-match / out-of-scope → clarify, then fall through (not a loop)
 *   - Mutating-skill gate: borderline schema-creation gated harder than read-only
 *
 * Usage:
 *   NS_BIN=target/release/nodespace \
 *   NODESPACED_SOCKET=/tmp/nodespaced-test/daemon.sock \
 *   NS_LOG=/tmp/nodespaced-test/daemon.log \
 *   NS_MODEL=<id> NS_TIMEOUT_MS=120000 \
 *     bun run scripts/routing-eval.ts <label> [out.json]
 *
 * Writes results to <out.json> (default: /tmp/routing-eval-<label>-<ts>.json).
 * Exits 0 if all mandatory assertions pass, 1 otherwise.
 * Set ROUTING_BASELINE=path/to/baseline.json to compare against recorded baseline.
 *
 * The daemon, socket, and DB must be managed by the caller (same as aichat-matrix.ts).
 * Skills must be seeded in the daemon's DB before running this harness.
 *
 * Dependencies: #1356 (find_skills returns instructions subtree — affects Stage-2 judgment).
 * Gated on: llama.cpp upgrade issue (run against the verified engine, not the vendored build).
 */

import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const WORKTREE = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const AICHAT = join(WORKTREE, "scripts", "aichat.ts");

const [label, outPathArg] = process.argv.slice(2);
if (!label) {
  console.error(
    "usage: routing-eval.ts <label> [out.json]\n" +
      "  label    arbitrary tag recorded in results (e.g. 'e4b-upgraded')\n" +
      "  out.json path to write results (default: /tmp/routing-eval-<label>-<ts>.json)",
  );
  process.exit(1);
}

const ts = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
const outPath = outPathArg ?? `/tmp/routing-eval-${label}-${ts}.json`;
const baselinePath = process.env.ROUTING_BASELINE;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type ExpectedOutcome =
  | { kind: "skill"; skill: string } // expects skill name (substring match, case-insensitive)
  | { kind: "clarify" } // expects a clarification question, no tool action
  | { kind: "search" }; // expects the Research & Search / general search skill

interface Fixture {
  id: string; // unique stable ID for baseline diffing
  scenario: string; // human-readable description
  prompt: string; // the user message to send
  // Optional prior turns that set up context (same chat node, in order)
  priorTurns?: string[];
  expected: ExpectedOutcome;
  // Fixtures that cover the "thin evidence" areas the ADR calls out explicitly
  loadBearing?: boolean;
  // Mutating skill: the bar should be higher (schema-create vs search)
  mutating?: boolean;
  // Adversarial: a message that should NOT match a mutating skill
  adversarial?: boolean;
}

interface TurnRecord {
  toolsOffered: string;
  toolsCalled: string[];
  reply: string;
  latencyMs: number;
}

interface FixtureResult {
  id: string;
  scenario: string;
  prompt: string;
  expected: ExpectedOutcome;
  loadBearing: boolean;
  mutating: boolean;
  adversarial: boolean;
  passed: boolean;
  failure?: string;
  // Routing signals
  skillsSearched: boolean; // did the model call search_skills?
  matchedSkill: string | null; // first skill name returned (from log)
  topScore: number | null;
  turnCount: number; // total turns in the chat node for this fixture
  clarified: boolean; // did the reply contain a clarification question?
  // Raw turn data
  turns: TurnRecord[];
}

interface EvalResults {
  label: string;
  model: string;
  timestamp: string;
  summary: {
    total: number;
    passed: number;
    failed: number;
    loadBearingPassed: number;
    loadBearingTotal: number;
    mutatingPassed: number;
    mutatingTotal: number;
  };
  fixtures: FixtureResult[];
}

// ---------------------------------------------------------------------------
// Fixture set (issue #1357 acceptance criteria)
// ---------------------------------------------------------------------------

const FIXTURES: Fixture[] = [
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
    scenario: "Indirect: 'start tracking my freelance projects' → Schema Creation",
    prompt: "start tracking my freelance projects",
    expected: { kind: "skill", skill: "Schema Creation" },
    loadBearing: true,
    mutating: true,
  },
  {
    id: "indirect-schema-expenses",
    scenario: "Indirect: 'I need a way to log my business expenses' → Schema Creation",
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
    scenario: "Instance vs schema: 'add an invoice for $500' → Node Creation, not Schema Creation",
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
    scenario: "Ambiguous: 'organize my client contacts' → clarify (schema vs collection)",
    prompt: "organize my client contacts",
    expected: { kind: "clarify" },
    loadBearing: true,
  },
  {
    id: "ambiguous-manage-projects",
    scenario: "Ambiguous: 'help me manage my projects' → clarify (schema vs search vs task)",
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
    scenario: "Out of scope: weather query → clarify or fallback, no mutating tool",
    prompt: "What's the weather like in Tokyo today?",
    // Not routing to any mutating skill; either clarify or general response
    expected: { kind: "clarify" },
    adversarial: true,
  },

  // ── Clarification contract: one clarification, then fall through ──────────
  {
    id: "clarification-then-fallthrough",
    scenario: "Clarification contract: after user clarifies, model proceeds (not a loop)",
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
    scenario: "Mutating gate: borderline schema request — gated harder than read-only",
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
// Harness helpers (reuse aichat.ts patterns)
// ---------------------------------------------------------------------------

function runAichat(args: string[]): {
  exitCode: number;
  stdout: string;
  stderr: string;
} {
  const r = Bun.spawnSync(["bun", "run", AICHAT, ...args], {
    stdout: "pipe",
    stderr: "pipe",
    env: { ...process.env },
  });
  return {
    exitCode: r.exitCode ?? 1,
    stdout: r.stdout.toString(),
    stderr: r.stderr.toString(),
  };
}

function newChat(): string {
  const r = runAichat(["new"]);
  if (r.exitCode !== 0) throw new Error(`new failed: ${r.stderr}`);
  return r.stdout.trim();
}

function sendTurn(chatId: string, message: string): TurnRecord {
  const start = performance.now();
  const r = runAichat(["send", chatId, message]);
  const latencyMs = Math.round(performance.now() - start);

  if (r.exitCode !== 0) {
    return {
      toolsOffered: `(error: ${r.stderr.trim()})`,
      toolsCalled: [],
      reply: `(send failed: ${r.stderr.trim()})`,
      latencyMs,
    };
  }

  const out = r.stdout;
  const toolsOffered = out.match(/\[tools offered\] (.*)/)?.[1]?.trim() ?? "";
  const toolsCalled = [...out.matchAll(/\[tool\] ([a-z_]+)/g)].map((m) => m[1]);
  const reply = out.match(/assistant> ([\s\S]*)$/)?.[1]?.trim() ?? "(no reply parsed)";

  return { toolsOffered, toolsCalled, reply, latencyMs };
}

// ---------------------------------------------------------------------------
// Assertion logic
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
    lower.includes("which") ||
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
  const mutating = ["create_schema", "update_schema", "create_node", "delete_node"];
  return turns.some((t) => t.toolsCalled.some((tc) => mutating.includes(tc)));
}

function calledSchemaCreate(turns: TurnRecord[]): boolean {
  return turns.some((t) => t.toolsCalled.includes("create_schema"));
}

function assertFixture(fixture: Fixture, turns: TurnRecord[]): { passed: boolean; failure?: string } {
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
          (expectedSkill.includes("schema creation") && calledSchemaCreate(turns)) ||
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
      if (fixture.adversarial && fixture.expected.skill === "Node Creation" && calledSchemaCreate(turns)) {
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

// ---------------------------------------------------------------------------
// Main eval loop
// ---------------------------------------------------------------------------

console.error(`[routing-eval] label=${label} model=${process.env.NS_MODEL ?? "(default)"}`);
console.error(`[routing-eval] ${FIXTURES.length} fixtures to run`);

const results: FixtureResult[] = [];
let failed = 0;

for (const fixture of FIXTURES) {
  const chatId = newChat();
  console.error(`[routing-eval] fixture: ${fixture.id} (chat ${chatId})`);

  const allTurns: TurnRecord[] = [];

  // Run prior-context turns (not asserted)
  for (const prior of fixture.priorTurns ?? []) {
    console.error(`  [context] ${prior}`);
    const t = sendTurn(chatId, prior);
    allTurns.push(t);
    console.error(`    tools=[${t.toolsCalled.join(",")}] ${t.latencyMs}ms`);
  }

  // Run the fixture turn (asserted)
  console.error(`  [fixture] ${fixture.prompt}`);
  const t = sendTurn(chatId, fixture.prompt);
  allTurns.push(t);
  console.error(`    tools=[${t.toolsCalled.join(",")}] ${t.latencyMs}ms`);

  // Only assert on the fixture turn and its reply (last turn)
  const assertionTurns = allTurns.slice(fixture.priorTurns?.length ?? 0);
  const { passed, failure } = assertFixture(fixture, assertionTurns);

  if (!passed) {
    failed++;
    console.error(`  ✗ FAILED: ${failure}`);
  } else {
    console.error(`  ✓ passed`);
  }

  results.push({
    id: fixture.id,
    scenario: fixture.scenario,
    prompt: fixture.prompt,
    expected: fixture.expected,
    loadBearing: fixture.loadBearing ?? false,
    mutating: fixture.mutating ?? false,
    adversarial: fixture.adversarial ?? false,
    passed,
    failure,
    skillsSearched: calledSearchSkills(assertionTurns),
    matchedSkill: skillNameFromTurns(assertionTurns),
    topScore: null, // populated from log parsing in future iteration
    turnCount: allTurns.length,
    clarified: isClarification(assertionTurns.map((t) => t.reply).join("\n")),
    turns: allTurns,
  });
}

// ---------------------------------------------------------------------------
// Summary
// ---------------------------------------------------------------------------

const total = results.length;
const passed = total - failed;
const loadBearingResults = results.filter((r) => r.loadBearing);
const loadBearingPassed = loadBearingResults.filter((r) => r.passed).length;
const mutatingResults = results.filter((r) => r.mutating);
const mutatingPassed = mutatingResults.filter((r) => r.passed).length;

const evalResults: EvalResults = {
  label,
  model: process.env.NS_MODEL ?? "(default)",
  timestamp: new Date().toISOString(),
  summary: {
    total,
    passed,
    failed,
    loadBearingPassed,
    loadBearingTotal: loadBearingResults.length,
    mutatingPassed,
    mutatingTotal: mutatingResults.length,
  },
  fixtures: results,
};

await Bun.write(outPath, JSON.stringify(evalResults, null, 2));
console.error(`\n[routing-eval] results written to ${outPath}`);

// Print summary table
console.log(`\n── Routing Eval Results ─────────────────────────────────────────────`);
console.log(`   Label:   ${label}`);
console.log(`   Model:   ${process.env.NS_MODEL ?? "(default)"}`);
console.log(`   Passed:  ${passed}/${total}`);
console.log(
  `   Load-bearing (indirect phrasing + clarification): ${loadBearingPassed}/${loadBearingResults.length}`,
);
console.log(`   Mutating gate:  ${mutatingPassed}/${mutatingResults.length}`);
console.log(`────────────────────────────────────────────────────────────────────`);
for (const r of results) {
  const mark = r.passed ? "✓" : "✗";
  const lb = r.loadBearing ? " [load-bearing]" : "";
  console.log(`  ${mark} ${r.id}${lb}`);
  if (!r.passed) console.log(`      ↳ ${r.failure}`);
}
console.log(`────────────────────────────────────────────────────────────────────\n`);

// ---------------------------------------------------------------------------
// Baseline comparison (optional)
// ---------------------------------------------------------------------------

if (baselinePath) {
  try {
    const baseline: EvalResults = JSON.parse(await Bun.file(baselinePath).text());
    console.log(`── Baseline Comparison (vs ${baseline.label} @ ${baseline.timestamp}) ──`);
    let regressions = 0;
    for (const cur of results) {
      const base = baseline.fixtures.find((f) => f.id === cur.id);
      if (!base) {
        console.log(`  NEW  ${cur.id} → ${cur.passed ? "pass" : "fail"}`);
        continue;
      }
      if (base.passed && !cur.passed) {
        console.log(`  REGRESSION  ${cur.id}: was passing, now failing — ${cur.failure}`);
        regressions++;
      } else if (!base.passed && cur.passed) {
        console.log(`  FIXED  ${cur.id}: was failing, now passing`);
      }
    }
    if (regressions > 0) {
      console.error(`\n[routing-eval] ✗ ${regressions} regression(s) vs baseline — failing`);
      process.exit(1);
    } else {
      console.log(`\n[routing-eval] ✓ No regressions vs baseline`);
    }
  } catch (e) {
    console.error(`[routing-eval] Warning: could not read baseline at ${baselinePath}: ${e}`);
  }
}

// Fail the process if mandatory assertions failed (for CI)
if (failed > 0) {
  console.error(`\n[routing-eval] ✗ ${failed}/${total} fixtures failed`);
  process.exit(1);
} else {
  console.error(`\n[routing-eval] ✓ All ${total} fixtures passed`);
  process.exit(0);
}
