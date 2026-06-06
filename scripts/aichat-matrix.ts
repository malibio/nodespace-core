#!/usr/bin/env bun
/**
 * aichat-matrix.ts — drive the issue #1329 A/B scenario matrix through aichat.ts.
 *
 * Runs the 8 scenarios from #1329 once against the model the daemon currently has
 * loaded, capturing per-scenario tools-offered, tools-called, the assistant reply,
 * and wall-clock latency into a JSON results file. Run it once per model to A/B/C
 * (E4B vs 12B vs 31B).
 *
 * The daemon, socket, and DB are managed by the caller (see #1329 reproduce steps).
 * This script only talks to the already-running test daemon via scripts/aichat.ts,
 * reusing its log-scraping for tool calls.
 *
 * Usage:
 *   NS_BIN=target/release/nodespace \
 *   NODESPACED_SOCKET=/tmp/nodespaced-test/daemon.sock \
 *   NS_LOG=/tmp/nodespaced-test/daemon.log \
 *   NS_MODEL=<id> NS_TIMEOUT_MS=240000 \
 *     bun run scripts/aichat-matrix.ts <label> <out.json>
 *     <label>    arbitrary tag stored in the results (e.g. "e4b" / "12b" / "31b")
 *     <out.json> path to write the results array
 *
 * Scenarios 4/6/8 depend on earlier turns, so the script keeps groups of turns on
 * the same chat node. Scenario 6 ("Mark invoice ... as paid") has no stable id up
 * front, so it is phrased to let the agent search-then-update by description.
 */

import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const WORKTREE = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const AICHAT = join(WORKTREE, "scripts", "aichat.ts");

const [label, outPath] = process.argv.slice(2);
if (!label || !outPath) {
  console.error("usage: aichat-matrix.ts <label> <out.json>");
  process.exit(1);
}

interface TurnResult {
  scenario: string;
  prompt: string;
  expect: string;
  toolsOffered: string;
  toolsCalled: string[];
  reply: string;
  latencyMs: number;
}

/** Run one `aichat.ts send <id> <msg>` turn; parse its stdout. */
function runTurn(chatId: string, message: string): {
  toolsOffered: string;
  toolsCalled: string[];
  reply: string;
  latencyMs: number;
} {
  const start = performance.now();
  const r = Bun.spawnSync(["bun", "run", AICHAT, "send", chatId, message], {
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
function newChat(): string {
  const r = Bun.spawnSync(["bun", "run", AICHAT, "new"], {
    stdout: "pipe",
    stderr: "pipe",
    env: { ...process.env },
  });
  if (r.exitCode !== 0) throw new Error(`new failed: ${r.stderr.toString()}`);
  return r.stdout.toString().trim();
}

// Scenario groups. Each group shares a chat node so later turns see earlier context.
const GROUPS: Array<Array<{ scenario: string; prompt: string; expect: string }>> = [
  [{ scenario: "1. Greeting", prompt: "Hi there", expect: "no tools, short reply" }],
  [{ scenario: "2. Capability", prompt: "What can you do?", expect: "no tools, concise, no trailing reasoning" }],
  // Invoice CRUD chain (scenarios 3-7) shares one chat node.
  [
    { scenario: "3. Schema creation", prompt: "Create an invoice tracking database", expect: "exactly one create_schema, no extra related types" },
    { scenario: "4. Instance creation", prompt: "Add an invoice for $500 due next Friday", expect: "create instance against new schema" },
    { scenario: "5. List/query", prompt: "List all my invoices", expect: "search_nodes with type filter" },
    { scenario: "6. Update", prompt: "Mark the $500 invoice as paid", expect: "search-then-update" },
    { scenario: "7. Empty-result query", prompt: "Find my invoice for one million dollars", expect: "single statement, no retry" },
  ],
  // Multi-custom-type CRUD (scenario 8) shares its own chat node.
  [
    { scenario: "8a. Create type: book", prompt: "Create a database to track books I want to read", expect: "one create_schema" },
    { scenario: "8b. Create type: contact", prompt: "Create a database for my contacts", expect: "one create_schema" },
    { scenario: "8c. Instance: book", prompt: 'Add a book called "Dune" by Frank Herbert', expect: "create instance" },
    { scenario: "8d. Instance: contact", prompt: "Add a contact named Jane Doe, email jane@example.com", expect: "create instance" },
    { scenario: "8e. Query across types", prompt: "Show me all my books", expect: "search_nodes book type filter" },
  ],
];

const results: TurnResult[] = [];
for (const group of GROUPS) {
  const chatId = newChat();
  console.error(`[${label}] chat ${chatId} for: ${group.map((g) => g.scenario).join(", ")}`);
  for (const step of group) {
    console.error(`[${label}] → ${step.scenario}: ${step.prompt}`);
    const t = runTurn(chatId, step.prompt);
    results.push({ scenario: step.scenario, prompt: step.prompt, expect: step.expect, ...t });
    console.error(`[${label}]   tools=[${t.toolsCalled.join(",")}] ${t.latencyMs}ms`);
  }
}

await Bun.write(outPath, JSON.stringify({ label, model: process.env.NS_MODEL ?? "(default)", results }, null, 2));
console.error(`[${label}] wrote ${results.length} results to ${outPath}`);
