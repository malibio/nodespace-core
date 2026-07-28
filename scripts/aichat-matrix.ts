#!/usr/bin/env bun
/**
 * aichat-matrix.ts — assertable end-to-end agent-behavior eval (issue #1364).
 *
 * Runs the #1329 scenario matrix (greeting, schema creation, instance creation,
 * list/query, update, empty-result, multi-type CRUD) through aichat.ts and asserts
 * a structured, machine-checkable expectation per scenario — not just captures
 * prose for a human to read.
 *
 * This is the third eval layer, distinct from the other two:
 *   - chat::parser::tests            — tool-call PARSING shape (fixtures)
 *   - scripts/routing-eval.ts        — skill ROUTING accuracy (which skill fires)
 *   - scripts/aichat-matrix.ts (this)— END-TO-END behavior (right tool, right
 *                                      number of times, right effect)
 *
 * Under ADR-038 the model calls search_skills before acting, so assertions check
 * for the TARGET tool tolerating search_skills, never raw tool count.
 *
 * The daemon, socket, and DB are managed by the caller (see #1329 reproduce steps).
 * This script only talks to the already-running test daemon via scripts/aichat.ts,
 * reusing its log-scraping for tool calls. Skills must be seeded before running.
 *
 * Usage:
 *   NS_BIN=target/release/nodespace \
 *   NODESPACED_SOCKET=/tmp/nodespaced-test/daemon.sock \
 *   NS_LOG=/tmp/nodespaced-test/daemon.log \
 *   NS_MODEL=<id> NS_TIMEOUT_MS=240000 \
 *     bun run scripts/aichat-matrix.ts <label> [out.json]
 *     <label>    arbitrary tag stored in the results (e.g. "e4b" / "12b" / "31b")
 *     <out.json> path to write the results (default: /tmp/aichat-matrix-<label>-<ts>.json)
 *
 * Exits 0 if all scenario assertions pass, 1 otherwise.
 * Set AICHAT_MATRIX_BASELINE=path/to/baseline.json to compare against a recorded
 * baseline and fail on regression (a scenario that was passing and is now failing).
 *
 * Scenarios 4/6/8 depend on earlier turns, so the script keeps groups of turns on
 * the same chat node. Scenario 6 has no stable id up front, so it is phrased to let
 * the agent search-then-update by description.
 *
 * CONTAMINATION: scenario prompts must NOT appear in the agent's compiled guidance
 * (packages/agent/src/agent_guidance.rs, skill_rules.rs) — otherwise a pass measures
 * recall of a planted example rather than generalization. The Rust test
 * `guidance_is_not_contaminated_by_eval_prompts` parses the prompts out of this file
 * and fails the build if any reappears in guidance. When editing prompts here, keep
 * them phrased unlike the guidance rules; when editing guidance, don't paste prompts
 * from this file in as worked examples.
 *
 * Scenario 8's sub-labels ("book", "contact") name each step's ROLE in the
 * multi-type flow (first type, second type, instance of each) — not the literal
 * entity in the prompt. They are stable IDs for baseline diffing; the prompt wording
 * underneath them is free to change.
 */

import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// ---------------------------------------------------------------------------
// Tool registry (cross-referenced against Tool::ALL in
// packages/agent/src/local_agent/tools.rs — update here if the registry changes)
// ---------------------------------------------------------------------------

// Tools that participate in ADR-038 pull-model routing but are not the scenario's
// target action — tolerated anywhere in the sequence, never asserted as "extra".
const ROUTING_TOOLS = ["search_skills"];

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

interface ScenarioStep {
  scenario: string;
  prompt: string;
  expect: Expectation;
}

interface TurnResult {
  scenario: string;
  prompt: string;
  expect: Expectation;
  toolsOffered: string;
  toolsCalled: string[];
  reply: string;
  latencyMs: number;
  passed: boolean;
  failure?: string;
}

/** Run one `aichat.ts send <id> <msg>` turn; parse its stdout. */
function runTurn(aichatPath: string, chatId: string, message: string): {
  toolsOffered: string;
  toolsCalled: string[];
  reply: string;
  latencyMs: number;
} {
  const start = performance.now();
  const r = Bun.spawnSync(["bun", "run", aichatPath, "send", chatId, message], {
    stdout: "pipe",
    stderr: "pipe",
    env: { ...process.env },
  });
  const latencyMs = Math.round(performance.now() - start);
  const out = r.stdout.toString();
  const err = r.stderr.toString();
  if (r.exitCode !== 0) {
    return {
      toolsOffered: `(error: ${err.trim()})`,
      toolsCalled: [],
      reply: `(send failed: ${err.trim()})`,
      latencyMs,
    };
  }
  const toolsOffered = out.match(/\[tools offered\] (.*)/)?.[1]?.trim() ?? "";
  const toolsCalled = [...out.matchAll(/\[tool\] ([a-z_]+)/g)].map((m) => m[1]);
  const reply = out.match(/assistant> ([\s\S]*)$/)?.[1]?.trim() ?? "(no reply parsed)";
  return { toolsOffered, toolsCalled, reply, latencyMs };
}

/** Create a fresh chat node via aichat.ts; return its id. */
function newChat(aichatPath: string): string {
  const r = Bun.spawnSync(["bun", "run", aichatPath, "new"], {
    stdout: "pipe",
    stderr: "pipe",
    env: { ...process.env },
  });
  if (r.exitCode !== 0) throw new Error(`new failed: ${r.stderr.toString()}`);
  return r.stdout.toString().trim();
}

// ---------------------------------------------------------------------------
// Assertion logic
// ---------------------------------------------------------------------------

export function actionTools(toolsCalled: string[]): string[] {
  return toolsCalled.filter((t) => !ROUTING_TOOLS.includes(t));
}

export function assertExpectation(
  expect: Expectation,
  toolsCalled: string[],
): { passed: boolean; failure?: string } {
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
      return { passed: true };
    }
  }
}

// ---------------------------------------------------------------------------
// Scenario groups (issue #1329 matrix; structured per issue #1364)
// Each group shares a chat node so later turns see earlier context.
// ---------------------------------------------------------------------------

const GROUPS: ScenarioStep[][] = [
  [{ scenario: "1. Greeting", prompt: "Hi there", expect: { kind: "noTools" } }],
  [{ scenario: "2. Capability", prompt: "What can you do?", expect: { kind: "noTools" } }],
  // Single-custom-type CRUD chain (scenarios 3-7) shares one chat node.
  // Wording here must stay independent of packages/agent/src/agent_guidance.rs —
  // guidance_is_not_contaminated_by_eval_prompts (agent_guidance.rs) enforces it.
  [
    {
      scenario: "3. Schema creation",
      prompt: "I want to keep a record of the equipment my team checks out",
      expect: { kind: "noExtraTypes" },
    },
    {
      scenario: "4. Instance creation",
      prompt: "Log a laser cutter checked out on the 12th, replacement cost 2400",
      expect: { kind: "toolOnce", tool: "create_node" },
    },
    {
      scenario: "5. List/query",
      prompt: "What equipment is on the books?",
      expect: { kind: "toolOnce", tool: "search_nodes" },
    },
    {
      scenario: "6. Update",
      prompt: "The 2400 one came back — set it to returned",
      expect: { kind: "toolSequence", tools: ["search_nodes", "update_node"] },
    },
    {
      scenario: "7. Empty-result query",
      prompt: "Do we have anything worth 90000 sitting out?",
      expect: { kind: "noRetry", tool: "search_nodes" },
    },
  ],
  // Multi-custom-type CRUD (scenario 8) shares its own chat node.
  [
    {
      scenario: "8a. Create type: book",
      prompt: "Set up somewhere to note down albums I mean to listen to",
      expect: { kind: "toolOnce", tool: "create_schema" },
    },
    {
      scenario: "8b. Create type: contact",
      prompt: "I also need to keep track of the venues I book",
      expect: { kind: "toolOnce", tool: "create_schema" },
    },
    {
      scenario: "8c. Instance: book",
      prompt: "Put down Kind of Blue, it's by Miles Davis",
      expect: { kind: "toolOnce", tool: "create_node" },
    },
    {
      scenario: "8d. Instance: contact",
      prompt: "New venue: the Blue Note, they can be reached at booking@example.com",
      expect: { kind: "toolOnce", tool: "create_node" },
    },
    {
      scenario: "8e. Query across types",
      prompt: "Run through the albums for me",
      expect: { kind: "toolOnce", tool: "search_nodes" },
    },
  ],
];

// ---------------------------------------------------------------------------
// Main eval loop (CLI entrypoint only — not run when imported for tests)
// ---------------------------------------------------------------------------

if (import.meta.main) {
  const WORKTREE = resolve(dirname(fileURLToPath(import.meta.url)), "..");
  const AICHAT = join(WORKTREE, "scripts", "aichat.ts");

  const [label, outPathArg] = process.argv.slice(2);
  if (!label) {
    console.error(
      "usage: aichat-matrix.ts <label> [out.json]\n" +
        "  label    arbitrary tag stored in the results (e.g. 'e4b' / '12b' / '31b')\n" +
        "  out.json path to write results (default: /tmp/aichat-matrix-<label>-<ts>.json)",
    );
    process.exit(1);
  }

  const ts = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
  const outPath = outPathArg ?? `/tmp/aichat-matrix-${label}-${ts}.json`;
  const baselinePath = process.env.AICHAT_MATRIX_BASELINE;

  console.error(`[aichat-matrix] label=${label} model=${process.env.NS_MODEL ?? "(default)"}`);

  const results: TurnResult[] = [];
  for (const group of GROUPS) {
    const chatId = newChat(AICHAT);
    console.error(`[${label}] chat ${chatId} for: ${group.map((g) => g.scenario).join(", ")}`);
    for (const step of group) {
      console.error(`[${label}] → ${step.scenario}: ${step.prompt}`);
      const t = runTurn(AICHAT, chatId, step.prompt);
      const { passed, failure } = assertExpectation(step.expect, t.toolsCalled);
      results.push({ scenario: step.scenario, prompt: step.prompt, expect: step.expect, ...t, passed, failure });
      const mark = passed ? "✓" : "✗";
      console.error(`[${label}]   ${mark} tools=[${t.toolsCalled.join(",")}] ${t.latencyMs}ms`);
      if (!passed) console.error(`[${label}]     ↳ ${failure}`);
    }
  }

  const total = results.length;
  const failed = results.filter((r) => !r.passed).length;
  const passed = total - failed;

  await Bun.write(outPath, JSON.stringify({ label, model: process.env.NS_MODEL ?? "(default)", results }, null, 2));
  console.error(`[${label}] wrote ${results.length} results to ${outPath}`);

  // Print summary table
  console.log(`\n── Agent Eval Results ───────────────────────────────────────────────`);
  console.log(`   Label:   ${label}`);
  console.log(`   Model:   ${process.env.NS_MODEL ?? "(default)"}`);
  console.log(`   Passed:  ${passed}/${total}`);
  console.log(`────────────────────────────────────────────────────────────────────`);
  for (const r of results) {
    const mark = r.passed ? "✓" : "✗";
    console.log(`  ${mark} ${r.scenario}`);
    if (!r.passed) console.log(`      ↳ ${r.failure}`);
  }
  console.log(`────────────────────────────────────────────────────────────────────\n`);

  // -------------------------------------------------------------------------
  // Baseline comparison (optional)
  // -------------------------------------------------------------------------

  if (baselinePath) {
    try {
      const baseline: { results: TurnResult[] } = JSON.parse(await Bun.file(baselinePath).text());
      console.log(`── Baseline Comparison (vs ${baselinePath}) ──`);
      let regressions = 0;
      for (const cur of results) {
        const base = baseline.results.find((f) => f.scenario === cur.scenario);
        if (!base) {
          console.log(`  NEW  ${cur.scenario} → ${cur.passed ? "pass" : "fail"}`);
          continue;
        }
        if (base.passed && !cur.passed) {
          console.log(`  REGRESSION  ${cur.scenario}: was passing, now failing — ${cur.failure}`);
          regressions++;
        } else if (!base.passed && cur.passed) {
          console.log(`  FIXED  ${cur.scenario}: was failing, now passing`);
        }
      }
      if (regressions > 0) {
        console.error(`\n[aichat-matrix] ✗ ${regressions} regression(s) vs baseline — failing`);
        process.exit(1);
      } else {
        console.log(`\n[aichat-matrix] ✓ No regressions vs baseline`);
      }
    } catch (e) {
      console.error(`[aichat-matrix] Warning: could not read baseline at ${baselinePath}: ${e}`);
    }
  }

  // Fail the process if any scenario assertion failed (for CI)
  if (failed > 0) {
    console.error(`\n[aichat-matrix] ✗ ${failed}/${total} scenarios failed`);
    process.exit(1);
  } else {
    console.error(`\n[aichat-matrix] ✓ All ${total} scenarios passed`);
    process.exit(0);
  }
}
