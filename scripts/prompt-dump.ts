#!/usr/bin/env bun
/**
 * prompt-dump.ts — read and pretty-print an agent prompt/response dump.
 *
 * The local agent can write the EXACT assembled prompt + raw model response per
 * ReAct iteration to a JSONL file when `NODESPACE_PROMPT_DUMP` is set (see
 * packages/agent/src/local_agent/prompt_dump.rs). The daemon log only records the
 * system-prompt *length* and a short preview, which is insufficient for
 * diagnosing prompt/tool-call issues. This reader renders the dump so you can see
 * verbatim what reached the model and what it produced.
 *
 * Workflow:
 *   1. Run the daemon with dumping on:
 *        NODESPACE_PROMPT_DUMP=/tmp/dump.jsonl <launch nodespaced>
 *   2. Drive a turn (e.g. `bun run chat ask "Create an invoice database"`).
 *   3. Inspect:
 *        bun run prompt:dump /tmp/dump.jsonl            # full, pretty
 *        bun run prompt:dump /tmp/dump.jsonl --prompt   # system prompts only
 *        bun run prompt:dump /tmp/dump.jsonl --tools    # tools offered only
 *        bun run prompt:dump /tmp/dump.jsonl --last     # only the most recent turn
 *        bun run prompt:dump /tmp/dump.jsonl --raw      # raw JSONL lines
 *
 * Each line in the dump is one JSON object:
 *   {"kind":"turn","iteration":N,"system_prompt":"…","messages":[…],"tools":[…]}
 *   {"kind":"response","iteration":N,"raw_response":"…","tool_calls":[…]}
 */

import { readFileSync } from "node:fs";

type TurnRecord = {
  kind: "turn";
  session_id: string;
  iteration: number;
  user_message: string;
  system_prompt: string;
  system_prompt_len: number;
  messages: Array<{ role: string; content: string }>;
  tools: Array<{ name: string; description: string; parameters?: unknown }>;
};

type ResponseRecord = {
  kind: "response";
  session_id: string;
  iteration: number;
  raw_response: string;
  raw_response_len: number;
  tool_calls: Array<{ id?: string; name?: string; arguments?: string }>;
};

type Record = TurnRecord | ResponseRecord;

const args = process.argv.slice(2);
const file = args.find((a) => !a.startsWith("--"));
const flags = new Set(args.filter((a) => a.startsWith("--")));

if (!file) {
  console.error(
    "usage: bun run prompt:dump <dump.jsonl> [--prompt|--tools|--last|--raw]",
  );
  process.exit(1);
}

let lines: string[];
try {
  lines = readFileSync(file, "utf8").trim().split("\n").filter(Boolean);
} catch (e) {
  console.error(`Cannot read ${file}: ${(e as Error).message}`);
  process.exit(1);
}

if (flags.has("--raw")) {
  for (const l of lines) console.log(l);
  process.exit(0);
}

let records: Record[] = lines.map((l) => JSON.parse(l));

// --last: keep only records from the most recent session.
if (flags.has("--last")) {
  const lastSession = [...records].reverse().find((r) => r.session_id)?.session_id;
  records = records.filter((r) => r.session_id === lastSession);
}

const rule = "=".repeat(72);

for (const r of records) {
  if (r.kind === "turn") {
    if (flags.has("--tools")) {
      if (r.iteration === 0) {
        console.log(`\n${rule}\nTOOLS OFFERED (${r.tools.length})`);
        for (const t of r.tools) {
          console.log(`  • ${t.name}: ${t.description.slice(0, 90)}`);
        }
      }
      continue;
    }
    if (flags.has("--prompt")) {
      console.log(
        `\n${rule}\nTURN iter=${r.iteration} | system_prompt_len=${r.system_prompt_len}\n${rule}`,
      );
      console.log(r.system_prompt);
      continue;
    }
    // default: full
    console.log(
      `\n${rule}\nTURN iter=${r.iteration} | user: ${r.user_message}`,
    );
    console.log(`system_prompt_len=${r.system_prompt_len}`);
    console.log("--- messages sent ---");
    for (const m of r.messages) {
      const c = m.content.length > 200 ? m.content.slice(0, 200) + "…" : m.content;
      console.log(`  [${m.role}] ${JSON.stringify(c)}`);
    }
    if (r.iteration === 0 && r.tools.length) {
      console.log(`--- tools (${r.tools.length}) ---`);
      for (const t of r.tools) console.log(`  • ${t.name}`);
    }
  } else if (r.kind === "response") {
    if (flags.has("--prompt") || flags.has("--tools")) continue;
    console.log(`--- RESPONSE iter=${r.iteration} (raw_len=${r.raw_response_len}) ---`);
    if (r.raw_response) {
      const raw =
        r.raw_response.length > 300 ? r.raw_response.slice(0, 300) + "…" : r.raw_response;
      console.log(`  raw: ${JSON.stringify(raw)}`);
    }
    for (const tc of r.tool_calls) {
      console.log(`  → tool_call ${tc.name}: ${(tc.arguments ?? "").slice(0, 400)}`);
    }
  }
}
