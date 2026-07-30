/**
 * Shared eval harness.
 *
 * Owns everything an eval does NOT need to decide for itself: the environment
 * contract, the preflight gate, argv, chat-node lifecycle, results assembly,
 * the summary table, baseline diffing, and exit codes. An eval supplies only
 * its scenarios and how to score a turn (see ./types.ts, ./fixtures/).
 *
 * Adding an eval is therefore a fixture module, not another copy of this
 * plumbing — which is what let the two original harnesses drift into having
 * separately-implemented, separately-broken versions of the same five concerns.
 */

import { readEnv, ENV_USAGE, REPO_ROOT, type EvalEnv } from "./env.ts";
import {
  abortOnEnvironment,
  preflight,
  readDaemonStatus,
  readGuidanceProvenance,
  EnvironmentError,
  EXIT_FAILED,
  EXIT_USAGE,
} from "./preflight.ts";
import type {
  EvalFixture,
  EvalResults,
  Provenance,
  Scenario,
  ScenarioResult,
  ToolCallRecord,
  TurnRecord,
} from "./types.ts";

// ---------------------------------------------------------------------------
// aichat.ts driver
// ---------------------------------------------------------------------------

/**
 * Create a fresh chat node; return its id.
 *
 * Failing here means the daemon went away mid-run — preflight already proved it
 * was reachable. That is an environment failure, not a scenario failure, and
 * must not be reported (or exit) as one: the scenarios scored before the daemon
 * died would otherwise look like a partial result rather than a void run.
 */
function newChat(env: EvalEnv): string {
  const r = Bun.spawnSync(["bun", "run", env.aichat, "new"], {
    stdout: "pipe",
    stderr: "pipe",
    env: { ...process.env },
  });
  if (r.exitCode !== 0) {
    throw new EnvironmentError(
      `The daemon stopped responding partway through the run: could not create a ` +
        `chat node.\n  ${r.stderr.toString().trim()}`,
      `Check that the daemon on ${env.socket} is still alive (and did not run out ` +
        `of memory or get killed).\n  Partial scores from an aborted run are not ` +
        `comparable to a complete one, so none were written.`,
    );
  }
  return r.stdout.toString().trim();
}

/**
 * Run one turn and scrape its outcome.
 *
 * A failed send is recorded rather than thrown, so one flaky turn does not
 * abandon a run that costs minutes of inference. It is flagged `sendFailed` so
 * the caller can tell it apart from a turn that genuinely called no tools —
 * scoring the two alike is what lets a dead daemon pass a "no tools" assertion.
 * The run loop aborts once sends fail consecutively.
 */
function runTurn(env: EvalEnv, chatId: string, message: string): TurnRecord {
  const start = performance.now();
  const r = Bun.spawnSync(["bun", "run", env.aichat, "send", chatId, message], {
    stdout: "pipe",
    stderr: "pipe",
    env: { ...process.env },
  });
  const latencyMs = Math.round(performance.now() - start);

  if (r.exitCode !== 0) {
    const err = r.stderr.toString().trim();
    return {
      toolsOffered: `(error: ${err})`,
      toolsCalled: [],
      reply: `(send failed: ${err})`,
      latencyMs,
      sendFailed: true,
    };
  }

  const out = r.stdout.toString();

  // One pass over the [tool] lines feeds both shapes, so they cannot disagree
  // about how many calls a turn made. aichat.ts emits:
  //   [tool] <name>[ERROR][ [fields=N]] <args>
  // The args are free-form and truncated, so every structured marker precedes
  // them and nothing here parses them.
  const toolCalls: ToolCallRecord[] = [
    ...out.matchAll(/\[tool\] ([a-z_]+)( \[ERROR\])?( \[fields=(\d+)\])?/g),
  ].map((m) => ({
    name: m[1],
    isError: m[2] !== undefined,
    ...(m[4] === undefined ? {} : { fieldCount: Number(m[4]) }),
  }));

  // Raw generations, one per ReAct iteration:  [raw] iteration=N <text>
  // Joined in iteration order so a multi-round turn's trace reads as one
  // transcript rather than requiring the reader to re-sort log lines.
  const rawLines = [...out.matchAll(/^\[raw\] iteration=(\d+) ([\s\S]*?)(?=\n\[raw\] iteration=\d+ |\n\[tool\]|\n$|$)/gm)];
  const rawOutput =
    rawLines.length > 0
      ? rawLines.map((m) => `[iteration ${m[1]}] ${m[2].trim()}`).join("\n")
      : undefined;

  return {
    toolsOffered: out.match(/\[tools offered\] (.*)/)?.[1]?.trim() ?? "",
    toolsCalled: toolCalls.map((t) => t.name),
    toolCalls,
    reply:
      out.match(/assistant> ([\s\S]*)$/)?.[1]?.trim() ?? "(no reply parsed)",
    latencyMs,
    routingDecision: out.match(/\[routing\] ([a-z_]+)/)?.[1],
    stage2CandidatesInjected: (() => {
      const m = out.match(/\[stage2 injected\] (true|false)/);
      return m ? m[1] === "true" : undefined;
    })(),
    rawOutput,
    emptyGeneration: out.includes("[empty-generation]") || undefined,
  };
}

// ---------------------------------------------------------------------------
// Provenance
// ---------------------------------------------------------------------------

function gitCommit(): { commit: string; dirty: boolean } {
  const rev = Bun.spawnSync(
    ["git", "-C", REPO_ROOT, "rev-parse", "--short", "HEAD"],
    {
      stdout: "pipe",
      stderr: "pipe",
    },
  );
  const status = Bun.spawnSync(
    ["git", "-C", REPO_ROOT, "status", "--porcelain"],
    {
      stdout: "pipe",
      stderr: "pipe",
    },
  );
  return {
    commit: rev.exitCode === 0 ? rev.stdout.toString().trim() : "(unknown)",
    dirty: status.exitCode === 0 && status.stdout.toString().trim().length > 0,
  };
}

// ---------------------------------------------------------------------------
// Baseline comparison
// ---------------------------------------------------------------------------

/**
 * Compare against a recorded baseline. Returns the regression count.
 *
 * Joins on scenario `id`, not the prompt text: prompts get reworded (the
 * decontamination pass rewrote every one of them) and joining on text would
 * report the whole suite as removed-and-added.
 */
async function compareToBaseline(
  evalName: string,
  results: ScenarioResult[],
  baselinePath: string,
): Promise<number> {
  let baseline: EvalResults;
  try {
    baseline = JSON.parse(await Bun.file(baselinePath).text());
  } catch (e) {
    // A baseline that cannot be read is an operator error worth surfacing, but
    // it must not discard a run that took minutes of inference to produce.
    console.error(
      `[${evalName}] Warning: could not read baseline at ${baselinePath}: ${e}`,
    );
    return 0;
  }

  const p = baseline.provenance;
  console.log(`── Baseline comparison (vs ${baselinePath}) ──`);
  if (p) {
    console.log(
      `   Recorded: ${p.recordedAt} · model ${p.model} · n_ctx ${p.nCtx}`,
    );
  }

  const byId = new Map((baseline.results ?? []).map((r) => [r.id, r]));
  let regressions = 0;

  for (const cur of results) {
    const base = byId.get(cur.id);
    if (!base) {
      console.log(`   NEW         ${cur.id} → ${cur.passed ? "pass" : "fail"}`);
      continue;
    }
    // An empty-generation exclusion is an inference bug, not a scoring
    // outcome — it carries `passed: false` only so older tooling degrades
    // safely, and must not be compared against a baseline verdict as if it
    // were one. Otherwise every run with a stray empty generation reports a
    // spurious REGRESSION on a scenario the model was never actually scored
    // against this time.
    if (cur.excludedAsEmptyGeneration) {
      console.log(
        `   EXCLUDED    ${cur.id}: degenerate empty generation this run — not compared`,
      );
      continue;
    }
    if (base.passed && !cur.passed) {
      console.log(
        `   REGRESSION  ${cur.id}: was passing, now failing — ${cur.failure}`,
      );
      regressions++;
    } else if (!base.passed && cur.passed) {
      console.log(`   FIXED       ${cur.id}: was failing, now passing`);
    }
  }

  for (const [id] of byId) {
    if (!results.some((r) => r.id === id)) {
      console.log(`   REMOVED     ${id}: in baseline, not in this run`);
    }
  }

  console.log(
    regressions > 0
      ? `\n[${evalName}] ✗ ${regressions} regression(s) vs baseline`
      : `\n[${evalName}] ✓ No regressions vs baseline`,
  );
  return regressions;
}

// ---------------------------------------------------------------------------
// Scoring/reporting helpers — pure, so they are unit-testable without a
// daemon (see scripts/eval/runner.test.ts).
// ---------------------------------------------------------------------------

/**
 * Split a run's results into scenarios that were actually scored and those
 * excluded as degenerate empty generations (see `TurnRecord.emptyGeneration`).
 *
 * Kept out of `runEval`'s body so uniformity/totals math is independently
 * testable against a fixed `ScenarioResult[]`, without spawning a daemon.
 */
export function partitionExcluded(results: ScenarioResult[]): {
  scored: ScenarioResult[];
  excludedCount: number;
} {
  const scored = results.filter((r) => !r.excludedAsEmptyGeneration);
  return { scored, excludedCount: results.length - scored.length };
}

/**
 * Decide whether a scored run's pass rate is uniform enough to be a harness
 * signature rather than a result. Returns `null` when the run is fine, or an
 * `EnvironmentError` when it should abort.
 *
 * A pure decision function (as opposed to inlining the check + throw in
 * `runEval`) so the threshold and both boundary cases — one scenario under
 * `minScenarios`, and a genuine partial result — are covered by a unit test
 * that never spawns a daemon.
 */
export function checkUniformity(
  passed: number,
  total: number,
  minScenarios = 4,
): EnvironmentError | null {
  if (total < minScenarios) return null;
  if (passed !== 0 && passed !== total) return null;
  return new EnvironmentError(
    `Every scored scenario ${passed === 0 ? "FAILED" : "PASSED"} (${passed}/${total}). ` +
      `A rate this uniform across an entire run is a harness signature, not a result — ` +
      `real runs on this suite have never been perfectly uniform, and the two known false ` +
      `results on record (contaminated 11/12, and the "no tools called" scenarios passing ` +
      `while every turn died on context overflow) both looked like plausible numbers until ` +
      `someone read the raw output.`,
    `Read a raw generation before trusting this number: re-run with the daemon at ` +
      `RUST_LOG=debug and inspect the [raw] lines this eval now captures (see rawOutput on ` +
      `each turn), or check the daemon log directly for an error common to every turn.\n` +
      `  No results file was written for this run.`,
  );
}

/** One line of the raw-output JSONL trace. */
export interface TraceLine {
  scenarioId: string;
  turnIndex: number;
  isPriorContext: boolean;
  rawOutput: string;
  routingDecision?: string;
  stage2CandidatesInjected?: boolean;
  toolsCalled: string[];
}

/**
 * Build the raw-output trace lines for a run's results.
 *
 * Only turns that actually captured `rawOutput` produce a line — a turn run
 * against a daemon without `RUST_LOG=debug` simply contributes nothing,
 * rather than a null placeholder.
 */
export function buildTraceLines(results: ScenarioResult[]): TraceLine[] {
  const lines: TraceLine[] = [];
  for (const r of results) {
    for (const [idx, t] of r.turns.entries()) {
      if (t.rawOutput === undefined) continue;
      lines.push({
        scenarioId: r.id,
        turnIndex: idx,
        isPriorContext: idx < r.turns.length - 1,
        rawOutput: t.rawOutput,
        routingDecision: t.routingDecision,
        stage2CandidatesInjected: t.stage2CandidatesInjected,
        toolsCalled: t.toolsCalled,
      });
    }
  }
  return lines;
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

function usage(fixture: EvalFixture): string {
  return (
    `usage: bun run scripts/eval/${fixture.name}.ts <label> [out.json] [--baseline <path>]\n\n` +
    `  ${fixture.description}\n\n` +
    `  label        tag recorded in the results (e.g. 'e4b')\n` +
    `  out.json     where to write results (default: /tmp/${fixture.name}-<label>-<ts>.json)\n` +
    `  --baseline   compare against a recorded run and fail on regression\n\n` +
    ENV_USAGE +
    `\n\nExit codes: 0 all passed · 1 scenario failure/regression · 2 environment unusable · 64 usage`
  );
}

/**
 * Run an eval end to end. Call this from a fixture's CLI wrapper; it owns the
 * process lifetime and exits rather than returning.
 */
export async function runEval(fixture: EvalFixture): Promise<never> {
  const argv = process.argv.slice(2);

  const baselineFlag = argv.indexOf("--baseline");
  let baselinePath: string | undefined;
  if (baselineFlag !== -1) {
    baselinePath = argv[baselineFlag + 1];
    if (!baselinePath) {
      console.error(`--baseline needs a path\n\n${usage(fixture)}`);
      process.exit(EXIT_USAGE);
    }
    argv.splice(baselineFlag, 2);
  }

  const [label, outPathArg] = argv;
  if (!label) {
    console.error(usage(fixture));
    process.exit(EXIT_USAGE);
  }

  const env = readEnv();

  // Preflight BEFORE any scenario runs. An environment failure must never be
  // reported as scenario results, so this precedes both the run and the file.
  const status = (() => {
    try {
      const s = readDaemonStatus(env);
      preflight(env, s);
      return s;
    } catch (e) {
      if (e instanceof EnvironmentError) abortOnEnvironment(fixture.name, e);
      throw e;
    }
  })();

  const ts = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
  const outPath = outPathArg ?? `/tmp/${fixture.name}-${label}-${ts}.json`;
  const { commit, dirty } = gitCommit();
  const guidance = readGuidanceProvenance(env);

  const provenance: Provenance = {
    model: status.modelId,
    recordedAt: new Date().toISOString(),
    hostMemoryGb: Number((status.hostRamBytes / 1e9).toFixed(1)),
    nCtx: status.grantedNCtx,
    ...(status.modelMatchedByPath ? { modelMatchedByPath: true } : {}),
    evalCommit: commit,
    dirty,
    guidance,
  };

  console.error(
    `[${fixture.name}] label=${label} model=${status.modelId} n_ctx=${status.grantedNCtx}` +
      (dirty ? " (working tree dirty)" : ""),
  );
  for (const [nodeType, entries] of Object.entries(guidance)) {
    console.error(
      `[${fixture.name}] guidance seeded (${nodeType}): ${
        entries.length === 0
          ? "none found — cannot confirm what this run measured"
          : entries.map((e) => `${e.key}@${e.version}`).join(", ")
      }`,
    );
  }

  // -------------------------------------------------------------------------
  // Run
  // -------------------------------------------------------------------------

  const results: ScenarioResult[] = [];

  /**
   * Consecutive failed sends tolerated before the run is declared void.
   *
   * Preflight proves the environment was usable at t=0; it cannot prove it
   * stayed usable across the minutes a run takes. If the daemon dies mid-run
   * every subsequent turn calls no tools, and every negative assertion scores
   * as a PASS — the exact false result this harness exists to prevent, just
   * relocated past the gate. One failure can be a flaky turn and is a real
   * scenario failure; two in a row is the environment.
   */
  const MAX_CONSECUTIVE_SEND_FAILURES = 2;
  let consecutiveSendFailures = 0;

  /**
   * Run a turn and count it against the consecutive-failure budget.
   *
   * Every turn goes through here, prior-context turns included. A guard on only
   * the scored turn would leave the identical hole one call site over: a prior
   * turn that never reached the model leaves its scenario running against a
   * chat that never received its setup, and the resulting verdict gets filed as
   * the model's behavior rather than as an environment artifact.
   */
  const turn = (chatId: string, message: string): TurnRecord => {
    const t = runTurn(env, chatId, message);
    if (!t.sendFailed) {
      consecutiveSendFailures = 0;
      return t;
    }
    consecutiveSendFailures++;
    if (consecutiveSendFailures >= MAX_CONSECUTIVE_SEND_FAILURES) {
      throw new EnvironmentError(
        `${consecutiveSendFailures} turns in a row failed to send — the daemon ` +
          `went away partway through the run.\n  Last error: ${t.reply}`,
        `Check whether the daemon on ${env.socket} is still alive (it may have ` +
          `run out of memory or been killed).\n  ${results.length} scenario(s) had ` +
          `been scored; they are NOT written, because scores from a run that died ` +
          `midway are not comparable to a complete one — and turns that never ` +
          `reached the model would score "no tools called" assertions as passes.`,
      );
    }
    return t;
  };

  try {
    for (const group of fixture.groups) {
      const chatId = newChat(env);
      console.error(
        `[${fixture.name}] chat ${chatId} for: ${group.map((s) => s.id).join(", ")}`,
      );

      for (const scenario of group) {
        console.error(`[${fixture.name}] → ${scenario.scenario}`);

        // Prior turns establish context and are never scored.
        const priorTurns: TurnRecord[] = [];
        for (const prior of scenario.priorTurns ?? []) {
          console.error(`[${fixture.name}]   [context] ${prior}`);
          priorTurns.push(turn(chatId, prior));
        }

        const scored = turn(chatId, scenario.prompt);

        // The degenerate-empty-generation failure mode (agent_loop.rs: the
        // model opens a turn and emits neither text nor a tool call) is an
        // inference bug, not a scenario outcome — scoring it as a failure
        // silently deflates every cell it lands in, exactly the harness-vs-model
        // confusion this eval exists to prevent. Excluded from the denominator
        // rather than scored, but still recorded with its turn data so the rate
        // of empty generations stays visible rather than vanishing silently.
        if (scored.emptyGeneration) {
          results.push({
            id: scenario.id,
            scenario: scenario.scenario,
            prompt: scenario.prompt,
            passed: false,
            failure: "excluded: degenerate empty generation (no text, no tool call)",
            turns: [...priorTurns, scored],
            extra: fixture.extra?.(scenario, [scored]),
            excludedAsEmptyGeneration: true,
          });
          console.error(
            `[${fixture.name}]   ⊘ excluded (empty generation) ${scored.latencyMs}ms`,
          );
          continue;
        }

        const verdict = fixture.score(scenario, [scored]);

        results.push({
          id: scenario.id,
          scenario: scenario.scenario,
          prompt: scenario.prompt,
          passed: verdict.passed,
          failure: verdict.failure,
          turns: [...priorTurns, scored],
          extra: fixture.extra?.(scenario, [scored]),
        });

        console.error(
          `[${fixture.name}]   ${verdict.passed ? "✓" : "✗"} ` +
            `tools=[${scored.toolsCalled.join(",")}] ${scored.latencyMs}ms`,
        );
        if (!verdict.passed)
          console.error(`[${fixture.name}]     ↳ ${verdict.failure}`);
      }
    }
  } catch (e) {
    if (e instanceof EnvironmentError) abortOnEnvironment(fixture.name, e);
    throw e;
  }

  // -------------------------------------------------------------------------
  // Report
  // -------------------------------------------------------------------------

  const { scored, excludedCount: excludedEmptyGenerations } =
    partitionExcluded(results);
  const total = scored.length;
  const passed = scored.filter((r) => r.passed).length;
  const failed = total - passed;

  // Uniform 0% or 100% across every SCORED scenario is a harness signature —
  // an environment that preflight could not catch (e.g. every send silently
  // routing to a dead model behind a load balancer, or every turn hitting the
  // same unhandled code path) rather than a real result. Excluded scenarios
  // are not counted toward "every": a run that is all empty-generations is
  // reported by excludedEmptyGenerations instead, and one that is otherwise a
  // real 0/1 or 1/1 must not trip this on a single-scenario smoke test.
  const uniformityError = checkUniformity(passed, total);
  if (uniformityError) abortOnEnvironment(fixture.name, uniformityError);

  const evalResults: EvalResults = {
    eval: fixture.name,
    label,
    provenance,
    summary: { total, passed, failed, excludedEmptyGenerations },
    results,
  };

  await Bun.write(outPath, JSON.stringify(evalResults, null, 2));
  console.error(`[${fixture.name}] wrote ${total} results to ${outPath}`);

  // Raw-output JSONL trace: one line per scored turn that captured raw
  // generation text, alongside the results JSON so a scenario that needs
  // investigating never requires a re-run to see what the model actually
  // said. Absent turns (no RUST_LOG=debug on the daemon) are simply skipped
  // rather than padded with nulls.
  const tracePath = outPath.replace(/\.json$/, ".trace.jsonl");
  const traceLines = buildTraceLines(results).map((l) => JSON.stringify(l));
  if (traceLines.length > 0) {
    await Bun.write(tracePath, traceLines.join("\n") + "\n");
    console.error(
      `[${fixture.name}] wrote ${traceLines.length} raw-output trace line(s) to ${tracePath}`,
    );
  } else {
    console.error(
      `[${fixture.name}] no raw-output trace written — daemon was not run with RUST_LOG=debug ` +
        `(or an equivalent filter including "nodespace_agent" at debug level)`,
    );
  }

  console.log(`\n── ${fixture.description} ─────────────────────────────────`);
  console.log(`   Label:    ${label}`);
  console.log(`   Model:    ${provenance.model}`);
  console.log(
    `   Context:  n_ctx ${provenance.nCtx} · host RAM ${provenance.hostMemoryGb} GB`,
  );
  console.log(
    `   Commit:   ${provenance.evalCommit}${provenance.dirty ? " (dirty)" : ""}`,
  );
  console.log(`   Passed:   ${passed}/${total}`);
  if (excludedEmptyGenerations > 0) {
    console.log(
      `   Excluded: ${excludedEmptyGenerations} (degenerate empty generation — not scored either way)`,
    );
  }
  for (const line of fixture.summary?.(results) ?? []) {
    console.log(`   ${line}`);
  }
  console.log(
    `────────────────────────────────────────────────────────────────────`,
  );
  for (const r of results) {
    const marker = r.excludedAsEmptyGeneration ? "⊘" : r.passed ? "✓" : "✗";
    console.log(`  ${marker} ${r.id}`);
    if (!r.passed) console.log(`      ↳ ${r.failure}`);
  }
  console.log(
    `────────────────────────────────────────────────────────────────────\n`,
  );

  let regressions = 0;
  if (baselinePath) {
    regressions = await compareToBaseline(fixture.name, results, baselinePath);
  }

  if (failed > 0 || regressions > 0) {
    console.error(`\n[${fixture.name}] ✗ ${failed}/${total} scenarios failed`);
    process.exit(EXIT_FAILED);
  }
  console.error(`\n[${fixture.name}] ✓ All ${total} scenarios passed`);
  process.exit(0);
}
